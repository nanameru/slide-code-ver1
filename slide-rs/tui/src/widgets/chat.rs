use crate::history_cell::HistoryCell;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};

/// Codex風のチャットウィジェット。履歴セルを組み立てて描画する。
pub struct ChatWidget<'a> {
    history: &'a [HistoryCell],
    prompt_input: Option<&'a str>,
    scroll_top: usize,
}

impl<'a> ChatWidget<'a> {
    pub fn new(history: &'a [HistoryCell]) -> Self {
        Self {
            history,
            prompt_input: None,
            scroll_top: 0,
        }
    }

    pub fn with_scroll(mut self, scroll_top: usize, _viewport_height: usize) -> Self {
        self.scroll_top = scroll_top;
        self
    }

    pub fn with_prompt(mut self, prompt: &'a str) -> Self {
        self.prompt_input = Some(prompt);
        self
    }

    fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();

        for cell in self.history {
            let mut cell_lines = cell.lines();
            if lines.is_empty() {
                if let Some(first) = cell_lines.first() {
                    let is_empty = first
                        .spans
                        .iter()
                        .all(|span| span.content.trim().is_empty());
                    if is_empty {
                        cell_lines.remove(0);
                    }
                }
            }
            lines.extend(cell_lines);
        }

        if let Some(prompt) = self.prompt_input {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }

            if prompt.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        "Ask Slide Code to do anything",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::DIM),
                    ),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(Color::Cyan)),
                    Span::raw(prompt.to_string()),
                ]));
            }
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "Welcome to Slide Code! Type your message below.",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            )));
        }

        lines
    }
}

impl<'a> ratatui::widgets::Widget for ChatWidget<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let lines = self.build_lines();
        let text = Text::from(lines);

        let scroll = self.scroll_top.min(u16::MAX as usize) as u16;

        let paragraph = Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left)
            .scroll((scroll, 0));

        paragraph.render(area, buf);
    }
}
