use crate::custom_terminal::Terminal;
use anyhow::Result;
use crossterm::{
    cursor::{self, MoveTo},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, ScrollUp, EnterAlternateScreen, LeaveAlternateScreen},
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
use crate::app_event_sender::{AppEvent, AppEventSender};
use crate::bottom_pane::{BottomPane, BottomPaneParams};
use crate::history_cell::{HistoryCell, SystemLabel};
use crate::insert_history::insert_history_lines;
use crate::streaming::AnswerStreamState;
use crate::user_approval_widget::ApprovalRequest;
use crate::widgets::banner::banner_history_lines;
use crate::pager_overlay::PagerOverlay;
use crate::model_presets::builtin_model_presets;
use crate::approval_presets::builtin_approval_presets;
use crate::settings;
use slide_core::codex::Event as CoreEvent;
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

/// Codex風に簡略化されたアプリケーション状態
pub struct App {
    should_quit: bool,
    mode: Mode,
    status: RunStatus,
    last_tick: Instant,
    // Chat state represented as history cells
    history: Vec<HistoryCell>,
    // Chat scroll state
    chat_scroll_top: usize,
    chat_follow_bottom: bool,
    chat_viewport_height: usize,
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
    // Agent integration
    agent: Option<AgentHandle>,
    // Bottom pane integration (Codex風の統合UI)
    bottom_pane: BottomPane,
    // App event channel
    app_event_rx: tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    app_event_tx: AppEventSender,
    // Inline viewport history (pending lines to insert above)
    // pending_history_lines removed - messages now insert directly
    // Assistant応答の行単位ストリーミング状態
    answer_stream: AnswerStreamState,
    // Thinking... spinner state
    thinking_frame_idx: usize,
    thinking_last_change: Instant,
    // Preview of last assistant message (for notifications)
    last_agent_preview: String,
    // Simple running output size counter for token approximation
    approx_output_chars: usize,
    // Transcript/Diff overlay
    overlay: PagerOverlay,
}

impl App {
    fn clamp_scroll_top(&mut self) {
        let max_top = self.max_scroll_top();
        if self.chat_scroll_top > max_top {
            self.chat_scroll_top = max_top;
        }
    }

    fn max_scroll_top(&self) -> usize {
        let total_lines = self.total_chat_lines();
        total_lines.saturating_sub(self.chat_viewport_height)
    }

    fn follow_bottom_after_change(&mut self) {
        if self.chat_follow_bottom {
            self.chat_scroll_top = usize::MAX;
        } else {
            self.clamp_scroll_top();
        }
    }
    pub fn new() -> Self {
        Self::new_with_recents(Vec::new())
    }

    fn total_chat_lines(&self) -> usize {
        let mut history_lines: usize = self.history.iter().map(HistoryCell::line_count).sum();
        if !self.history.is_empty() {
            history_lines = history_lines.saturating_sub(1);
        }
        // plus one prompt line always
        history_lines + 1
    }

    pub fn new_with_recents(recent_files: Vec<String>) -> Self {
        let (app_tx_raw, app_rx) = tokio::sync::mpsc::unbounded_channel();
        let app_tx = AppEventSender::new(app_tx_raw);
        let s = Self {
            should_quit: false,
            mode: Mode::Normal,
            status: RunStatus::Idle,
            last_tick: Instant::now(),
            history: vec![HistoryCell::banner()],
            chat_scroll_top: 0,
            chat_follow_bottom: true,
            chat_viewport_height: 0,
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
            agent: None,
            // Empty placeholder to hide any ghost text in input
            bottom_pane: BottomPane::new(BottomPaneParams{ has_input_focus: true, placeholder_text: "".into()}),
            app_event_rx: app_rx,
            app_event_tx: app_tx,
            // pending_history_lines removed - history cells now insert directly
            answer_stream: AnswerStreamState::new(),
            thinking_frame_idx: 0,
            thinking_last_change: Instant::now(),
            last_agent_preview: String::new(),
            approx_output_chars: 0,
            overlay: PagerOverlay::new(),
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

        let cell = HistoryCell::new_user_prompt(text.clone());
        self.history.push(cell);
        self.follow_bottom_after_change();
        append_log(&format!("user: {}", text));

        if let Some(agent) = &self.agent {
            agent.submit_text_bg(text);
        }

        // Mark generating state
        self.status = RunStatus::Running;
        self.last_tick = Instant::now();
        self.thinking_frame_idx = 0;
        self.thinking_last_change = Instant::now();
    }

    /// Codex風のシンプルなキーイベント処理（user送信時は上側へ差し込み）
    pub fn handle_key_event<B>(&mut self, key: KeyEvent, terminal: &mut Terminal<B>)
    where
        B: ratatui::backend::Backend,
    {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // Global shortcuts
        match key {
            // Transcript overlay toggle
            KeyEvent { code: KeyCode::Char('t'), modifiers: KeyModifiers::CONTROL, .. } => {
                let mut lines: Vec<Line<'static>> = Vec::new();
                for cell in &self.history {
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
            | KeyEvent {
                code: KeyCode::Esc, ..
            } => {
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
                        is_current: settings::current_model().as_deref() == Some(p.model),
                        actions: vec![Box::new(move |t: &AppEventSender| {
                            t.send(AppEvent::UpdateModel(model_slug.clone()));
                            t.send(AppEvent::UpdateReasoningEffort(effort));
                            tx.send(AppEvent::PersistModelSelection { model: model_slug.clone(), effort });
                            t.send(AppEvent::ToolOutput { text: format!("Model: {}", name) });
                            settings::save_model(&model_slug);
                        })],
                    });
                }
                self.bottom_pane.show_selection_view(
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
                self.bottom_pane.show_selection_view(
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
                self.history.clear();
                return;
            }
            KeyEvent { code: KeyCode::Char('i'), .. } if self.mode == Mode::Normal => { self.mode = Mode::Insert; return; }
            KeyEvent { code: KeyCode::Char('/'), .. } if self.mode == Mode::Insert && !self.bottom_pane.is_intercepting_input() => {
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
        if self.bottom_pane.is_intercepting_input() {
            // Minimal key handling for search: type to update query, Enter to select, Esc to close
            use crossterm::event::KeyCode::*;
            if let Some(popup) = self.bottom_pane.file_search_mut() {
                match key.code {
                    Esc => { self.bottom_pane.hide_file_search(); return; }
                    Enter => {
                        if let Some(rel) = popup.selected_match() {
                            // Resolve relative path from cwd
                            let path = rel.to_string();
                            self.app_event_tx.send(AppEvent::FileReadRequest { path });
                            self.bottom_pane.hide_file_search();
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
                    Up => { if let Some(p) = self.bottom_pane.file_search_mut() { p.move_up(); } return; }
                    Down => { if let Some(p) = self.bottom_pane.file_search_mut() { p.move_down(); } return; }
                    _ => {}
                }
            }
        }

        if let Some(result) = self.bottom_pane.handle_key_event(key) {
            use crate::bottom_pane::InputResult;
            match result {
                InputResult::Submitted(text) => {
                    if text.trim().is_empty() {
                        return;
                    }
                    // Insert via unified AppEvent so ordering is consistent
                    let cell = HistoryCell::new_user_prompt(text.clone());
                    self.app_event_tx
                        .send(AppEvent::InsertHistoryCell(cell));
                    // then update internal state and dispatch to agent
                    // If we had image attachments queued, include them in the submission (core wire-up simplified)
                    {
                        let images = self.bottom_pane.take_recent_submission_images();
                        if images.is_empty() {
                            self.submit_message(text);
                        } else {
                            // Build items: text then LocalImage(s)
                            let mut items: Vec<InputItem> = Vec::new();
                            if !text.is_empty() {
                                items.push(InputItem::Text { text: text.clone() });
                            }
                            for p in images {
                                items.push(InputItem::LocalImage { path: p });
                            }
                            if let Some(agent) = &self.agent {
                                agent.submit_items_bg(items);
                            }
                            // Mark generating state similarly to submit_message
                            self.status = RunStatus::Running;
                            self.last_tick = Instant::now();
                            self.thinking_frame_idx = 0;
                            self.thinking_last_change = Instant::now();
                        }
                    }
                }
                InputResult::None => {}
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
            self.chat_scroll_top = self
                .chat_scroll_top
                .saturating_add(delta_lines.unsigned_abs() as usize);
        } else {
            // scroll up (towards top)
            let dec = delta_lines as usize;
            self.chat_scroll_top = self.chat_scroll_top.saturating_sub(dec);
        }
        self.clamp_scroll_top();
        self.chat_follow_bottom = self.chat_scroll_top >= self.max_scroll_top();
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
            "Save Chat to slides/draft.md" => match save_chat_as_draft(&self.history) {
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
            },
            "Toggle Help" => {
                self.show_modal = !self.show_modal;
            }
            "Clear Messages" => {
                self.history.clear();
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
    // 通常スクリーン＋インラインビューポート（Codex と同等の初期化）
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // オプションの alt-screen モード
    let use_alt = std::env::var("SLIDE_ALT_SCREEN").ok().map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if use_alt {
        let _ = execute!(stdout, EnterAlternateScreen);
    }
    // 念のため通常スクリーン由来の残骸を上に押し上げ、(0,0) から描画開始する
    if let Ok((_x, y)) = cursor::position() {
        if y > 0 {
            let _ = execute!(stdout, ScrollUp(y));
        }
    }
    let _ = execute!(stdout, MoveTo(0, 0));

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::with_options(backend)?;

    let mut app = App::new_with_recents(init_recent_files);
    // Spawn core agent
    match crate::agent::AgentHandle::spawn().await {
        Ok(agent) => app.agent = Some(agent),
        Err(_e) => {
            app.history.push(HistoryCell::new_system_status(
                SystemLabel::Info,
                ["(failed to start agent; using local demo)"],
            ));
        }
    }

    // 初回: 下部だけ描画 → バナーをスクロールバックへ
    draw_input_area_only(&mut terminal, &mut app)?;
    insert_history_lines(&mut terminal, banner_history_lines());

    loop {
        // Drain app events from UI widgets
        while let Ok(ev) = app.app_event_rx.try_recv() {
            match ev {
                AppEvent::StartFileSearch { query } => {
                    app.bottom_pane.show_file_search();
                    if let Some(p) = app.bottom_pane.file_search_mut() {
                        p.set_query(&query);
                    }
                }
                AppEvent::InsertHistoryCell(cell) => {
                    insert_history_lines(&mut terminal, cell.lines());
                }
                AppEvent::ToolOutput { text } => {
                    let cell = HistoryCell::new_system_status(SystemLabel::Info, [text]);
                    insert_history_lines(&mut terminal, cell.lines());
                }
                AppEvent::StartCommitAnimation => {
                    // ensure status indicator is visible
                    app.bottom_pane.set_task_running(true);
                }
                AppEvent::CommitTick => {
                    // progress shimmer and request redraw; update a simple animated header
                    let dots = [".", "..", "...", "…." ];
                    app.thinking_frame_idx = app.thinking_frame_idx.wrapping_add(1);
                    let idx = (app.thinking_frame_idx as usize) % dots.len();
                    app.bottom_pane.update_status_header(format!("Working{}", dots[idx]));
                }
                AppEvent::StopCommitAnimation => {
                    app.bottom_pane.set_task_running(false);
                }
                AppEvent::UpdateModel(model) => {
                    if let Some(agent) = &app.agent {
                        agent.override_turn_context_bg(Some(model.clone()), None, None, None);
                    }
                    // Update composer placeholder locally to reflect model
                    app.bottom_pane.set_composer_placeholder(format!("Model: {}", model));
                }
                AppEvent::UpdateReasoningEffort(effort) => {
                    if let Some(agent) = &app.agent {
                        agent.override_turn_context_bg(None, effort, None, None);
                    }
                }
                AppEvent::UpdateAskForApprovalPolicy(policy) => {
                    if let Some(agent) = &app.agent {
                        agent.override_turn_context_bg(None, None, Some(policy), None);
                    }
                }
                AppEvent::UpdateSandboxPolicy(policy) => {
                    if let Some(agent) = &app.agent {
                        agent.override_turn_context_bg(None, None, None, Some(policy));
                    }
                }
                AppEvent::PersistModelSelection { .. } => {
                    // Placeholder: No persistent config system in slide-code-test yet.
                }
                AppEvent::ExecApproval { id, decision } => {
                    if let Some(agent) = &app.agent {
                        let c = agent.codex.clone();
                        tokio::spawn(async move {
                            let _ = c.submit(Op::ExecApproval { id, decision }).await;
                        });
                    }
                }
                AppEvent::PatchApproval { id, decision } => {
                    if let Some(agent) = &app.agent {
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
                        // Canonicalize & bound size
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
                        let max_bytes: u64 = 256 * 1024; // 256KB
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
                        if truncated {
                            content.push_str("\n…[truncated]\n");
                        }
                        tx.send(AppEvent::FileReadResult { path, content: Ok(content) });
                    });
                }
                AppEvent::FileReadResult { path, content } => {
                    match content {
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
                            insert_history_lines(&mut terminal, lines);
                        }
                        Err(err) => {
                            let cell = HistoryCell::new_system_status(SystemLabel::Error, [format!("open: {path} — {err}")]);
                            insert_history_lines(&mut terminal, cell.lines());
                        }
                    }
                }
                AppEvent::FileSearchResults { query, matches } => {
                    if let Some(p) = app.bottom_pane.file_search_mut() {
                        p.set_matches(&query, matches);
                    }
                }
            }
        }

        // 下部の入力エリアのみ描画（履歴はスクロールバックへ差し込み）
        draw_input_area_only(&mut terminal, &mut app)?;

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Mouse(mev) => match mev.kind {
                    MouseEventKind::ScrollUp => app.on_mouse_wheel(3),
                    MouseEventKind::ScrollDown => app.on_mouse_wheel(-3),
                    _ => {}
                },
                Event::Key(key) => {
                    app.handle_key_event(key, &mut terminal);
                }
                Event::Resize(_, _) => {
                    // Keep latest visible on resize only when follow-bottom is enabled
                    // Inline ビューポートは毎描画で再計算するため、ここでは何もしない
                }
                _ => {}
            }
        }

        // Drain core events (non-blocking) without holding borrow on app.agent
        let mut drained_events = Vec::new();
        if let Some(agent) = app.agent.as_mut() {
            loop {
                match agent.rx.try_recv() {
                    Ok(ev) => drained_events.push(ev),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                }
            }
        }
        for ev in drained_events {
            handle_core_event(&mut terminal, &mut app, ev);
        }

        if app.should_quit {
            break;
        }

        // Tick and sleep
        app.on_tick();
        sleep(Duration::from_millis(16)).await;
    }

    // Cleanup terminal
    disable_raw_mode()?;
    terminal.show_cursor()?;
    terminal.clear()?;
    terminal.backend_mut().flush()?;

    let exit = if let Some(path) = app.preview_path {
        AppExit::Preview(path)
    } else {
        AppExit::Quit
    };
    // leave alt screen if used
    if use_alt {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
    Ok(RunResult {
        exit,
        recent_files: app.recent_files,
    })
}
fn draw_input_area_only<B>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()>
where
    B: ratatui::backend::Backend,
{
    let size = terminal.size()?;
    // ステータス行は描画しない（必要ならここで高さを足す）
    let status_height: u16 = 0;
    let desired_bottom_height = app.bottom_pane.desired_height(size.width).max(1);
    let total_desired_height = status_height.saturating_add(desired_bottom_height);
    let input_height = total_desired_height.min(size.height.max(1));
    let bottom_height = input_height.saturating_sub(status_height).max(1);
    let input_area = Rect {
        x: 0,
        y: size.height.saturating_sub(input_height),
        width: size.width,
        height: input_height,
    };

    // Update viewport area to match current terminal size
    terminal.set_viewport_area(input_area);

    terminal.draw(|f| {
        if app.overlay.is_active() {
            app.overlay.render_ref(Rect { x: input_area.x, y: input_area.y, width: input_area.width, height: bottom_height }, f.buffer_mut());
        } else {
            // Bottom pane (input area) using render_ref
            app.bottom_pane.render_ref(Rect { x: input_area.x, y: input_area.y + status_height, width: input_area.width, height: bottom_height }, f.buffer_mut());
            if let Some((x, y)) = app.bottom_pane.cursor_pos(Rect { x: input_area.x, y: input_area.y + status_height, width: input_area.width, height: bottom_height }) {
                f.set_cursor_position((x, y));
            }
        }
    })?;

    Ok(())
}

fn handle_core_event<B>(terminal: &mut Terminal<B>, app: &mut App, ev: CoreEvent)
where
    B: ratatui::backend::Backend,
{
    match ev {
        CoreEvent::SessionConfigured { .. } => {}
        CoreEvent::TaskStarted => {
            app.status = RunStatus::Running;
            app.bottom_pane.set_task_running(true);
            append_log("[task] started");
            app.app_event_tx.send(AppEvent::StartCommitAnimation);
        }
        CoreEvent::AgentMessageDelta { delta } => {
            let lines = app.answer_stream.push_delta(&delta);
            if !lines.is_empty() {
                insert_history_lines(terminal, lines);
            }
            append_log(&format!("assistantΔ: {}", delta));
            app.approx_output_chars = app.approx_output_chars.saturating_add(delta.len());
        }
        CoreEvent::AgentMessage { message } => {
            let mut pending = Vec::new();
            if !message.is_empty() {
                pending.extend(app.answer_stream.push_delta(&message));
                // keep a short preview for notification
                use unicode_segmentation::UnicodeSegmentation;
                let mut preview = String::new();
                let mut count = 0usize;
                for g in message.graphemes(true) {
                    let next = count + g.len();
                    if next > 200 { break; }
                    preview.push_str(g);
                    count = next;
                }
                app.last_agent_preview = preview;
                app.approx_output_chars = app.approx_output_chars.saturating_add(message.len());
            }
            let mut tail = app.answer_stream.finalize();
            pending.append(&mut tail);
            if !pending.is_empty() {
                insert_history_lines(terminal, pending);
            }
            append_log(&format!("assistant: {}", message));
        }
        CoreEvent::ExecCommandBegin { command, .. } => {
            let display = format!("$ {}", command.join(" "));
            let cell = HistoryCell::new_system_status(SystemLabel::Exec, [display.clone()]);
            insert_history_lines(terminal, cell.lines());
            append_log(&format!("[exec] {}", display));
        }
        CoreEvent::ExecCommandEnd { exit_code, .. } => {
            let display = format!("exit {}", exit_code);
            let cell = HistoryCell::new_system_status(SystemLabel::Exec, [display.clone()]);
            insert_history_lines(terminal, cell.lines());
            append_log(&format!("[exec] {}", display));
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
            app.bottom_pane
                .show_approval_modal(req, app.app_event_tx.clone());
            append_log("[approve] apply_patch requested");
        }
        CoreEvent::PatchApplyBegin { .. } => {
            let cell = HistoryCell::new_system_status(SystemLabel::Patch, ["applying..."]);
            insert_history_lines(terminal, cell.lines());
            append_log("[patch] applying...");
        }
        CoreEvent::PatchApplyEnd { success, .. } => {
            let status = if success { "ok" } else { "failed" };
            let cell = HistoryCell::new_system_status(SystemLabel::Patch, [status.to_string()]);
            insert_history_lines(terminal, cell.lines());
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
            app.bottom_pane.set_task_running(false);
            // 念のため残りをフラッシュ
            let tail = app.answer_stream.finalize();
            if !tail.is_empty() {
                insert_history_lines(terminal, tail);
            }
            append_log("[task] complete");
            app.app_event_tx.send(AppEvent::StopCommitAnimation);
            // simple inline notification (one-line)
            let should_show_note = !app.bottom_pane.is_intercepting_input();
            if should_show_note && !app.last_agent_preview.is_empty() {
                let note = format!("✓ {}", app.last_agent_preview);
                let cell = HistoryCell::new_system_status(SystemLabel::Info, [note]);
                insert_history_lines(terminal, cell.lines());
                app.last_agent_preview.clear();
            }
            // Approximate tokens (rough heuristic: 4 chars per token)
            if app.approx_output_chars > 0 {
                let approx_tokens = (app.approx_output_chars as f32 / 4.0).ceil() as u64;
                let line = format!("Token approx: ~{} tokens", approx_tokens);
                let cell = HistoryCell::new_system_status(SystemLabel::Info, [line]);
                insert_history_lines(terminal, cell.lines());
                app.approx_output_chars = 0;
            }
        }
        CoreEvent::Error { message } => {
            let cell = HistoryCell::new_system_status(SystemLabel::Error, [message.clone()]);
            insert_history_lines(terminal, cell.lines());
            app.status = RunStatus::Error;
            app.bottom_pane.set_task_running(false);
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
            app.bottom_pane
                .show_approval_modal(req, app.app_event_tx.clone());
            append_log("[approve] exec requested");
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
