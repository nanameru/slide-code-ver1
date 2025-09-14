use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::io;

pub struct SlidePreview {
    slides: Vec<String>,
    current_slide: usize,
    running: bool,
}

impl SlidePreview {
    pub fn new(slides: Vec<String>) -> Self {
        Self {
            slides,
            current_slide: 0,
            running: true,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        while self.running {
            terminal.draw(|f| self.draw(f))?;

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        self.running = false;
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.previous_slide();
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.next_slide();
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

    fn draw(&self, f: &mut Frame) {
        let size = f.area();

        let default_content = "No slide content".to_string();
        let current_content = self.slides.get(self.current_slide)
            .unwrap_or(&default_content);

        let title = format!(
            "Slide Preview ({}/{})",
            self.current_slide + 1,
            self.slides.len()
        );

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL);

        let paragraph = Paragraph::new(current_content.as_str())
            .block(block)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, size);
    }

    fn next_slide(&mut self) {
        if self.current_slide < self.slides.len().saturating_sub(1) {
            self.current_slide += 1;
        }
    }

    fn previous_slide(&mut self) {
        if self.current_slide > 0 {
            self.current_slide -= 1;
        }
    }
}