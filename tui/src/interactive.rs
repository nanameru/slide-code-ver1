use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap, List, ListItem},
};
use std::io;
use slide_core::{client::{OpenAiAdapter, StubClient}, codex2::Codex};
use tokio::time::{Duration, sleep};
use std::sync::Arc;

pub struct InteractiveApp {
    running: bool,
    input: String,
    messages: Vec<String>,
    codex: Option<Codex>,
    // Buffer for streaming assistant message
    assistant_streaming: bool,
}

impl InteractiveApp {
    pub fn new() -> Self {
        Self {
            running: true,
            input: String::new(),
            messages: Vec::new(),
            codex: None,
            assistant_streaming: false,
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

        while self.running {
            terminal.draw(|f| self.draw(f))?;

            // Prefer non-blocking, responsive multiplexing of input and codex events
            tokio::select! {
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
                                        self.submit_input().await;
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
                let _ = codex.submit(slide_core::codex2::Op::UserInput { 
                    text: input_text 
                }).await;
            }
        }
    }

    fn handle_event(&mut self, ev: slide_core::codex2::Event) {
        match ev {
            slide_core::codex2::Event::AgentMessageDelta { delta } => {
                if !self.assistant_streaming {
                    self.messages.push("Assistant: ".to_string());
                    self.assistant_streaming = true;
                }
                if let Some(last) = self.messages.last_mut() {
                    last.push_str(&delta);
                }
            }
            slide_core::codex2::Event::AgentMessage { message } => {
                self.messages.push(format!("Assistant: {}", message));
                self.assistant_streaming = false;
            }
            slide_core::codex2::Event::TaskComplete => {
                // Close streaming message if any
                self.assistant_streaming = false;
            }
            slide_core::codex2::Event::Error { message } => {
                self.messages.push(format!("Error: {}", message));
                self.assistant_streaming = false;
            }
            _ => {}
        }
    }

    fn draw(&self, f: &mut Frame) {
        let size = f.area();
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(3)])
            .split(size);

        // Messages area
        let messages: Vec<ListItem> = self.messages
            .iter()
            .map(|m| ListItem::new(m.as_str()))
            .collect();

        let messages_list = List::new(messages)
            .block(Block::default().title("Chat").borders(Borders::ALL));

        f.render_widget(messages_list, chunks[0]);

        // Input area
        let input_paragraph = Paragraph::new(self.input.as_str())
            .block(Block::default().title("Input (Ctrl+Q to quit)").borders(Borders::ALL));

        f.render_widget(input_paragraph, chunks[1]);
    }
}
