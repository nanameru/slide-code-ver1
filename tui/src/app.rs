use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::io;
use tokio::time::{Duration, interval};

pub struct App {
    pub running: bool,
    pub content: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            content: "Welcome to Slide TUI!\nPress 'q' to quit.".to_string(),
        }
    }

    pub fn update_content(&mut self, content: String) {
        self.content = content;
    }

    pub fn quit(&mut self) {
        self.running = false;
    }
}

pub async fn run_app() -> Result<()> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let mut tick_interval = interval(Duration::from_millis(250));

    loop {
        terminal.draw(|f| ui(f, &app))?;

        tokio::select! {
            _ = tick_interval.tick() => {
                // Regular tick for updates
            }
            event_result = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(0))) => {
                if let Ok(Ok(true)) = event_result {
                    if let Ok(event) = event::read() {
                        match event {
                            Event::Key(key) => {
                                match key.code {
                                    KeyCode::Char('q') | KeyCode::Esc => {
                                        app.quit();
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if !app.running {
            break;
        }
    }

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.area();

    let block = Block::default()
        .title("Slide TUI")
        .borders(Borders::ALL);

    let paragraph = Paragraph::new(app.content.as_str())
        .block(block)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, size);
}