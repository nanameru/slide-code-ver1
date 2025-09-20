use ratatui::{
    style::Style,
    text::Text,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct Modal<'a> {
    title: &'a str,
    body: &'a str,
}

impl<'a> Modal<'a> {
    pub fn new(title: &'a str, body: &'a str) -> Self {
        Self { title, body }
    }
}

impl<'a> ratatui::widgets::Widget for Modal<'a> {
    fn render(self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let widget = Paragraph::new(Text::from(self.body.to_string()))
            .style(Style::default())
            .block(Block::default().borders(Borders::ALL).title(self.title))
            .wrap(Wrap { trim: true });
        widget.render(area, buf);
    }
}
