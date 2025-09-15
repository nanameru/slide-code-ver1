use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct ChatWidget<'a> {
    messages: &'a [String],
    scroll_top: usize,
    viewport_height: usize,
}

impl<'a> ChatWidget<'a> {
    pub fn new(messages: &'a [String]) -> Self {
        Self { messages, scroll_top: 0, viewport_height: 0 }
    }

    pub fn with_scroll(mut self, scroll_top: usize, viewport_height: usize) -> Self {
        self.scroll_top = scroll_top;
        self.viewport_height = viewport_height;
        self
    }
}

impl<'a> ratatui::widgets::Widget for ChatWidget<'a> {
    fn render(self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let mut lines: Vec<Line> = Vec::with_capacity(self.messages.len() * 2);
        for (i, m) in self.messages.iter().enumerate() {
            let prefix = if m.starts_with("You:") {
                Span::styled("You", Style::default().fg(Color::Yellow))
            } else {
                Span::styled("Msg", Style::default().fg(Color::Green))
            };
            lines.push(Line::from(vec![prefix, Span::raw(format!(": {}", m.trim_start_matches("You: ")))]));
            if i + 1 < self.messages.len() {
                lines.push(Line::from(""));
            }
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "No messages yet.",
                Style::default().fg(Color::DarkGray),
            )));
        }
        // Compute slice based on scroll_top and viewport_height (excluding borders handled by caller)
        let total = lines.len();
        let vp = if self.viewport_height == 0 { area.height.saturating_sub(2) as usize } else { self.viewport_height };
        let max_top = total.saturating_sub(vp);
        let top = self.scroll_top.min(max_top);
        let bottom = (top + vp).min(total);
        let view = &lines[top..bottom];
        let text = Text::from(view.to_vec());
        // Codex風: 枠線なし・タイトル無し
        let widget = Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);
        widget.render(area, buf);
    }
}

