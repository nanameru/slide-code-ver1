use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, WidgetRef, Wrap},
};

pub struct PagerOverlay {
    active: bool,
    scroll_top: usize,
    lines: Vec<Line<'static>>,
    title: Option<String>,
    last_page_h: usize,
}

impl PagerOverlay {
    pub fn new() -> Self {
        Self {
            active: false,
            scroll_top: 0,
            lines: Vec::new(),
            title: None,
            last_page_h: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.scroll_top = 0;
        self.lines.clear();
    }

    pub fn set_lines(&mut self, lines: Vec<Line<'static>>) {
        self.lines = lines;
        self.active = true;
        self.scroll_top = 0;
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    /// Returns true if the key was consumed by the overlay
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.active || key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Char('q') => {
                self.clear();
                true
            }
            KeyCode::Esc => {
                self.clear();
                true
            }
            KeyCode::Up => {
                self.scroll_top = self.scroll_top.saturating_sub(1);
                true
            }
            KeyCode::Down => {
                self.scroll_top = self.scroll_top.saturating_add(1);
                true
            }
            KeyCode::PageUp => {
                let page = self.last_page_h.max(1);
                self.scroll_top = self.scroll_top.saturating_sub(page);
                true
            }
            KeyCode::PageDown => {
                let page = self.last_page_h.max(1);
                self.scroll_top = self.scroll_top.saturating_add(page);
                true
            }
            KeyCode::Home => {
                self.scroll_top = 0;
                true
            }
            KeyCode::End => {
                self.scroll_top = usize::MAX;
                true
            }
            _ => false,
        }
    }

    pub fn render_ref(&mut self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let header_h = 2u16; // key hints + optional title
        let footer_h = 1u16; // progress bar
        let content_h = area.height.saturating_sub(header_h).saturating_sub(footer_h);

        // Header
        let header_area = Rect::new(area.x, area.y, area.width, header_h);
        self.render_header(header_area, buf);

        // Content (paged)
        if content_h > 0 {
            let content_area = Rect::new(area.x, area.y + header_h, area.width, content_h);
            let height = content_area.height as usize;
            self.last_page_h = height;
            // Render entire content and let ratatui handle wrapping; use scroll for offset
            Paragraph::new(self.lines.clone())
                .wrap(Wrap { trim: false })
                .scroll((self.scroll_top as u16, 0))
                .render_ref(content_area, buf);

            // Footer with percentage
            let footer_area = Rect::new(area.x, content_area.y + content_area.height, area.width, footer_h);
            let total_est = self.estimate_total_wrapped_lines(content_area.width);
            self.render_footer(footer_area, buf, total_est, height);
        }
    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 { return; }
        // Line 1: key hints
        let line1 = Rect::new(area.x, area.y, area.width, 1);
        let cyan = Style::default().fg(Color::Cyan);
        let mut spans: Vec<Span<'static>> = vec![" ".into()];
        let pairs = [("↑/↓", "scroll"), ("PgUp/PgDn", "page"), ("Home/End", "jump"), ("q/Esc", "quit")];
        let mut first = true;
        for (k, d) in pairs {
            if !first { spans.push("   ".into()); }
            spans.push(Span::from(k.to_string()).style(cyan));
            spans.push(" ".into());
            spans.push(Span::from(d.to_string()));
            first = false;
        }
        Paragraph::new(vec![Line::from(spans).style(Style::default().fg(Color::Gray))])
            .render_ref(line1, buf);

        // Line 2: optional title
        if area.height > 1 {
            let line2 = Rect::new(area.x, area.y + 1, area.width, 1);
            let title = self.title.as_deref().unwrap_or("");
            if !title.is_empty() {
                Paragraph::new(vec![Line::from(title).style(Style::default().fg(Color::White))])
                    .render_ref(line2, buf);
            }
        }
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer, total: usize, page_h: usize) {
        if area.height == 0 { return; }
        // Simple percent based on unwrapped lines count (approx)
        let max_scroll = total.saturating_sub(page_h);
        let pct = if max_scroll == 0 { 100 } else { ((self.scroll_top.min(max_scroll) as f32 / max_scroll as f32) * 100.0).round() as u8 };
        let label = format!(" {pct}% ");
        let w = label.chars().count() as u16;
        let x = area.x + area.width.saturating_sub(w).saturating_sub(1);
        Paragraph::new(vec![Line::from(label).style(Style::default().fg(Color::Gray))])
            .render_ref(Rect::new(x, area.y, w, 1), buf);
        // underline separator
        let sep_w = area.width.saturating_sub(w + 2);
        if sep_w > 0 {
            let sep = "─".repeat(sep_w as usize);
            Paragraph::new(vec![Line::from(sep).style(Style::default().fg(Color::Gray))])
                .render_ref(Rect::new(area.x, area.y, sep_w, 1), buf);
        }
    }

    fn estimate_total_wrapped_lines(&self, width: u16) -> usize {
        let w = width.max(1) as usize;
        if w == 0 { return self.lines.len(); }
        let mut total = 0usize;
        for line in &self.lines {
            // Best-effort: approximate wrapped rows by visual width
            let lw = line.width();
            let rows = (lw + w - 1) / w; // ceil
            total = total.saturating_add(rows.max(1));
        }
        total
    }
}


