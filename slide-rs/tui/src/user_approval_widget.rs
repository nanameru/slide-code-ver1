use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    prelude::Widget,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, WidgetRef},
};

use crate::app_event_sender::{AppEvent, AppEventSender};
use crate::bottom_pane::scroll_state::ScrollState;
use crate::bottom_pane::selection_popup_common::{render_rows, GenericDisplayRow};
use slide_core::codex::ReviewDecision;

#[derive(Clone, Debug)]
pub enum ApprovalRequest {
    Exec {
        id: String,
        command: Vec<String>,
        reason: Option<String>,
    },
    Patch {
        id: String,
        changes: Vec<String>,
        reason: Option<String>,
    },
}

pub struct UserApprovalWidget {
    request: ApprovalRequest,
    complete: bool,
    tx: AppEventSender,
    scroll: ScrollState,
    page_size_hint: usize,
}

impl UserApprovalWidget {
    pub fn new(request: ApprovalRequest, tx: AppEventSender) -> Self {
        let mut scroll = ScrollState::new();
        if matches!(request, ApprovalRequest::Patch { .. }) {
            scroll.selected_idx = Some(0);
        }
        Self {
            request,
            complete: false,
            tx,
            scroll,
            page_size_hint: 10,
        }
    }
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.emit_decision(ReviewDecision::Approved);
                self.complete = true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.emit_decision(ReviewDecision::Denied);
                self.complete = true;
            }
            KeyCode::Esc => {
                self.emit_decision(ReviewDecision::Abort);
                self.complete = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection_up(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection_down(1);
            }
            KeyCode::PageUp => {
                let step = self.page_size_hint.max(1);
                self.move_selection_up(step);
            }
            KeyCode::PageDown => {
                let step = self.page_size_hint.max(1);
                self.move_selection_down(step);
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.select_home();
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.select_end();
            }
            _ => {}
        }
    }
    pub fn on_ctrl_c(&mut self) {
        self.complete = true;
    }
    pub fn is_complete(&self) -> bool {
        self.complete
    }
    pub fn desired_height(&self, _width: u16) -> u16 {
        10
    }
}

impl WidgetRef for &UserApprovalWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title("Approval Required");
        let inner_area = block.inner(area);
        block.render(area, buf);

        let areas = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas::<3>(inner_area);
        // Header
        let header_line: Line = match &self.request {
            ApprovalRequest::Exec {
                command, reason, ..
            } => {
                let mut spans = vec![
                    Span::styled("$ ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(command.join(" ")),
                ];
                if let Some(r) = reason {
                    spans.push(Span::raw("  — "));
                    spans.push(Span::styled(
                        r.clone(),
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
                Line::from(spans)
            }
            ApprovalRequest::Patch { reason, .. } => {
                let mut spans = vec![Span::styled(
                    "apply_patch changes",
                    Style::default().add_modifier(Modifier::BOLD),
                )];
                if let Some(r) = reason {
                    spans.push(Span::raw("  — "));
                    spans.push(Span::styled(
                        r.clone(),
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
                Line::from(spans)
            }
        };
        Paragraph::new(header_line).render(areas[0], buf);

        // Body rows (scrollable)
        let rows_area = areas[1];
        if rows_area.height > 0 {
            let rows_all: Vec<GenericDisplayRow> = match &self.request {
                ApprovalRequest::Exec { command, .. } => {
                    vec![GenericDisplayRow {
                        name: format!("$ {}", command.join(" ")),
                        match_indices: None,
                        is_current: false,
                        description: None,
                    }]
                }
                ApprovalRequest::Patch { changes, .. } => changes
                    .iter()
                    .map(|ch| GenericDisplayRow {
                        name: ch.clone(),
                        match_indices: None,
                        is_current: false,
                        description: None,
                    })
                    .collect(),
            };
            render_rows(rows_area, buf, &rows_all, &self.scroll, usize::MAX, true);
        }

        // Footer
        let footer = Rect {
            x: inner_area.x,
            y: inner_area.y + inner_area.height.saturating_sub(1),
            width: inner_area.width,
            height: 1,
        };
        let mut footer_spans = vec![
            Span::styled(" y ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": approve   "),
            Span::styled(" n ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": deny   "),
            Span::styled(" Esc ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": close"),
        ];
        if matches!(self.request, ApprovalRequest::Patch { .. }) {
            footer_spans.push(Span::raw("    "));
            footer_spans.push(Span::styled(
                "↑/↓ PgUp/PgDn Home/End j/k g/G",
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        Paragraph::new(Line::from(footer_spans)).render(footer, buf);
    }
}

impl UserApprovalWidget {
    fn emit_decision(&self, decision: ReviewDecision) {
        match &self.request {
            ApprovalRequest::Exec { id, .. } => self.tx.send(AppEvent::ExecApproval {
                id: id.clone(),
                decision,
            }),
            ApprovalRequest::Patch { id, .. } => self.tx.send(AppEvent::PatchApproval {
                id: id.clone(),
                decision,
            }),
        }
    }

    fn total_lines(&self) -> usize {
        match &self.request {
            ApprovalRequest::Exec { .. } => 1,
            ApprovalRequest::Patch { changes, .. } => changes.len(),
        }
    }

    fn ensure_clamped(&mut self) {
        let len = self.total_lines();
        self.scroll.clamp_selection(len);
        let window = self.page_size_hint.max(1);
        self.scroll.ensure_visible(len, window);
    }

    fn move_selection_up(&mut self, step: usize) {
        if !matches!(self.request, ApprovalRequest::Patch { .. }) {
            return;
        }
        let len = self.total_lines();
        if len == 0 {
            return;
        }
        let cur = self.scroll.selected_idx.unwrap_or(0);
        let new_idx = cur.saturating_sub(step);
        self.scroll.selected_idx = Some(new_idx);
        self.ensure_clamped();
    }

    fn move_selection_down(&mut self, step: usize) {
        if !matches!(self.request, ApprovalRequest::Patch { .. }) {
            return;
        }
        let len = self.total_lines();
        if len == 0 {
            return;
        }
        let cur = self.scroll.selected_idx.unwrap_or(0);
        let new_idx = (cur + step).min(len.saturating_sub(1));
        self.scroll.selected_idx = Some(new_idx);
        self.ensure_clamped();
    }

    fn select_home(&mut self) {
        if !matches!(self.request, ApprovalRequest::Patch { .. }) {
            return;
        }
        self.scroll.selected_idx = Some(0);
        self.scroll.scroll_top = 0;
    }

    fn select_end(&mut self) {
        if !matches!(self.request, ApprovalRequest::Patch { .. }) {
            return;
        }
        let len = self.total_lines();
        if len == 0 {
            self.scroll.selected_idx = None;
            self.scroll.scroll_top = 0;
            return;
        }
        self.scroll.selected_idx = Some(len - 1);
        self.scroll.scroll_top = self
            .scroll
            .scroll_top
            .max(len.saturating_sub(self.page_size_hint.max(1)));
    }
}
