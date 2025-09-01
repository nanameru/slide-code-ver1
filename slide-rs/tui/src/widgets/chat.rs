use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct ChatWidget<'a> {
    messages: &'a [String],
}

impl<'a> ChatWidget<'a> {
    pub fn new(messages: &'a [String]) -> Self {
        Self { messages }
    }
}

impl<'a> ratatui::widgets::Widget for ChatWidget<'a> {
    fn render(self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let mut lines: Vec<Line> = Vec::with_capacity(self.messages.len());
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
        let text = Text::from(lines);
        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("Chat"))
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);
        widget.render(area, buf);
    }
}

