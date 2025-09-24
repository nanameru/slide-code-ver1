use crate::tui::Tui;
use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, WidgetRef},
};
use std::io::Write as _;
use std::{io, path::PathBuf, time::Instant};
use tokio::time::{sleep, Duration};

use crate::agent::AgentHandle;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chat_widget::{ChatWidget, ChatWidgetInit};
use crate::tui::FrameRequester;
use crate::history_cell::{HistoryCell, SystemLabel};
use crate::insert_history::insert_history_lines;
use crate::streaming::controller::AppEventHistorySink;
use crate::user_approval_widget::ApprovalRequest;
use crate::widgets::banner::banner_history_lines;
use crate::pager_overlay::PagerOverlay;
use crate::model_presets::builtin_model_presets;
use crate::approval_presets::builtin_approval_presets;
use crate::settings;
use slide_core::codex::Event as CoreEvent;
use slide_core::codex::ToolKind as CoreToolKind;
use slide_core::codex::ToolStream as CoreToolStream;
use slide_core::codex::Op;
use slide_core::protocol::InputItem;
use slide_core::protocol::ReasoningEffort as ReasoningEffortConfig;
use slide_core::protocol::{CoreAskForApproval as AskForApproval, CoreSandboxPolicy as SandboxPolicy};

// (leftover from earlier spinner impl) — intentionally removed

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Insert,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunStatus {
    Idle,
    Running,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopupKind {
    Command,
    FileSearch,
}

#[derive(Debug)]
pub enum AppExit {
    Quit,
    Preview(PathBuf),
}

#[derive(Debug)]
pub struct RunResult {
    pub exit: AppExit,
    pub recent_files: Vec<String>,
}

/// Simplified application state with ChatWidget delegation
pub struct App {
    should_quit: bool,
    mode: Mode,
    status: RunStatus,
    last_tick: Instant,
    
    // ChatWidget delegation (codex-1 style)
    chat_widget: ChatWidget,
    
    // UI state
    show_modal: bool,
    modal_title: String,
    modal_body: String,
    
    // Popup state
    active_popup: Option<PopupKind>,
    popup_title: String,
    popup_items: Vec<String>,
    popup_filtered_indices: Vec<usize>,
    popup_selected: usize,
    popup_filter: String,
    
    // Next action
    preview_path: Option<PathBuf>,
    // MRU files
    recent_files: Vec<String>,
    
    // App event channel
    app_event_rx: tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    app_event_tx: AppEventSender,
    
    // Thinking... spinner state
    thinking_frame_idx: usize,
    thinking_last_change: Instant,
    
    // Transcript/Diff overlay
    overlay: PagerOverlay,
    
    // --- Tool/Exec rendering state (codex-like grouping) ---
    pending_exec_block: Option<Vec<String>>,
    
    // --- Chat history spacing ---
    has_emitted_history: bool,
    last_message_was_assistant: bool,
    pending_tool_block: Option<Vec<String>>,
    pending_exec_started_at: Option<Instant>,
    pending_tool_started_at: Option<Instant>,
}

impl App {
    // Chat management methods delegated to ChatWidget
    // new() method removed - use new_with_recents with frame_requester

    // total_chat_lines delegated to ChatWidget

    pub fn new_with_recents(recent_files: Vec<String>, frame_requester: FrameRequester) -> Self {
        let (app_tx_raw, app_rx) = tokio::sync::mpsc::unbounded_channel();
        let app_tx = AppEventSender::new(app_tx_raw);
        
        // Create ChatWidget with delegation
        let chat_widget_init = ChatWidgetInit {
            app_event_tx: app_tx.clone(),
            agent: None,
            initial_prompt: None,
            initial_images: Vec::new(),
            frame_requester,
        };
        let chat_widget = ChatWidget::new(chat_widget_init);
        
        let s = Self {
            should_quit: false,
            mode: Mode::Normal,
            status: RunStatus::Idle,
            last_tick: Instant::now(),
            
            // ChatWidget delegation
            chat_widget,
            
            show_modal: false,
            modal_title: "Help".into(),
            modal_body: "Keybindings:\n- i: Insert (compose)\n- Esc: Normal\n- Enter: Send message\n- h: Toggle help modal\n- c: Clear messages\n- q: Quit".into(),
            active_popup: None,
            popup_title: String::new(),
            popup_items: Vec::new(),
            popup_filtered_indices: Vec::new(),
            popup_selected: 0,
            popup_filter: String::new(),
            preview_path: None,
            recent_files,
            app_event_rx: app_rx,
            app_event_tx: app_tx,
            thinking_frame_idx: 0,
            thinking_last_change: Instant::now(),
            overlay: PagerOverlay::new(),
            pending_exec_block: None,
            has_emitted_history: false,
            last_message_was_assistant: false,
            pending_tool_block: None,
            pending_exec_started_at: None,
            pending_tool_started_at: None,
        };
        // Write a small banner to the log so the browser viewer has content
        append_log("[info] Slide TUI session started");
        s
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    fn on_tick(&mut self) {
        // Emit commit animation ticks every ~100ms while running
        if self.status == RunStatus::Running {
            if self.thinking_last_change.elapsed() > Duration::from_millis(100) {
                self.app_event_tx.send(AppEvent::CommitTick);
                self.thinking_last_change = Instant::now();
            }
        }
    }

    fn submit_message(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }

        // ユーザーメッセージは送信直後に表示する（codex と同様）
        // 表示は AppEvent::InsertHistoryCell 側で処理する
        append_log(&format!("user: {}", text));

        if let Some(agent) = self.chat_widget.get_agent() {
            agent.submit_text_bg(text);
        }

        // Mark generating state
        self.status = RunStatus::Running;
        self.last_tick = Instant::now();
        self.thinking_frame_idx = 0;
        self.thinking_last_change = Instant::now();
    }

    /// Codex風のシンプルなキーイベント処理（user送信時は上側へ差し込み）
    pub fn handle_key_event(&mut self, key: KeyEvent)
    {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // Global shortcuts
        match key {
            // Transcript overlay toggle
            KeyEvent { code: KeyCode::Char('t'), modifiers: KeyModifiers::CONTROL, .. } => {
                let mut lines: Vec<Line<'static>> = Vec::new();
                // TODO: Get history from ChatWidget
                let history: Vec<HistoryCell> = Vec::new();
                for cell in &history {
                    for l in cell.lines() {
                        lines.push(l);
                    }
                }
                self.overlay.set_lines(lines);
                return;
            }
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            |             KeyEvent { code: KeyCode::Esc, .. } => {
                if self.chat_widget.is_task_running() {
                    self.chat_widget.interrupt_agent();
                    return;
                }
                if self.show_modal {
                    self.show_modal = false;
                } else {
                    self.quit();
                }
                return;
            }
            KeyEvent { code: KeyCode::Char('m'), modifiers: KeyModifiers::CONTROL, .. } => {
                // Model selection popup from builtin presets
                let mut items: Vec<crate::bottom_pane::list_selection_view::SelectionItem> = Vec::new();
                for p in builtin_model_presets() {
                    let tx = self.app_event_tx.clone();
                    let name = p.label.to_string();
                    let desc = Some(p.description.to_string());
                    let model_slug = p.model.to_string();
                    let effort = p.effort;
                    items.push(crate::bottom_pane::list_selection_view::SelectionItem {
                        name: name.clone(),
                        description: desc,
                        is_current: settings::current_model().as_deref() == Some(p.model)
                            && settings::current_effort() == effort,
                        actions: vec![Box::new(move |t: &AppEventSender| {
                            t.send(AppEvent::UpdateModel(model_slug.clone()));
                            t.send(AppEvent::UpdateReasoningEffort(effort));
                            tx.send(AppEvent::PersistModelSelection { model: model_slug.clone(), effort });
                            t.send(AppEvent::ToolOutput { text: format!("Model: {}", name) });
                            settings::save_model(&model_slug);
                            settings::save_effort(effort);
                        })],
                    });
                }
                self.chat_widget.show_selection_view(
                    "Select model and reasoning level".to_string(),
                    Some("Switch model for this session".to_string()),
                    Some("Enter to confirm, Esc to dismiss".to_string()),
                    items,
                    self.app_event_tx.clone(),
                );
                return;
            }
            KeyEvent { code: KeyCode::Char('a'), modifiers: KeyModifiers::CONTROL, .. } => {
                // Approvals popup from builtin presets
                let mut items: Vec<crate::bottom_pane::list_selection_view::SelectionItem> = Vec::new();
                for p in builtin_approval_presets() {
                    let tx = self.app_event_tx.clone();
                    let name = p.name.to_string();
                    let desc = Some(p.description.to_string());
                    let approval = p.approval.clone();
                    let sandbox = p.sandbox.clone();
                    items.push(crate::bottom_pane::list_selection_view::SelectionItem {
                        name: name.clone(),
                        description: desc,
                        is_current: false,
                        actions: vec![Box::new(move |t: &AppEventSender| {
                            t.send(AppEvent::UpdateAskForApprovalPolicy(approval.clone()));
                            t.send(AppEvent::UpdateSandboxPolicy(sandbox.clone()));
                            tx.send(AppEvent::ToolOutput { text: format!("Approval preset: {}", name) });
                        })],
                    });
                }
                self.chat_widget.show_selection_view(
                    "Select approval & sandbox".to_string(),
                    None,
                    Some("Press Enter to confirm or Esc to go back".to_string()),
                    items,
                    self.app_event_tx.clone(),
                );
                return;
            }
            KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.show_modal = !self.show_modal;
                return;
            }
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.chat_widget.clear_history();
                return;
            }
            KeyEvent { code: KeyCode::Char('i'), .. } if self.mode == Mode::Normal => { self.mode = Mode::Insert; return; }
            KeyEvent { code: KeyCode::Char('/'), .. } if self.mode == Mode::Insert && !self.chat_widget.is_intercepting_input() => {
                // Open command palette or file search depending on input later. For now show search popup directly for /open-file UX
                self.open_file_search();
                return;
            }
            _ => {}
        }

        // If overlay is active, let it handle keys first
        if self.overlay.handle_key(key) { return; }

        // Delegate to bottom pane for input handling
        // If file-search popup is active, intercept keys here
        if self.chat_widget.is_intercepting_input() {
            // Minimal key handling for search: type to update query, Enter to select, Esc to close
            use crossterm::event::KeyCode::*;
            if let Some(popup) = self.chat_widget.file_search_mut() {
                match key.code {
                    Esc => { self.chat_widget.hide_file_search(); return; }
                    Enter => {
                        if let Some(rel) = popup.selected_match() {
                            // Resolve relative path from cwd
                            let path = rel.to_string();
                            self.app_event_tx.send(AppEvent::FileReadRequest { path });
                            self.chat_widget.hide_file_search();
                            return;
                        }
                    }
                    Backspace => {
                        // Drop last char from query
                        // For simplicity, we cannot read current query; treat as no-op here
                    }
                    Char(c) => {
                        let mut q = String::new();
                        // Simplified: append a char per key to build query (real impl would track state)
                        q.push(c);
                        // Kick async search task
                        let tx = self.app_event_tx.clone();
                        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                        tokio::spawn(async move {
                            let query = q.clone();
                            match crate::bottom_pane::file_search_popup::FileSearchPopup::run_search(query.clone(), cwd).await {
                                Ok(list) => tx.send(AppEvent::FileSearchResults { query, matches: list }),
                                Err(e) => tx.send(AppEvent::FileSearchResults { query, matches: vec![] }),
                            }
                        });
                        return;
                    }
                    Up => { if let Some(p) = self.chat_widget.file_search_mut() { p.move_up(); } return; }
                    Down => { if let Some(p) = self.chat_widget.file_search_mut() { p.move_down(); } return; }
                    _ => {}
                }
            }
        }

        // ChatWidget handles all input processing internally
        if let Some(_result) = self.chat_widget.handle_key_event(key) {
            // ChatWidget will handle the submission internally
            // Just update app running state when needed
            if self.chat_widget.is_task_running() {
                self.status = RunStatus::Running;
                self.last_tick = Instant::now();
                self.thinking_frame_idx = 0;
                self.thinking_last_change = Instant::now();
            }
        }
    }

    fn on_mouse_wheel(&mut self, delta_lines: isize) {
        // Mouse scroll controls chat history only; disable follow-to-bottom on user scroll
        if delta_lines == 0 {
            return;
        }
        if delta_lines < 0 {
            // scroll down (towards bottom)
        self.chat_widget.on_mouse_wheel(delta_lines);
        } else {
            // scroll up (towards top) - delegate to ChatWidget
            self.chat_widget.on_mouse_wheel(delta_lines);
        }
    }

    fn handle_popup_key(&mut self, kind: PopupKind, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.active_popup = None;
            }
            KeyCode::Down => {
                if !self.popup_filtered_indices.is_empty() {
                    self.popup_selected = (self.popup_selected + 1)
                        .min(self.popup_filtered_indices.len().saturating_sub(1));
                }
            }
            KeyCode::Up => {
                if !self.popup_filtered_indices.is_empty() {
                    self.popup_selected = self.popup_selected.saturating_sub(1);
                }
            }
            KeyCode::Home => {
                self.popup_selected = 0;
            }
            KeyCode::End => {
                if !self.popup_filtered_indices.is_empty() {
                    self.popup_selected = self.popup_filtered_indices.len() - 1;
                }
            }
            KeyCode::Backspace => {
                self.popup_filter.pop();
                self.apply_popup_filter();
            }
            KeyCode::Char(c) => {
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                    self.popup_filter.push(c);
                    self.apply_popup_filter();
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = self
                    .popup_filtered_indices
                    .get(self.popup_selected)
                    .copied()
                {
                    match kind {
                        PopupKind::Command => self.exec_command_palette(idx),
                        PopupKind::FileSearch => self.exec_file_open(idx),
                    }
                }
            }
            _ => {}
        }
    }

    fn apply_popup_filter(&mut self) {
        let q = self.popup_filter.to_lowercase();
        self.popup_filtered_indices = self
            .popup_items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        self.popup_selected = 0;
    }

    fn open_file_search(&mut self) {
        self.active_popup = Some(PopupKind::FileSearch);
        self.popup_title = "Search slides/*.md".into();
        self.popup_items = find_markdown_files();
        self.popup_filter.clear();
        self.popup_filtered_indices = (0..self.popup_items.len()).collect();
        self.popup_selected = 0;
    }

    fn exec_command_palette(&mut self, idx: usize) {
        let cmd = &self.popup_items[idx];
        self.active_popup = None;
        match cmd.as_str() {
            "New Slide from Template" => match create_slide_from_template() {
                Ok(path) => {
                    self.modal_title = "Created".into();
                    self.modal_body = format!("Created new slide: {}", path);
                    self.show_modal = true;
                    self.mru_add(path);
                }
                Err(e) => {
                    self.modal_title = "Error".into();
                    self.modal_body = format!("Failed to create slide: {}", e);
                    self.show_modal = true;
                }
            },
            "Open Slide Preview (from file)" => {
                // TODO: Add file search back if needed
                self.modal_title = "Not Implemented".into();
                self.modal_body = "File search functionality not yet implemented".into();
                self.show_modal = true;
            }
            "Save Chat to slides/draft.md" => {
                // TODO: Get history from ChatWidget  
                let history: Vec<HistoryCell> = Vec::new();
                match save_chat_as_draft(&history) {
                    Ok(path) => {
                        self.modal_title = "Saved".into();
                        self.modal_body = format!("Saved to {}", path);
                        self.show_modal = true;
                        self.mru_add(path);
                    }
                    Err(e) => {
                        self.modal_title = "Error".into();
                        self.modal_body = format!("Failed to save draft: {}", e);
                        self.show_modal = true;
                    }
                }
            }
            "Toggle Help" => {
                self.show_modal = !self.show_modal;
            }
            "Clear Messages" => {
                self.chat_widget.clear_history();
            }
            "Quit" => {
                self.quit();
            }
            _ => {
                if let Some(rest) = cmd.strip_prefix("Open Recent: ") {
                    self.preview_path = Some(PathBuf::from(rest));
                    self.should_quit = true;
                }
            }
        }
    }

    fn exec_file_open(&mut self, idx_in_items: usize) {
        self.active_popup = None;
        if let Some(path) = self.popup_items.get(idx_in_items) {
            self.preview_path = Some(PathBuf::from(path));
            self.mru_add(path.clone());
            self.should_quit = true; // exit app loop to launch preview
        }
    }

    fn mru_add(&mut self, path: String) {
        // move-to-front unique
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        if self.recent_files.len() > 10 {
            self.recent_files.truncate(10);
        }
    }
}

pub async fn run_app(init_recent_files: Vec<String>) -> Result<RunResult> {
    // Initialize TUI system with frame synchronization
    crate::tui::set_modes()?;
    
    let mut tui = Tui::new()?;
    let mut app = App::new_with_recents(init_recent_files, tui.frame_requester());
    
    // Spawn core agent and set it in ChatWidget
    match crate::agent::AgentHandle::spawn().await {
        Ok(agent) => {
            app.chat_widget.set_agent(Some(agent));
        }
        Err(_e) => {
            let cell = HistoryCell::new_system_status(
                SystemLabel::Info,
                ["(failed to start agent; using local demo)"],
            );
            app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        }
    }

    // Insert initial banner
    tui.insert_history_lines(banner_history_lines());

    // Main event loop with frame synchronization
    loop {
        // Process all pending AppEvents first
        while let Ok(ev) = app.app_event_rx.try_recv() {
            handle_app_event(&mut tui, &mut app, ev);
        }
        
        // Process agent events (collect first to avoid borrowing conflicts)
        let mut core_events = Vec::new();
        if let Some(agent) = app.chat_widget.get_agent_mut() {
            while let Ok(core_ev) = agent.rx.try_recv() {
                core_events.push(core_ev);
            }
        }
        
        // Process collected core events
        for core_ev in core_events {
            handle_core_event(&mut tui, &mut app, core_ev);
            
            // Process any AppEvents generated by core events
            while let Ok(app_ev) = app.app_event_rx.try_recv() {
                handle_app_event(&mut tui, &mut app, app_ev);
            }
        }
        
        // Handle terminal events
        if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(16)) {
            if let Ok(event) = crossterm::event::read() {
                match event {
                    crossterm::event::Event::Key(key) => {
                        app.handle_key_event(key);
                        
                        // Process any AppEvents generated by key handling
                        while let Ok(ev) = app.app_event_rx.try_recv() {
                            handle_app_event(&mut tui, &mut app, ev);
                        }
                    }
                    crossterm::event::Event::Paste(pasted) => {
                        app.chat_widget.handle_paste(pasted);
                    }
                    crossterm::event::Event::Mouse(mev) => {
                        match mev.kind {
                            crossterm::event::MouseEventKind::ScrollUp => app.chat_widget.on_mouse_wheel(3),
                            crossterm::event::MouseEventKind::ScrollDown => app.chat_widget.on_mouse_wheel(-3),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        
        // Draw frame with proper ordering
        let terminal_size = tui.size()?;
        let desired_height = app.chat_widget.desired_height(terminal_size.width);
        tui.draw(desired_height, |frame| {
            (&app.chat_widget).render_ref(frame.area(), frame.buffer_mut());
            if let Some((x, y)) = app.chat_widget.cursor_pos(frame.area()) {
                frame.set_cursor_position((x, y));
            }
        })?;
        
        // Tick for animations
        app.on_tick();

        if app.should_quit {
            break;
        }
        
        // Small delay to prevent busy loop
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
    }

    // Cleanup terminal
    crate::tui::clear_modes()?;
    tui.show_cursor()?;
    tui.clear()?;
    tui.flush()?;

    let exit = if let Some(path) = app.preview_path {
        AppExit::Preview(path)
    } else {
        AppExit::Quit
    };
    Ok(RunResult {
        exit,
        recent_files: app.recent_files,
    })
}
// draw_input_area_only removed - now handled by tui.draw()

fn handle_core_event(tui: &mut Tui, app: &mut App, ev: CoreEvent)
{
    match ev {
        CoreEvent::SessionConfigured { .. } => {}
        CoreEvent::TaskStarted => {
            app.status = RunStatus::Running;
            app.chat_widget.set_task_running(true);
            append_log("[task] started");
            app.app_event_tx.send(AppEvent::StartCommitAnimation);
        }
        CoreEvent::AgentMessageDelta { delta } => {
            // Split incoming delta into normal text vs tool/exec annotations, and group blocks
            // IMPORTANT: preserve original newline semantics using split_inclusive('\n')
            let mut normal_buf = String::new();
            for raw in delta.split_inclusive('\n') {
                let line = raw.trim_end_matches('\n').trim_end_matches('\r');
                let t = line.trim_start();
                let is_tool_begin = t.starts_with("[Tool Execution]");
                let is_tool_mid = t.starts_with('▶');
                let is_tool_end = t.starts_with("[Tool Execution Result]");
                let is_mcp_tag = t.contains("MCP:") || t.contains("Tool:");
                let is_search_tag = t.contains("WebSearch:") || t.contains("Search:");
                let is_exec_begin = t.starts_with("$ ");
                let is_exec_end = t.starts_with("exit ");

                if is_tool_begin || is_tool_mid || is_tool_end || is_exec_begin || is_exec_end {
                    // Ensure assistant stream is flushed before tool blocks
                    app.chat_widget.finalize_stream();
                    // Exec block handling
                    if is_exec_begin || is_exec_end {
                        if is_exec_begin {
                            if app.pending_exec_block.is_none() {
                                app.pending_exec_block = Some(Vec::new());
                            }
                            app.pending_exec_started_at = Some(Instant::now());
                            if let Some(ref mut blk) = app.pending_exec_block {
                                blk.push(line.to_string());
                            }
                        } else {
                            // exit ...
                            if let Some(mut blk) = app.pending_exec_block.take() {
                                if let Some(st) = app.pending_exec_started_at.take() {
                                    let ms = st.elapsed().as_millis();
                                    blk.push(format!("took {}ms", ms));
                                }
                                blk.push(line.to_string());
                                let cell = HistoryCell::new_system_status(SystemLabel::Exec, blk);
                                app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
                            } else {
                                let cell = HistoryCell::new_system_status(SystemLabel::Exec, [line.to_string()]);
                                app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
                            }
                        }
                        continue;
                    }
                    // Generic tool block handling
                    if app.pending_tool_block.is_none() {
                        app.pending_tool_block = Some(Vec::new());
                    }
                    if app.pending_tool_started_at.is_none() { app.pending_tool_started_at = Some(Instant::now()); }
                    if let Some(ref mut blk) = app.pending_tool_block {
                        blk.push(line.to_string());
                    }
                    if is_tool_end {
                        if let Some(mut blk) = app.pending_tool_block.take() {
                            if let Some(st) = app.pending_tool_started_at.take() {
                                let ms = st.elapsed().as_millis();
                                blk.push(format!("took {}ms", ms));
                            }
                            // Try to classify as MCP/Search ifタグを含む
                            let label = if blk.iter().any(|l| l.contains("MCP:")) || is_mcp_tag {
                                SystemLabel::Mcp
                            } else if blk.iter().any(|l| l.contains("WebSearch:")) || is_search_tag {
                                SystemLabel::Search
                            } else {
                                SystemLabel::Info
                            };
                            let cell = HistoryCell::new_system_status(label, blk);
                            app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
                        }
                    }
                } else {
                    normal_buf.push_str(line);
                    if raw.ends_with('\n') {
                        // Re-add newline only when it was present in the original chunk
                        normal_buf.push('\n');
                    }
                }
            }

            if !normal_buf.is_empty() {
                app.chat_widget.handle_agent_message_delta(&normal_buf);
            }
            // Debug-only per-chunk logging (default off)
            if std::env::var("SLIDE_STREAM_DEBUG_CHUNKS").ok().as_deref() == Some("1") {
                append_log(&format!("assistantΔ: {}", delta.replace('\n', "\\n")));
            }
            // approx_output_chars now handled by ChatWidget
        }
        CoreEvent::AgentMessage { message } => {
            if !message.is_empty() {
                app.chat_widget.handle_agent_message(&message);
            }
            let message_for_log = message.clone();
            append_log(&format!("assistant: {}", message_for_log));
        }
        // New explicit tool events (preferred over heuristic blocks)
        CoreEvent::ToolBegin { id: _id, kind, summary, cwd: _ } => {
            let label = match kind { CoreToolKind::Exec => SystemLabel::Exec, CoreToolKind::Mcp => SystemLabel::Mcp, CoreToolKind::Search => SystemLabel::Search, CoreToolKind::Info => SystemLabel::Info };
            app.chat_widget.finalize_stream();
            app.pending_tool_block = Some(vec![format!("▶ {}", summary)]);
            app.pending_tool_started_at = Some(Instant::now());
            app.chat_widget.update_status_header(format!("Tool: {}", summary));
            let cell = HistoryCell::new_system_status(label, [format!("▶ {}", summary)]);
            app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        }
        CoreEvent::ToolOutput { id: _id, stream: _, line } => {
            // Render 1 line immediately (use content formatter; stderr coloring handled by prefix in core in future)
            let styled = crate::history_cell::format_content_line(&line);
            app.app_event_tx.send(AppEvent::ToolOutput { text: line });
        }
        CoreEvent::ToolEnd { id: _id, ok: _, exit_code, took_ms } => {
            if let Some(mut blk) = app.pending_tool_block.take() {
                blk.push(format!("took {}ms", took_ms));
                if let Some(code) = exit_code { blk.push(format!("exit {}", code)); }
                let label = SystemLabel::Exec; // unknown kind in this context; Exec as default
                let cell = HistoryCell::new_system_status(label, blk);
                app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            }
            app.chat_widget.update_status_header("Working".to_string());
        }
        CoreEvent::ExecCommandBegin { command, .. } => {
            // Flush assistant stream and start a new exec block
            app.chat_widget.finalize_stream();
            app.pending_exec_block = Some(vec![format!("$ {}", command.join(" "))]);
            app.pending_exec_started_at = Some(Instant::now());
            app.chat_widget.update_status_header(format!("Running: {}", command.join(" ")));
            append_log("[exec] begin");
        }
        CoreEvent::ExecCommandEnd { exit_code, .. } => {
            // Close and flush exec block if present; otherwise print a one-liner
            let exit_line = format!("exit {}", exit_code);
            if let Some(mut blk) = app.pending_exec_block.take() {
                if let Some(st) = app.pending_exec_started_at.take() {
                    let ms = st.elapsed().as_millis();
                    blk.push(format!("took {}ms", ms));
                }
                blk.push(exit_line);
                let cell = HistoryCell::new_system_status(SystemLabel::Exec, blk);
                app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            } else {
                let cell = HistoryCell::new_system_status(SystemLabel::Exec, [exit_line.clone()]);
                app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            }
            app.chat_widget.update_status_header("Working".to_string());
            append_log("[exec] end");
        }
        CoreEvent::ApplyPatchApprovalRequest {
            id,
            changes,
            reason,
        } => {
            // Convert map of path->desc into a vector of display strings
            let mut items: Vec<String> = changes
                .into_iter()
                .map(|(p, v)| format!("{}: {}", p.display(), v))
                .collect();
            items.sort();
            let req = ApprovalRequest::Patch {
                id,
                changes: items,
                reason,
            };
            app.chat_widget
                .show_approval_modal(req, app.app_event_tx.clone());
            append_log("[approve] apply_patch requested");
        }
        CoreEvent::PatchApplyBegin { .. } => {
            let cell = HistoryCell::new_system_status(SystemLabel::Patch, ["applying..."]);
            app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            append_log("[patch] applying...");
        }
        CoreEvent::PatchApplyEnd { success, .. } => {
            let status = if success { "ok" } else { "failed" };
            let cell = HistoryCell::new_system_status(SystemLabel::Patch, [status.to_string()]);
            app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            append_log(&format!("[patch] {}", status));
        }
        CoreEvent::TurnDiff { unified_diff } => {
            let mut lines: Vec<Line<'static>> = Vec::new();
            for l in unified_diff.split('\n') {
                lines.push(Line::from(l.to_string()));
            }
            app.overlay.set_lines(lines);
            app.overlay.set_title("D I F F");
            append_log("[diff] overlay opened");
        }
        CoreEvent::TaskComplete => {
            app.status = RunStatus::Idle;
            app.chat_widget.set_task_running(false);
            
            // Delegate task completion to ChatWidget
            app.chat_widget.handle_task_complete();

            // Flush any pending tool/exec blocks to avoid dangling groups
            if let Some(mut blk) = app.pending_tool_block.take() {
                if let Some(st) = app.pending_tool_started_at.take() {
                    let ms = st.elapsed().as_millis();
                    blk.push(format!("took {}ms", ms));
                }
                let label = if blk.iter().any(|l| l.contains("MCP:")) {
                    SystemLabel::Mcp
                } else if blk.iter().any(|l| l.contains("WebSearch:")) || blk.iter().any(|l| l.contains("Search:")) {
                    SystemLabel::Search
                } else {
                    SystemLabel::Info
                };
                let cell = HistoryCell::new_system_status(label, blk);
                app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            }
            if let Some(blk) = app.pending_exec_block.take() {
                let cell = HistoryCell::new_system_status(SystemLabel::Exec, blk);
                app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            }
            app.chat_widget.update_status_header("Working".to_string());
            append_log("[task] complete");
            app.app_event_tx.send(AppEvent::StopCommitAnimation);
        }
        CoreEvent::Error { message } => {
            let cell = HistoryCell::new_system_status(SystemLabel::Error, [message.clone()]);
            app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            app.status = RunStatus::Error;
            app.chat_widget.set_task_running(false);
            append_log(&format!("[error] {}", message));
            app.app_event_tx.send(AppEvent::StopCommitAnimation);
        }
        CoreEvent::ShutdownComplete => {}
        CoreEvent::ExecApprovalRequest {
            id,
            command,
            cwd: _,
            reason,
        } => {
            let req = ApprovalRequest::Exec {
                id,
                command,
                reason,
            };
            app.chat_widget
                .show_approval_modal(req, app.app_event_tx.clone());
            append_log("[approve] exec requested");
        }
    }
}

fn handle_app_event(tui: &mut Tui, app: &mut App, ev: AppEvent)
{
    match ev {
        AppEvent::StartFileSearch { query } => {
            app.chat_widget.show_file_search();
            if let Some(p) = app.chat_widget.file_search_mut() {
                p.set_query(&query);
            }
        }
        AppEvent::InsertHistoryCell(cell) => {
            // Queue history lines for ordered rendering (codex-1 style)
            let mut lines = cell.display_lines(80);
            
            // Add spacing for proper conversation flow
            if !lines.is_empty() {
                // Debug info
                tracing::debug!(
                    "Spacing logic: is_user={}, is_assistant={}, last_was_assistant={}, has_history={}",
                    cell.is_user(),
                    cell.is_assistant(),
                    app.last_message_was_assistant,
                    app.has_emitted_history
                );
                
                // Add space before user messages when previous was AI (new conversation turn)
                if cell.is_user() && app.last_message_was_assistant && app.has_emitted_history {
                    tracing::debug!("Adding space before user message");
                    lines.insert(0, "".into()); // ユーザーメッセージの前に空行を追加
                }
                
                // Add space after AI messages for visual separation
                if cell.is_assistant() {
                    tracing::debug!("Adding space after AI message");
                    lines.push("".into()); // AIメッセージの後に空行を追加
                }
            }
            
            // Update tracking state
            app.has_emitted_history = true;
            app.last_message_was_assistant = cell.is_assistant();
            
            tui.insert_history_lines(lines);
        }
        AppEvent::ToolOutput { text } => {
            let cell = HistoryCell::new_system_status(SystemLabel::Info, [text]);
            app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        }
        AppEvent::StartCommitAnimation => {
            app.chat_widget.set_task_running(true);
        }
        AppEvent::CommitTick => {
            let dots = [".", "..", "...", "…." ];
            app.thinking_frame_idx = app.thinking_frame_idx.wrapping_add(1);
            let idx = (app.thinking_frame_idx as usize) % dots.len();
            app.chat_widget.update_status_header(format!("Working{}", dots[idx]));
        }
        AppEvent::StopCommitAnimation => {
            app.chat_widget.set_task_running(false);
        }
        AppEvent::UpdateModel(model) => {
            if let Some(agent) = app.chat_widget.get_agent() {
                agent.override_turn_context_bg(Some(model.clone()), None, None, None);
            }
            // Model setting handled by ChatWidget
            app.chat_widget.update_status_header(format!("Model: {}", model));
        }
        AppEvent::UpdateReasoningEffort(effort) => {
            if let Some(agent) = app.chat_widget.get_agent() {
                agent.override_turn_context_bg(None, effort, None, None);
            }
        }
        AppEvent::UpdateAskForApprovalPolicy(policy) => {
            if let Some(agent) = app.chat_widget.get_agent() {
                agent.override_turn_context_bg(None, None, Some(policy), None);
            }
        }
        AppEvent::UpdateSandboxPolicy(policy) => {
            if let Some(agent) = app.chat_widget.get_agent() {
                agent.override_turn_context_bg(None, None, None, Some(policy));
            }
        }
        AppEvent::PersistModelSelection { .. } => {}
        AppEvent::ExecApproval { id, decision } => {
            if let Some(agent) = app.chat_widget.get_agent() {
                let c = agent.codex.clone();
                tokio::spawn(async move {
                    let _ = c.submit(Op::ExecApproval { id, decision }).await;
                });
            }
        }
        AppEvent::PatchApproval { id, decision } => {
            if let Some(agent) = app.chat_widget.get_agent() {
                let c = agent.codex.clone();
                tokio::spawn(async move {
                    let _ = c.submit(Op::PatchApproval { id, decision }).await;
                });
            }
        }
        AppEvent::FileReadRequest { path } => {
            let tx = app.app_event_tx.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let pathbuf = std::path::PathBuf::from(&path);
                let canonical = match tokio::fs::canonicalize(&pathbuf).await {
                    Ok(p) => p,
                    Err(e) => {
                        tx.send(AppEvent::FileReadResult { path, content: Err(format!("canonicalize failed: {e}")) });
                        return;
                    }
                };
                let meta = match tokio::fs::metadata(&canonical).await {
                    Ok(m) => m,
                    Err(e) => {
                        tx.send(AppEvent::FileReadResult { path, content: Err(format!("metadata failed: {e}")) });
                        return;
                    }
                };
                if !meta.is_file() {
                    tx.send(AppEvent::FileReadResult { path, content: Err("not a regular file".to_string()) });
                    return;
                }
                let max_bytes: u64 = 256 * 1024;
                let truncated = meta.len() > max_bytes;
                let mut file = match tokio::fs::File::open(&canonical).await {
                    Ok(f) => f,
                    Err(e) => {
                        tx.send(AppEvent::FileReadResult { path, content: Err(format!("open failed: {e}")) });
                        return;
                    }
                };
                let mut buf = Vec::with_capacity((max_bytes as usize).min(262144));
                let to_read = max_bytes.min(meta.len());
                let mut reader = tokio::io::BufReader::new(file);
                let mut handle = reader.take(to_read);
                if let Err(e) = handle.read_to_end(&mut buf).await {
                    tx.send(AppEvent::FileReadResult { path, content: Err(format!("read failed: {e}")) });
                    return;
                }
                let mut content = String::from_utf8_lossy(&buf).to_string();
                if truncated { content.push_str("\n…[truncated]\n"); }
                tx.send(AppEvent::FileReadResult { path, content: Ok(content) });
            });
        }
        AppEvent::FileReadResult { path, content } => match content {
            Ok(text) => {
                let header = format!("{}", path);
                let mut lines = Vec::new();
                lines.push(ratatui::text::Line::from(""));
                lines.push(ratatui::text::Line::from(
                    ratatui::text::Span::styled(
                        header,
                        ratatui::style::Style::default()
                            .fg(ratatui::style::Color::LightBlue)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    ),
                ));
                for l in text.lines() {
                    lines.push(crate::history_cell::format_content_line(l));
                }
                // Convert to HistoryCell for unified processing
                use crate::history_cell::{AgentMessageCell, HistoryCellTrait};
                let cell = AgentMessageCell::new(lines, true);
                tui.insert_history_lines(cell.display_lines(80));
            }
            Err(err) => {
                let cell = HistoryCell::new_system_status(SystemLabel::Error, [format!("open: {path} — {err}")]);
                app.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            }
        },
        AppEvent::FileSearchResults { query, matches } => {
            if let Some(p) = app.chat_widget.file_search_mut() {
                p.set_matches(&query, matches);
            }
        }
        AppEvent::CodexEvent(_) => {
            // Handle codex events if needed
        }
        AppEvent::NewSession => {
            // Handle new session if needed
        }
        AppEvent::ExitRequest => {
            // Handle exit request if needed
        }
        AppEvent::CodexOp(_) => {
            // Handle codex operations if needed
        }
        AppEvent::DiffResult(_) => {
            // Handle diff result if needed
        }
        
        // 連続実行関連イベントの処理
        AppEvent::ContinuousExecutionStart(event) => {
            app.chat_widget.on_continuous_execution_start(event);
        }
        AppEvent::ContinuousExecutionStep(event) => {
            app.chat_widget.on_continuous_execution_step(event);
        }
        AppEvent::ContinuousExecutionEnd(event) => {
            app.chat_widget.on_continuous_execution_end(event);
        }
        AppEvent::ToolExecutionBegin(event) => {
            app.chat_widget.on_tool_execution_begin(event);
        }
        AppEvent::ToolExecutionEnd(event) => {
            app.chat_widget.on_tool_execution_end(event);
        }
        
        // MCP関連イベントの処理
        AppEvent::McpToolCallBegin(event) => {
            app.chat_widget.on_mcp_tool_call_begin(event);
        }
        AppEvent::McpToolCallEnd(event) => {
            app.chat_widget.on_mcp_tool_call_end(event);
        }
    }
}

fn append_log(line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/slide.log")
    {
        let _ = writeln!(f, "{}", line);
    }
}

fn find_markdown_files() -> Vec<String> {
    let mut result = Vec::new();
    let roots = ["slides"];
    for root in roots {
        if let Ok(meta) = std::fs::metadata(root) {
            if meta.is_dir() {
                walk_md(root, &mut result);
            }
        }
    }
    result.sort();
    result
}

fn walk_md(dir: &str, out: &mut Vec<String>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    if let Some(s) = path.to_str() {
                        walk_md(s, out);
                    }
                } else if ft.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == "md" {
                            if let Some(s) = path.to_str() {
                                out.push(s.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
}

fn create_slide_from_template() -> std::io::Result<String> {
    use std::io::Write;
    let dir = std::path::Path::new("slides");
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("slide-{}.md", ts));
    let mut file = std::fs::File::create(&path)?;
    let template = "# Title\n\n## Agenda\n- Topic 1\n- Topic 2\n\n## Content\nWrite here.\n";
    file.write_all(template.as_bytes())?;
    Ok(path.to_string_lossy().to_string())
}

fn save_chat_as_draft(history: &[HistoryCell]) -> std::io::Result<String> {
    use std::io::Write;
    let dir = std::path::Path::new("slides");
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }
    let path = dir.join("draft.md");
    let mut file = std::fs::File::create(&path)?;
    for cell in history {
        let lines = cell.plain_text_lines();
        if lines.is_empty() {
            continue;
        }
        let mut iter = lines.into_iter();
        if let Some(first) = iter.next() {
            writeln!(file, "- {}", first)?;
        }
        for rest in iter {
            writeln!(file, "  {}", rest)?;
        }
    }
    Ok(path.to_string_lossy().to_string())
}
