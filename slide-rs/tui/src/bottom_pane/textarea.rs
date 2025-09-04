use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::{StatefulWidgetRef, WidgetRef}};
use std::{cell::RefCell, ops::Range};
use unicode_width::UnicodeWidthStr;
// use ratatui::widgets::WidgetRef; // already imported above via super modules

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TextAreaState { pub scroll: u16 }

#[derive(Debug)]
pub(crate) struct TextArea {
    text: String,
    cursor: usize,
    wrap_cache: RefCell<Option<(u16, Vec<Range<usize>>)>>,
}

impl TextArea {
    pub fn new() -> Self { Self { text: String::new(), cursor: 0, wrap_cache: RefCell::new(None) } }
    pub fn set_text(&mut self, s: &str) { self.text = s.to_string(); self.cursor = self.cursor.min(self.text.len()); self.wrap_cache.replace(None); }
    pub fn text(&self) -> &str { &self.text }
    pub fn is_empty(&self) -> bool { self.text.is_empty() }
    pub fn cursor(&self) -> usize { self.cursor }
    pub fn set_cursor(&mut self, pos: usize) { self.cursor = pos.min(self.text.len()); }
    pub fn insert_str(&mut self, s: &str) { self.text.insert_str(self.cursor, s); self.cursor += s.len(); self.wrap_cache.replace(None); }
    pub fn replace_range(&mut self, r: Range<usize>, s: &str) { let start = r.start.min(self.text.len()); let end = r.end.min(self.text.len()); self.text.replace_range(start..end, s); self.cursor = start + s.len(); self.wrap_cache.replace(None); }
    pub fn desired_height(&self, width: u16) -> u16 { self.wrapped_lines(width).len().max(1) as u16 }

    pub fn input(&mut self, ev: KeyEvent) {
        match ev {
            KeyEvent { code: KeyCode::Char(c), modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT, .. } => self.insert_str(&c.to_string()),
            KeyEvent { code: KeyCode::Enter, .. } => self.insert_str("\n"),
            KeyEvent { code: KeyCode::Backspace, .. } => {
                if self.cursor > 0 { let prev = self.cursor - 1; self.replace_range(prev..self.cursor, ""); }
            }
            KeyEvent { code: KeyCode::Left, .. } => { if self.cursor > 0 { self.cursor -= 1; } },
            KeyEvent { code: KeyCode::Right, .. } => { if self.cursor < self.text.len() { self.cursor += 1; } },
            KeyEvent { code: KeyCode::Home, .. } => { self.cursor = 0; },
            KeyEvent { code: KeyCode::End, .. } => { self.cursor = self.text.len(); },
            _ => {}
        }
    }

    pub fn cursor_pos_with_state(&self, area: Rect, state: &TextAreaState) -> Option<(u16,u16)> {
        let lines = self.wrapped_lines(area.width);
        let mut row = 0usize;
        for (i, r) in lines.iter().enumerate() { if self.cursor >= r.start && self.cursor <= r.end { row = i; break; } }
        let col = self.text[ranges_start(&lines, row)..self.cursor].width() as u16;
        let y = area.y + row.saturating_sub(state.scroll as usize) as u16;
        Some((area.x + col, y))
    }

    fn wrapped_lines(&self, width: u16) -> Vec<Range<usize>> {
        if width == 0 { return vec![0..self.text.len()]; }
        if let Some((w, lines)) = self.wrap_cache.borrow().as_ref() { if *w == width { return lines.clone(); } }
        let mut lines = Vec::new();
        let mut start = 0usize; let mut cur = 0usize; let mut curw = 0usize;
        for (i, ch) in self.text.char_indices() {
            if ch == '\n' { lines.push(start..i+1); start = i+1; cur = i+1; curw = 0; continue; }
            let w = ch.to_string().width();
            if curw + w > width as usize { lines.push(start..cur); start = cur; curw = 0; }
            cur = i + ch.len_utf8(); curw += w;
        }
        lines.push(start..self.text.len());
        self.wrap_cache.replace(Some((width, lines.clone())));
        lines
    }

    fn render_lines(&self, area: Rect, buf: &mut Buffer, lines: &[Range<usize>], range: std::ops::Range<usize>) {
        for (row, idx) in range.enumerate() {
            let r = &lines[idx];
            let y = area.y + row as u16;
            let slice = &self.text[r.start..r.end];
            buf.set_string(area.x, y, slice, Style::default());
        }
    }
}

impl WidgetRef for &TextArea {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.wrapped_lines(area.width);
        self.render_lines(area, buf, &lines, 0..lines.len());
    }
}

impl StatefulWidgetRef for &TextArea {
    type State = TextAreaState;
    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let lines = self.wrapped_lines(area.width);
        let start = state.scroll as usize;
        let end = (start + area.height as usize).min(lines.len());
        self.render_lines(area, buf, &lines, start..end);
    }
}

fn ranges_start(lines: &[Range<usize>], row: usize) -> usize { lines.get(row).map(|r| r.start).unwrap_or(0) }

