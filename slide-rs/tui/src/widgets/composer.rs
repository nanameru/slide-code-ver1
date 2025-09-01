use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct ComposerWidget<'a> {
    input: &'a str,
    active: bool,
}

impl<'a> ComposerWidget<'a> {
    pub fn new(input: &'a str, active: bool) -> Self {
        Self { input, active }
    }
}

impl<'a> ratatui::widgets::Widget for ComposerWidget<'a> {
    fn render(self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let mut lines: Vec<Line> = Vec::new();
        let placeholder = if self.active { "Type your message…" } else { "Press i to compose" };
        let content = if self.input.is_empty() {
            Line::from(Span::styled(
                placeholder,
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ))
        } else {
            Line::from(self.input.to_string())
        };
        lines.push(content);
        let text = Text::from(lines);
        let title = if self.active { "Composer (INSERT)" } else { "Composer" };
        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false });
        widget.render(area, buf);
    }
}

