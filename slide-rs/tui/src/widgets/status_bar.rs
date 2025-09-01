use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub struct StatusBar<'a> {
    mode: &'a str,
    status: &'a str,
    hints: &'a str,
}

impl<'a> StatusBar<'a> {
    pub fn new(mode: &'a str, status: &'a str, hints: &'a str) -> Self {
        Self { mode, status, hints }
    }
}

impl<'a> ratatui::widgets::Widget for StatusBar<'a> {
    fn render(self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let line = Line::from(vec![
            Span::styled(
                format!(" {} ", self.mode),
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{}", self.status),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  |  "),
            Span::styled(self.hints, Style::default().fg(Color::Gray)),
        ]);
        let widget = Paragraph::new(line)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Left);
        widget.render(area, buf);
    }
}

