use std::io::Result;
use std::io::Stdout;
use std::io::stdout;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use std::cell::RefCell;

use crossterm::Command;
use crossterm::event::Event;
use crossterm::event::KeyEvent;
use crossterm::event::EnableBracketedPaste;
use crossterm::event::DisableBracketedPaste;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use ratatui::backend::Backend;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::disable_raw_mode;
use ratatui::crossterm::terminal::enable_raw_mode;
use ratatui::layout::Offset;
use ratatui::text::Line;

use crate::custom_terminal;
use crate::custom_terminal::Terminal as CustomTerminal;
use tokio::select;
use tokio_stream::Stream;

/// A type alias for the terminal type used in this application
pub type Terminal = CustomTerminal<CrosstermBackend<Stdout>>;

pub fn set_modes() -> Result<()> {
    execute!(stdout(), EnableBracketedPaste)?;
    enable_raw_mode()?;
    Ok(())
}

pub fn clear_modes() -> Result<()> {
    execute!(stdout(), DisableBracketedPaste)?;
    disable_raw_mode()?;
    Ok(())
}

#[derive(Clone)]
pub struct FrameRequester {
    frame_schedule_tx: tokio::sync::mpsc::UnboundedSender<Instant>,
}

impl FrameRequester {
    pub fn schedule_frame(&self) {
        let _ = self.frame_schedule_tx.send(Instant::now());
    }
    
    pub fn schedule_frame_in(&self, dur: Duration) {
        let _ = self.frame_schedule_tx.send(Instant::now() + dur);
    }
}

// TuiEvent removed - using direct event handling

pub struct Tui {
    terminal: Terminal,
    pending_history_lines: Vec<Line<'static>>,
    alt_saved_viewport: Option<ratatui::layout::Rect>,
    alt_screen_active: Arc<AtomicBool>,
    frame_requester: FrameRequester,
    frame_schedule_rx: tokio::sync::mpsc::UnboundedReceiver<Instant>,
}

impl Tui {
    pub fn new() -> Result<Self> {
        let terminal = CustomTerminal::with_options(CrosstermBackend::new(stdout()))?;
        let (frame_schedule_tx, frame_schedule_rx) = tokio::sync::mpsc::unbounded_channel();
        
        Ok(Self {
            terminal,
            pending_history_lines: vec![],
            alt_saved_viewport: None,
            alt_screen_active: Arc::new(AtomicBool::new(false)),
            frame_requester: FrameRequester { frame_schedule_tx },
            frame_schedule_rx,
        })
    }

    pub fn frame_requester(&self) -> FrameRequester {
        self.frame_requester.clone()
    }

    pub fn insert_history_lines(&mut self, lines: Vec<Line<'static>>) {
        self.pending_history_lines.extend(lines);
        self.frame_requester().schedule_frame();
    }

    /// Enter alternate screen and expand the viewport to full terminal size, saving the current
    /// inline viewport for restoration when leaving.
    pub fn enter_alt_screen(&mut self) -> Result<()> {
        let _ = execute!(self.terminal.backend_mut(), EnterAlternateScreen);
        if let Ok(size) = self.terminal.size() {
            self.alt_saved_viewport = Some(self.terminal.viewport_area);
            self.terminal.set_viewport_area(ratatui::layout::Rect::new(
                0,
                0,
                size.width,
                size.height,
            ));
            let _ = self.terminal.clear();
        }
        self.alt_screen_active.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Leave alternate screen and restore the previously saved inline viewport, if any.
    pub fn leave_alt_screen(&mut self) -> Result<()> {
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        if let Some(saved) = self.alt_saved_viewport.take() {
            self.terminal.set_viewport_area(saved);
        }
        self.alt_screen_active.store(false, Ordering::Relaxed);
        Ok(())
    }

    pub fn draw(
        &mut self,
        height: u16,
        draw_fn: impl FnOnce(&mut custom_terminal::Frame),
    ) -> Result<()> {
        {
            let terminal = &mut self.terminal;
            let size = terminal.size()?;
            
            // Calculate input area
            let input_height = height.min(size.height.max(1));
            let area = ratatui::layout::Rect {
                x: 0,
                y: size.height.saturating_sub(input_height),
                width: size.width,
                height: input_height,
            };
            
            if area != terminal.viewport_area {
                terminal.set_viewport_area(area);
            }
            
            // Process pending history lines FIRST (correct order)
            if !self.pending_history_lines.is_empty() {
                crate::insert_history::insert_history_lines(
                    terminal,
                    self.pending_history_lines.clone(),
                );
                self.pending_history_lines.clear();
            }
            
            // Then draw UI
            terminal.draw(|frame| {
                draw_fn(frame);
            })?
        }
        Ok(())
    }

    // event_stream removed - using direct polling approach to avoid borrowing issues

    pub fn size(&self) -> Result<ratatui::layout::Size> {
        self.terminal.size()
    }

    pub fn clear(&mut self) -> Result<()> {
        self.terminal.clear()
    }

    pub fn show_cursor(&mut self) -> Result<()> {
        self.terminal.show_cursor()
    }

    pub fn backend_mut(&mut self) -> &mut CrosstermBackend<Stdout> {
        self.terminal.backend_mut()
    }

    pub fn flush(&mut self) -> Result<()> {
        self.terminal.backend_mut().flush()
    }
}
