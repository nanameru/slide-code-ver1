use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use slide_core::{
    client::{OpenAiAdapter, StubClient},
    codex2::Codex,
};
use std::io;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{interval, Duration};

const RADAR_FRAMES: [&str; 8] = [
    "レーダー [*    ]",
    "レーダー [ *   ]",
    "レーダー [  *  ]",
    "レーダー [   * ]",
    "レーダー [    *]",
    "レーダー [   * ]",
    "レーダー [  *  ]",
    "レーダー [ *   ]",
];
const STATUS_PREFIX: &str = "▌ /Users/kimurataiyou/slide-code-test";

#[derive(Copy, Clone)]
enum AssistantPhase {
    Thinking { started_at: Instant },
    Streaming { started_at: Instant },
}

pub struct InteractiveApp {
    running: bool,
    input: String,
    messages: Vec<String>,
    codex: Option<Codex>,
    assistant_response_index: Option<usize>,
    assistant_phase: Option<AssistantPhase>,
    tool_blocks_in_current_message: Vec<String>,
}

impl InteractiveApp {
    pub fn new() -> Self {
        Self {
            running: true,
            input: String::new(),
            messages: Vec::new(),
            codex: None,
            assistant_response_index: None,
            assistant_phase: None,
            tool_blocks_in_current_message: Vec::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // Initialize Codex system
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        let client: Arc<dyn slide_core::client::ModelClient + Send + Sync> = if api_key.is_empty() {
            Arc::new(StubClient)
        } else {
            Arc::new(OpenAiAdapter::new(api_key))
        };

        let codex_result = Codex::spawn(client).await?;
        self.codex = Some(codex_result.codex);

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let mut tick = interval(Duration::from_millis(120));

        while self.running {
            terminal.draw(|f| self.draw(f))?;

            // Prefer non-blocking, responsive multiplexing of input and codex events
            tokio::select! {
                _ = tick.tick() => {
                    // periodic refresh for animations
                }
                // Handle model events if codex is available
                maybe_ev = async {
                    if let Some(c) = self.codex.clone() { c.next_event().await } else { None }
                }, if self.codex.is_some() => {
                    if let Some(ev) = maybe_ev {
                        self.handle_event(ev);
                    }
                }
                // Handle terminal key events (non-blocking poll in blocking thread)
                event_result = tokio::task::spawn_blocking(|| event::poll(std::time::Duration::from_millis(0))) => {
                    if let Ok(Ok(true)) = event_result {
                        if let Ok(ev) = event::read() {
                            if let Event::Key(key) = ev {
                                match key.code {
                                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                        self.running = false;
                                    }
                                    KeyCode::Enter => {
                                        if key.modifiers.intersects(
                                            KeyModifiers::SHIFT | KeyModifiers::ALT,
                                        ) {
                                            self.input.push('\n');
                                        } else {
                                            self.submit_input().await;
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        self.input.push(c);
                                    }
                                    KeyCode::Backspace => {
                                        self.input.pop();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        Ok(())
    }

    async fn submit_input(&mut self) {
        if !self.input.trim().is_empty() {
            let input_text = self.input.clone();
            self.messages.push(format!("You: {}", input_text));
            self.input.clear();

            if let Some(ref codex) = self.codex {
                self.tool_blocks_in_current_message.clear();
                self.assistant_response_index = Some(self.messages.len());
                self.messages.push("Assistant:".to_string());
                self.assistant_phase = Some(AssistantPhase::Thinking {
                    started_at: Instant::now(),
                });
                let _ = codex
                    .submit(slide_core::codex2::Op::UserInput { text: input_text })
                    .await;
            }
        }
    }

    fn handle_event(&mut self, ev: slide_core::codex2::Event) {
        match ev {
            slide_core::codex2::Event::AgentMessageDelta { delta } => {
                let handled_as_tool = self.handle_tool_delta(&delta);

                if !handled_as_tool {
                    let idx = if let Some(idx) = self.assistant_response_index {
                        idx
                    } else {
                        let idx = self.messages.len();
                        self.messages.push("Assistant:".to_string());
                        self.assistant_response_index = Some(idx);
                        idx
                    };

                    if let Some(message) = self.messages.get_mut(idx) {
                        if message.ends_with(':') && !delta.is_empty() {
                            message.push(' ');
                        }
                        message.push_str(&delta);
                    }
                }

                self.assistant_phase = Some(match self.assistant_phase {
                    Some(AssistantPhase::Thinking { started_at }) => {
                        AssistantPhase::Streaming { started_at }
                    }
                    Some(AssistantPhase::Streaming { started_at }) => {
                        AssistantPhase::Streaming { started_at }
                    }
                    None => AssistantPhase::Streaming {
                        started_at: Instant::now(),
                    },
                });
            }
            slide_core::codex2::Event::AgentMessage { message } => {
                let clean_message = self
                    .sanitize_agent_message(&message)
                    .unwrap_or_else(|| message.clone());
                if let Some(idx) = self.assistant_response_index {
                    if let Some(slot) = self.messages.get_mut(idx) {
                        *slot = if clean_message.trim().is_empty() {
                            "Assistant: (ツール出力のみ)".to_string()
                        } else {
                            format!("Assistant: {}", clean_message)
                        };
                    } else {
                        if clean_message.trim().is_empty() {
                            self.messages
                                .push("Assistant: (ツール出力のみ)".to_string());
                        } else {
                            self.messages.push(format!("Assistant: {}", clean_message));
                        }
                    }
                } else {
                    if clean_message.trim().is_empty() {
                        self.messages
                            .push("Assistant: (ツール出力のみ)".to_string());
                    } else {
                        self.messages.push(format!("Assistant: {}", clean_message));
                    }
                }
                self.assistant_phase = None;
                self.assistant_response_index = None;
                self.tool_blocks_in_current_message.clear();
            }
            slide_core::codex2::Event::TaskComplete => {
                self.assistant_phase = None;
                self.tool_blocks_in_current_message.clear();
            }
            slide_core::codex2::Event::Error { message } => {
                if let Some(idx) = self.assistant_response_index {
                    if idx < self.messages.len() {
                        self.messages.remove(idx);
                    }
                }
                self.assistant_response_index = None;
                self.assistant_phase = None;
                self.messages.push(format!("Error: {}", message));
                self.tool_blocks_in_current_message.clear();
            }
            _ => {}
        }
    }

    fn draw(&self, f: &mut Frame) {
        let size = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(self.input_height(size.width)),
            ])
            .split(size);

        // Status line
        let status_line = self.current_indicator().unwrap_or_default();
        let status_widget = Paragraph::new(status_line);
        f.render_widget(status_widget, chunks[0]);

        // Messages area
        let messages: Vec<ListItem> = self
            .messages
            .iter()
            .map(|m| ListItem::new(m.clone()))
            .collect();

        let messages_list =
            List::new(messages).block(Block::default().title("Chat").borders(Borders::ALL));

        f.render_widget(messages_list, chunks[1]);

        // Input area
        let input_paragraph = Paragraph::new(self.input.as_str())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title("Input (Ctrl+Q to quit)")
                    .borders(Borders::ALL),
            );

        f.render_widget(input_paragraph, chunks[2]);
    }

    fn input_height(&self, available_width: u16) -> u16 {
        use unicode_width::UnicodeWidthChar;

        const MIN_HEIGHT: u16 = 3;
        if available_width <= 4 {
            return MIN_HEIGHT;
        }

        let inner_width = available_width.saturating_sub(2).max(1);
        let mut lines: u16 = 1;
        let mut current_width: u16 = 0;

        for ch in self.input.chars() {
            if ch == '\n' {
                lines = lines.saturating_add(1);
                current_width = 0;
                continue;
            }

            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
            if ch_width == 0 {
                continue;
            }

            if current_width + ch_width > inner_width {
                lines = lines.saturating_add(1);
                current_width = ch_width;
            } else {
                current_width += ch_width;
            }
        }

        lines
            .saturating_add(2) // account for the border rendered by Block
            .max(MIN_HEIGHT)
    }

    fn current_indicator(&self) -> Option<String> {
        let phase = self.assistant_phase?;
        let (label, started_at) = match phase {
            AssistantPhase::Thinking { started_at } => ("thinking...", started_at),
            AssistantPhase::Streaming { started_at } => ("generating...", started_at),
        };
        let frame = Self::radar_frame(started_at);
        Some(format!("{} {} {}", STATUS_PREFIX, label, frame))
    }

    fn radar_frame(started_at: Instant) -> &'static str {
        let elapsed = Instant::now().saturating_duration_since(started_at);
        let frame_index = ((elapsed.as_millis() / 120) as usize) % RADAR_FRAMES.len();
        RADAR_FRAMES[frame_index]
    }

    fn handle_tool_delta(&mut self, delta: &str) -> bool {
        let trimmed = delta.trim();
        let (prefix, content) = if let Some(rest) = trimmed.strip_prefix("[Tool Execution Result]")
        {
            ("ツール結果", rest)
        } else if let Some(rest) = trimmed.strip_prefix("[Tool Execution]") {
            ("ツール実行", rest)
        } else {
            return false;
        };

        let content = normalize_tool_content(content);
        let display = if content.contains('\n') {
            format!("{prefix}:\n{content}")
        } else if content.is_empty() {
            format!("{prefix}")
        } else {
            format!("{prefix}: {content}")
        };

        self.tool_blocks_in_current_message.push(delta.to_string());
        self.messages.push(display);
        true
    }

    fn sanitize_agent_message(&self, message: &str) -> Option<String> {
        if self.tool_blocks_in_current_message.is_empty() {
            return Some(message.trim().to_string());
        }

        let mut sanitized = message.to_string();
        for raw in &self.tool_blocks_in_current_message {
            if raw.is_empty() {
                continue;
            }
            sanitized = sanitized.replace(raw, "");
            let trimmed = raw.trim();
            if trimmed != raw {
                sanitized = sanitized.replace(trimmed, "");
            }
        }

        while sanitized.contains("\n\n\n") {
            sanitized = sanitized.replace("\n\n\n", "\n\n");
        }

        let trimmed = sanitized.trim();
        Some(trimmed.to_string())
    }
}

fn normalize_tool_content(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    trimmed
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}
