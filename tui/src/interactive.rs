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
}

impl InteractiveApp {
    pub fn new() -> Self {
        Self {
            running: true,
            input: String::new(),
            messages: Vec::new(),
            codex: None,
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

        // Start event processing
        let codex_clone = self.codex.clone();
        tokio::spawn(async move {
            if let Some(codex) = codex_clone {
                while let Some(event) = codex.next_event().await {
                    match event {
                        slide_core::codex2::Event::AgentMessageDelta { delta } => {
                            // Handle streaming response
                        }
                        slide_core::codex2::Event::TaskComplete => {
                            // Task completed
                        }
                        slide_core::codex2::Event::Error { message } => {
                            eprintln!("Error: {}", message);
                        }
                        _ => {}
                    }
                }
            }
        });

        while self.running {
            terminal.draw(|f| self.draw(f))?;

            if let Event::Key(key) = event::read()? {
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