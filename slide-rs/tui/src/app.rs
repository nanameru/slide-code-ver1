use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use std::{io, path::PathBuf, time::Instant};
use std::io::Write as _;
use tokio::time::{sleep, Duration};

use crate::widgets::{
    chat::ChatWidget,
    composer::ComposerWidget,
    list_selection::ListSelection,
    modal::Modal,
    status_bar::StatusBar,
};
use crate::agent::AgentHandle;
use slide_core::codex::Event as CoreEvent;
use slide_core::codex::Op;
use crate::app_event_sender::{AppEvent, AppEventSender};
use crate::user_approval_widget::ApprovalRequest;
use crate::bottom_pane::{BottomPane, BottomPaneParams};

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

pub struct App {
    should_quit: bool,
    mode: Mode,
    status: RunStatus,
    last_tick: Instant,
    // Chat state
    messages: Vec<String>,
    input: String,
    // Chat scroll state (top line index within rendered message lines)
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
    // Bottom pane integration
    bottom_pane: BottomPane,
    // App event channel
    app_event_rx: tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    app_event_tx: AppEventSender,
}

impl App {
    pub fn new() -> Self {
        Self::new_with_recents(Vec::new())
    }

    fn clamp_scroll_top(&mut self) {
        let max_top = self.max_scroll_top();
        if self.chat_scroll_top > max_top { self.chat_scroll_top = max_top; }
    }

    fn max_scroll_top(&self) -> usize {
        let total_lines = if self.messages.is_empty() { 1 } else { self.messages.len() * 2 - 1 };
        total_lines.saturating_sub(self.chat_viewport_height)
    }

    fn follow_bottom_after_change(&mut self) {
        if self.chat_follow_bottom { self.chat_scroll_top = self.max_scroll_top(); }
        else { self.clamp_scroll_top(); }
    }

    pub fn new_with_recents(recent_files: Vec<String>) -> Self {
        let (app_tx_raw, app_rx) = tokio::sync::mpsc::unbounded_channel();
        let app_tx = AppEventSender::new(app_tx_raw);
        let s = Self {
            should_quit: false,
            mode: Mode::Normal,
            status: RunStatus::Idle,
            last_tick: Instant::now(),
            messages: vec![
                "Welcome to Slide TUI!".into(),
                "Type i to start composing, Enter to send.".into(),
                "Press h for help. Press q to quit.".into(),
            ],
            input: String::new(),
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
            bottom_pane: BottomPane::new(BottomPaneParams{ has_input_focus: true, placeholder_text: "Ask Slide Code to do anything".into()}),
            app_event_rx: app_rx,
            app_event_tx: app_tx,
        };
        // Write a small banner to the log so the browser viewer has content
        append_log("[info] Slide TUI session started");
        s
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    fn on_tick(&mut self) {
        // Simulate finishing a running task after 1.5s
        if self.status == RunStatus::Running && self.last_tick.elapsed() > Duration::from_millis(1500)
        {
            self.status = RunStatus::Idle;
        }
    }

    fn submit(&mut self) {
        if self.input.trim().is_empty() {
            return;
        }
        let text = self.input.trim().to_string();
        self.messages.push(format!("You: {}", text));
        append_log(&format!("You: {}", text));
        self.input.clear();
        // Simulate agent response
        self.status = RunStatus::Running;
        self.last_tick = Instant::now();
        if let Some(agent) = &self.agent {
            let to_send = text.clone();
            agent.submit_text_bg(to_send);
        }
        self.follow_bottom_after_change();
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // If popup is active, handle popup interactions first
        if let Some(kind) = self.active_popup {
            self.handle_popup_key(kind, key);
            return;
        }

        match self.mode {
            Mode::Insert => match key.code {
                KeyCode::Esc => self.mode = Mode::Normal,
                KeyCode::Enter => self.submit(),
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Char(c) => {
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                        self.input.push(c);
                    }
                }
                _ => {}
            },
            _ => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.quit(),
                KeyCode::Char('i') => self.mode = Mode::Insert,
                KeyCode::Char('h') => {
                    self.show_modal = !self.show_modal;
                }
                KeyCode::Char(':') => {
                    self.open_command_palette();
                }
                KeyCode::Char('/') => {
                    self.open_file_search();
                }
                KeyCode::Char('c') => {
                    self.messages.clear();
                    self.chat_scroll_top = 0;
                    self.chat_follow_bottom = true;
                }
                KeyCode::Enter => {
                    if self.show_modal {
                        self.show_modal = false;
                    }
                }
                _ => {}
            },
        }
    }

    fn on_mouse_wheel(&mut self, delta_lines: isize) {
        // Mouse scroll controls chat history only; disable follow-to-bottom on user scroll
        if delta_lines == 0 { return; }
        if delta_lines < 0 {
            // scroll down (towards bottom)
            self.chat_scroll_top = self.chat_scroll_top.saturating_add(delta_lines.unsigned_abs() as usize);
        } else {
            // scroll up (towards top)
            let dec = delta_lines as usize;
            self.chat_scroll_top = self.chat_scroll_top.saturating_sub(dec);
        }
        self.clamp_scroll_top();
        self.chat_follow_bottom = self.chat_scroll_top >= self.max_scroll_top();
    }

    fn open_command_palette(&mut self) {
        self.active_popup = Some(PopupKind::Command);
        self.popup_title = "Command Palette".into();
        let mut items = vec![
            "New Slide from Template".into(),
            "Open Slide Preview (from file)".into(),
            "Save Chat to slides/draft.md".into(),
            "Toggle Help".into(),
            "Clear Messages".into(),
            "Quit".into(),
        ];
        if !self.recent_files.is_empty() {
            items.push("— Recent —".into());
            for f in self.recent_files.iter().take(10) {
                items.push(format!("Open Recent: {}", f));
            }
        }
        self.popup_items = items;
        self.popup_filter.clear();
        self.popup_filtered_indices = (0..self.popup_items.len()).collect();
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

    fn handle_popup_key(&mut self, kind: PopupKind, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.active_popup = None;
            }
            KeyCode::Down => {
                if !self.popup_filtered_indices.is_empty() {
                    self.popup_selected = (self.popup_selected + 1).min(self.popup_filtered_indices.len().saturating_sub(1));
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
                if let Some(idx) = self.popup_filtered_indices.get(self.popup_selected).copied() {
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

    fn exec_command_palette(&mut self, idx: usize) {
        let cmd = &self.popup_items[idx];
        self.active_popup = None;
        match cmd.as_str() {
            "New Slide from Template" => {
                match create_slide_from_template() {
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
                }
            }
            "Open Slide Preview (from file)" => {
                self.open_file_search();
            }
            "Save Chat to slides/draft.md" => {
                match save_chat_as_draft(&self.messages) {
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
                self.messages.clear();
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
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new_with_recents(init_recent_files);
    // Spawn core agent
    match crate::agent::AgentHandle::spawn().await {
        Ok(agent) => app.agent = Some(agent),
        Err(_e) => {
            app.messages.push("(failed to start agent; using local demo)".into());
        }
    }

    loop {
        // Drain app events from UI widgets
        while let Ok(ev) = app.app_event_rx.try_recv() {
            match ev {
                AppEvent::ExecApproval { id, decision } => {
                    if let Some(agent) = &app.agent {
                        let c = agent.codex.clone();
                        tokio::spawn(async move { let _ = c.submit(Op::ExecApproval { id, decision }).await; });
                    }
                }
                AppEvent::PatchApproval { id, decision } => {
                    if let Some(agent) = &app.agent {
                        let c = agent.codex.clone();
                        tokio::spawn(async move { let _ = c.submit(Op::PatchApproval { id, decision }).await; });
                    }
                }
            }
        }

        // Update chat viewport height based on terminal size (header=1, composer=3, status=1)
        if let Ok(sz) = terminal.size() { app.chat_viewport_height = sz.height.saturating_sub(1 + 3 + 1) as usize; }

        // Draw UI
        terminal.draw(|f| ui(f, &app))?;

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Mouse(mev) => {
                    match mev.kind {
                        MouseEventKind::ScrollUp => app.on_mouse_wheel(3),
                        MouseEventKind::ScrollDown => app.on_mouse_wheel(-3),
                        _ => {}
                    }
                }
                Event::Key(key) => {
                    // If bottom pane has an active view, let it intercept first
                    if app.bottom_pane.is_intercepting_input() || matches!(app.mode, Mode::Insert) {
                        if let Some(res) = app.bottom_pane.handle_key_event(key) {
                            if let crate::bottom_pane::InputResult::Submitted(text) = res {
                                if !text.is_empty() { app.messages.push(format!("You: {}", text)); }
                                if let Some(agent) = &app.agent {
                                    let to_send = text.clone();
                                    agent.submit_text_bg(to_send);
                                }
                            }
                        }
                    } else {
                        app.handle_key_event(key);
                    }
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
            handle_core_event(&mut app, ev);
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
    execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let exit = if let Some(path) = app.preview_path { AppExit::Preview(path) } else { AppExit::Quit };
    Ok(RunResult { exit, recent_files: app.recent_files })
}

fn ui(f: &mut Frame, app: &App) {
    // Layout: header | body | composer | status
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled("Slide TUI ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("— Interactive Mode"),
    ]));
    f.render_widget(header, chunks[0]);

    // Body: single full-width chat area (help panel removed)
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100)])
        .split(chunks[1]);

    let chat_height = body[0].height as usize;
    // store viewport height to compute max scroll top & follow-bottom updates
    let mut app_ref = unsafe { (app as *const App as *mut App).as_mut().unwrap() };
    app_ref.chat_viewport_height = chat_height;
    let chat_widget = ChatWidget::new(&app.messages).with_scroll(app.chat_scroll_top, chat_height);
    f.render_widget(chat_widget, body[0]);

    // Composer/bottom pane area
    // Render bottom pane regardless of mode; focusは modeに応じて将来切替
    app.bottom_pane.render_ref(chunks[2], f.buffer_mut());

    // Status bar
    let status = match app.status {
        RunStatus::Idle => "Idle",
        RunStatus::Running => "Running…",
        RunStatus::Error => "Error",
    };
    let mode = match app.mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Help => "HELP",
    };
    let status_bar = StatusBar::new(mode, status, "h:help  i:insert  q:quit");
    f.render_widget(status_bar, chunks[3]);

    // Modal overlay
    if app.show_modal {
        let area = centered_rect(60, 60, f.area());
        let modal = Modal::new(&app.modal_title, &app.modal_body);
        f.render_widget(Clear, area);
        f.render_widget(modal, area);
    }

    // Popups
    if let Some(_kind) = app.active_popup {
        let area = centered_rect(70, 70, f.area());
        // Build filtered view
        let items: Vec<String> = app
            .popup_filtered_indices
            .iter()
            .map(|&i| app.popup_items[i].clone())
            .collect();
        let widget = ListSelection::new(
            &app.popup_title,
            &app.popup_filter,
            &items,
            app.popup_selected,
            "Type to filter • Esc: close • Enter: select • ↑/↓: move",
        );
        widget.render(f, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1]);

    horizontal[1]
}

fn handle_core_event(app: &mut App, ev: CoreEvent) {
    match ev {
        CoreEvent::SessionConfigured { .. } => {}
        CoreEvent::TaskStarted => {
            app.status = RunStatus::Running;
            append_log("[task] started");
        }
        CoreEvent::AgentMessageDelta { delta } => {
            if let Some(last) = app.messages.last_mut() {
                if last.starts_with("Assistant:") {
                    last.push_str(&delta);
                    app.follow_bottom_after_change();
                    return;
                }
            }
            app.messages.push(format!("Assistant: {}", delta));
            append_log(&format!("AssistantΔ: {}", delta));
            app.follow_bottom_after_change();
        }
        CoreEvent::AgentMessage { message } => {
            app.messages.push(format!("Assistant: {}", message));
            append_log(&format!("Assistant: {}", message));
            app.follow_bottom_after_change();
        }
        CoreEvent::ExecCommandBegin { command, .. } => {
            app.messages.push(format!("[exec] $ {}", command.join(" ")));
            append_log(&format!("[exec] $ {}", command.join(" ")));
            app.follow_bottom_after_change();
        }
        CoreEvent::ExecCommandEnd { exit_code, .. } => {
            app.messages.push(format!("[exec] exit {}", exit_code));
            append_log(&format!("[exec] exit {}", exit_code));
            app.follow_bottom_after_change();
        }
        CoreEvent::ApplyPatchApprovalRequest { id, changes, reason } => {
            // Convert map of path->desc into a vector of display strings
            let mut items: Vec<String> = changes
                .into_iter()
                .map(|(p, v)| format!("{}: {}", p.display(), v))
                .collect();
            items.sort();
            let req = ApprovalRequest::Patch { id, changes: items, reason };
            app.bottom_pane.show_approval_modal(req, app.app_event_tx.clone());
            append_log("[approve] apply_patch requested");
        }
        CoreEvent::PatchApplyBegin { .. } => {
            app.messages.push("[patch] applying...".into());
            append_log("[patch] applying...");
            app.follow_bottom_after_change();
        }
        CoreEvent::PatchApplyEnd { success, .. } => {
            app.messages.push(format!("[patch] {}", if success { "ok" } else { "failed" }));
            append_log(&format!("[patch] {}", if success { "ok" } else { "failed" }));
            app.follow_bottom_after_change();
        }
        CoreEvent::TurnDiff { unified_diff } => {
            app.messages.push(format!("[diff]\n{}", unified_diff));
            append_log("[diff] updated");
            app.follow_bottom_after_change();
        }
        CoreEvent::TaskComplete => {
            app.status = RunStatus::Idle;
            append_log("[task] complete");
        }
        CoreEvent::Error { message } => {
            app.messages.push(format!("[error] {}", message));
            app.status = RunStatus::Error;
            append_log(&format!("[error] {}", message));
            app.follow_bottom_after_change();
        }
        CoreEvent::ShutdownComplete => {}
        CoreEvent::ExecApprovalRequest { id, command, cwd: _, reason } => {
            let req = ApprovalRequest::Exec { id, command, reason };
            app.bottom_pane.show_approval_modal(req, app.app_event_tx.clone());
            append_log("[approve] exec requested");
        }
    }
}

fn append_log(line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/slide.log") {
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

fn save_chat_as_draft(messages: &[String]) -> std::io::Result<String> {
    use std::io::Write;
    let dir = std::path::Path::new("slides");
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }
    let path = dir.join("draft.md");
    let mut file = std::fs::File::create(&path)?;
    for m in messages {
        writeln!(file, "- {}", m)?;
    }
    Ok(path.to_string_lossy().to_string())
}
