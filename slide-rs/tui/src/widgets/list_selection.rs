use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub struct ListSelection<'a> {
    pub title: &'a str,
    pub filter: &'a str,
    pub items: &'a [String],
    pub selected: usize,
    pub hint: &'a str,
}

impl<'a> ListSelection<'a> {
    pub fn new(title: &'a str, filter: &'a str, items: &'a [String], selected: usize, hint: &'a str) -> Self {
        Self { title, filter, items, selected, hint }
    }

    pub fn render(self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);
        let outer = Block::default().borders(Borders::ALL).title(self.title);
        let inner = outer.inner(area);
        f.render_widget(outer, area); // draw borders first

        // Layout: filter | list | hint
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);

        // Filter line
        let filter_line = Line::from(vec![
            Span::styled(
                "Filter: ",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(self.filter),
        ]);
        let filter = Paragraph::new(filter_line)
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        f.render_widget(filter, chunks[0]);

        // Items
        let mut lines: Vec<Line> = Vec::new();
        for (idx, item) in self.items.iter().enumerate() {
            let selected = idx == self.selected;
            let style = if selected {
                Style::default().fg(Color::Black).bg(Color::LightYellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(item.clone(), style)));
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "No results",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            )));
        }
        let list = Paragraph::new(lines).block(Block::default().borders(Borders::ALL));
        f.render_widget(list, chunks[1]);

        // Hint
        let hint = Paragraph::new(self.hint)
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::NONE));
        f.render_widget(hint, chunks[2]);
    }
}
