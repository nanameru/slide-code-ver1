use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, WidgetRef},
};

use crate::app_event_sender::AppEventSender;

#[derive(Clone, Debug)]
pub enum ApprovalRequest {
    Exec { id: String, command: Vec<String>, reason: Option<String> },
    Patch { id: String, changes: Vec<String>, reason: Option<String> },
}

pub struct UserApprovalWidget {
    request: ApprovalRequest,
    complete: bool,
    _tx: AppEventSender,
}

impl UserApprovalWidget {
    pub fn new(request: ApprovalRequest, tx: AppEventSender) -> Self {
        Self { request, complete: false, _tx: tx }
    }
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => { self.complete = true; }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => { self.complete = true; }
            _ => {}
        }
    }
    pub fn on_ctrl_c(&mut self) { self.complete = true; }
    pub fn is_complete(&self) -> bool { self.complete }
    pub fn desired_height(&self, _width: u16) -> u16 { 7 }
}

impl WidgetRef for &UserApprovalWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::ALL).border_type(BorderType::Double).title("Approval Required");
        block.render(area, buf);

        let inner = Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).areas(block.inner(area))[1];
        let lines: Vec<Span> = match &self.request {
            ApprovalRequest::Exec { command, reason, .. } => {
                let mut v = vec![Span::raw(format!("$ {}", command.join(" ")))] ;
                if let Some(r) = reason { v.push(Span::raw(format!("  — {}", r))); }
                v
            }
            ApprovalRequest::Patch { changes, reason, .. } => {
                let mut v = vec![Span::raw("apply_patch changes:")];
                for ch in changes.iter().take(3) { v.push(Span::raw(format!("  - {}", ch))); }
                if changes.len() > 3 { v.push(Span::raw(format!("  (+{} more)", changes.len()-3))); }
                if let Some(r) = reason { v.push(Span::raw(format!("  — {}", r))); }
                v
            }
        };
        Paragraph::new(Line::from(lines)).render(inner, buf);

        let footer = Rect { x: inner.x, y: inner.y + inner.height.saturating_sub(1), width: inner.width, height: 1 };
        Paragraph::new(Line::from(vec![
            Span::styled(" y ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": approve   "),
            Span::styled(" n ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": deny   "),
            Span::styled(" Esc ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(": close"),
        ])).render(footer, buf);
    }
}

