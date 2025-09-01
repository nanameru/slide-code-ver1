use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{buffer::Buffer, layout::Rect, style::{Modifier, Style}, text::{Line, Span}, widgets::{Paragraph, Widget}};

use super::{bottom_pane_view::BottomPaneView, popup_consts::MAX_POPUP_ROWS, scroll_state::ScrollState, selection_popup_common::{GenericDisplayRow, render_rows}, BottomPane, CancellationEvent, AppEventSender};

pub(crate) type SelectionAction = Box<dyn Fn(&AppEventSender) + Send + Sync>;

pub(crate) struct SelectionItem { pub name: String, pub description: Option<String>, pub is_current: bool, pub actions: Vec<SelectionAction> }

pub(crate) struct ListSelectionView {
    title: String,
    subtitle: Option<String>,
    footer_hint: Option<String>,
    items: Vec<SelectionItem>,
    state: ScrollState,
    complete: bool,
    app_event_tx: AppEventSender,
}

impl ListSelectionView {
    fn dim_prefix_span() -> Span<'static> { Span::styled("▌ ", Style::default().add_modifier(Modifier::DIM)) }
    fn render_dim_prefix_line(area: Rect, buf: &mut Buffer) { let para = Paragraph::new(Line::from(Self::dim_prefix_span())); para.render(area, buf); }
    pub fn new(title: String, subtitle: Option<String>, footer_hint: Option<String>, items: Vec<SelectionItem>, app_event_tx: AppEventSender) -> Self {
        let mut s = Self { title, subtitle, footer_hint, items, state: ScrollState::new(), complete: false, app_event_tx };
        let len = s.items.len();
        if let Some(idx) = s.items.iter().position(|it| it.is_current) { s.state.selected_idx = Some(idx); }
        s.state.clamp_selection(len); s.state.ensure_visible(len, MAX_POPUP_ROWS.min(len)); s
    }
    fn move_up(&mut self) { let len = self.items.len(); self.state.move_up_wrap(len); self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len)); }
    fn move_down(&mut self) { let len = self.items.len(); self.state.move_down_wrap(len); self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len)); }
    fn accept(&mut self) { if let Some(idx)=self.state.selected_idx { if let Some(item)=self.items.get(idx){ for act in &item.actions { act(&self.app_event_tx);} self.complete=true; } } else { self.complete=true; } }
    fn cancel(&mut self) { self.complete = true; }
}

impl BottomPaneView for ListSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane, key_event: KeyEvent) {
        match key_event {
            KeyEvent{ code: KeyCode::Up, .. } => self.move_up(),
            KeyEvent{ code: KeyCode::Down, .. } => self.move_down(),
            KeyEvent{ code: KeyCode::Esc, .. } => self.cancel(),
            KeyEvent{ code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => self.accept(),
            _ => {}
        }
    }
    fn is_complete(&self) -> bool { self.complete }
    fn on_ctrl_c(&mut self, _pane: &mut BottomPane) -> CancellationEvent { self.complete = true; CancellationEvent::Handled }
    fn desired_height(&self, _width: u16) -> u16 { let rows = (self.items.len()).clamp(1, MAX_POPUP_ROWS); let mut h = rows as u16 + 1; if self.subtitle.is_some(){ h = h.saturating_add(2);} if self.footer_hint.is_some(){ h = h.saturating_add(2);} h }
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height==0||area.width==0 { return; }
        let title_area = Rect { x: area.x, y: area.y, width: area.width, height: 1 };
        let title_spans: Vec<Span<'static>> = vec![ Self::dim_prefix_span(), Span::styled(self.title.clone(), Style::default().add_modifier(Modifier::BOLD)) ];
        Paragraph::new(Line::from(title_spans)).render(title_area, buf);
        let mut next_y = area.y.saturating_add(1);
        if let Some(sub) = &self.subtitle { let sub_area = Rect { x: area.x, y: next_y, width: area.width, height: 1 }; let sub_spans = vec![ Self::dim_prefix_span(), Span::styled(sub.clone(), Style::default().add_modifier(Modifier::DIM)) ]; Paragraph::new(Line::from(sub_spans)).render(sub_area, buf); let spacer = Rect { x: area.x, y: next_y.saturating_add(1), width: area.width, height: 1 }; Self::render_dim_prefix_line(spacer, buf); next_y = next_y.saturating_add(2); }
        let footer_reserved = if self.footer_hint.is_some() { 2 } else { 0 };
        let rows_area = Rect { x: area.x, y: next_y, width: area.width, height: area.height.saturating_sub(next_y.saturating_sub(area.y)).saturating_sub(footer_reserved) };
        let rows: Vec<GenericDisplayRow> = self.items.iter().enumerate().map(|(i,it)| { let is_sel = self.state.selected_idx==Some(i); let prefix = if is_sel {'>'} else {' '}; let name_with_marker = if it.is_current { format!("{} (current)", it.name) } else { it.name.clone() }; let display_name = format!("{} {}. {}", prefix, i+1, name_with_marker); GenericDisplayRow { name: display_name, match_indices: None, is_current: it.is_current, description: it.description.clone() } }).collect();
        if rows_area.height>0 { render_rows(rows_area, buf, &rows, &self.state, MAX_POPUP_ROWS, true); }
        if let Some(hint) = &self.footer_hint { let footer_area = Rect { x: area.x, y: area.y + area.height - 1, width: area.width, height: 1 }; Paragraph::new(Line::from(Span::styled(hint.clone(), Style::default().add_modifier(Modifier::DIM)))).render(footer_area, buf); }
    }
}

