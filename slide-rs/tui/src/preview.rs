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
    text::Text,
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::io;
use tokio::time::{sleep, Duration};

pub struct SlidePreview {
    slides: Vec<String>,
    current_slide: usize,
    should_quit: bool,
}

impl SlidePreview {
    pub fn new(slides: Vec<String>) -> Self {
        Self {
            slides,
            current_slide: 0,
            should_quit: false,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            // Draw UI
            terminal.draw(|f| self.ui(f))?;

            // Handle events
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key_event(key.code);
                    }
                }
            }

            if self.should_quit {
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

    fn handle_key_event(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Right | KeyCode::Char('j') => {
                if self.current_slide < self.slides.len() - 1 {
                    self.current_slide += 1;
                }
            }
            KeyCode::Left | KeyCode::Char('k') => {
                if self.current_slide > 0 {
                    self.current_slide -= 1;
                }
            }
            KeyCode::Home => {
                self.current_slide = 0;
            }
            KeyCode::End => {
                self.current_slide = self.slides.len().saturating_sub(1);
            }
            _ => {}
        }
    }

    fn ui(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.area());

        // Header with slide counter
        let title = format!(
            "Slide Preview ({}/{})", 
            self.current_slide + 1, 
            self.slides.len()
        );
        let header = Paragraph::new(title)
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(header, chunks[0]);

        // Current slide content
        let slide_content = if !self.slides.is_empty() {
            Text::from(self.slides[self.current_slide].as_str())
        } else {
            Text::from("No slides available")
        };
        
        let slide = Paragraph::new(slide_content)
            .block(Block::default().borders(Borders::ALL).title("Slide Content"))
            .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(slide, chunks[1]);

        // Footer with controls
        let controls = "Navigation: ← → (or j k) | Home/End | Press 'q' to quit";
        let footer = Paragraph::new(controls)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(footer, chunks[2]);
    }
}