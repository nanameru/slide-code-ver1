use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::io;
use tokio::time::{sleep, Duration};

pub struct App {
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self { should_quit: false }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn handle_key_event(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.quit(),
            _ => {}
        }
    }
}

pub async fn run_app() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    loop {
        // Draw UI
        terminal.draw(|f| ui(f, &app))?;

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key_event(key.code);
                }
            }
        }

        if app.should_quit {
            break;
        }

        // Small delay to prevent high CPU usage
        sleep(Duration::from_millis(16)).await;
    }

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame, _app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new("Slide CLI - Interactive Mode")
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Main content
    let content = Text::from(vec![
        Line::from("Welcome to Slide CLI!"),
        Line::from(""),
        Line::from("This is the interactive mode where you can:"),
        Line::from("• Chat with AI to create slides"),
        Line::from("• Preview generated markdown"),
        Line::from("• Save slides to your slides/ directory"),
        Line::from(""),
        Line::from("Press 'q' to quit"),
    ]);
    let main = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("Main"));
    f.render_widget(main, chunks[1]);

    // Footer
    let footer = Paragraph::new("Press 'q' to quit")
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}