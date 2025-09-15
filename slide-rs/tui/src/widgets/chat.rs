use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Codex風のシンプルなチャットウィジェット
/// カスタムスクロールバーを削除し、内部的なスクロール管理のみ
pub struct ChatWidget<'a> {
    messages: &'a [String],
    prompt_input: Option<&'a str>,
}

impl<'a> ChatWidget<'a> {
    pub fn new(messages: &'a [String]) -> Self {
        Self {
            messages,
            prompt_input: None,
        }
    }

    pub fn with_scroll(self, _scroll_top: usize, _viewport_height: usize) -> Self {
        // スクロール管理はターミナルに委ねるため、パラメータは無視
        self
    }

    pub fn with_prompt(mut self, prompt: &'a str) -> Self {
        self.prompt_input = Some(prompt);
        self
    }

    /// メッセージを行に変換（Codex風のシンプルなフォーマット）
    fn build_lines(&self) -> Vec<Line> {
        let mut lines: Vec<Line> = Vec::with_capacity(self.messages.len() * 2);

        for (i, message) in self.messages.iter().enumerate() {
            // メッセージの種別によってスタイルを変更
            if message.starts_with("You:") {
                let content = message.strip_prefix("You:").unwrap_or(message).trim();
                lines.push(Line::from(vec![
                    Span::styled("You", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(": "),
                    Span::raw(content),
                ]));
            } else if message.starts_with("Assistant:") || message.starts_with("AI:") {
                let content = message
                    .strip_prefix("Assistant:")
                    .or_else(|| message.strip_prefix("AI:"))
                    .unwrap_or(message)
                    .trim();
                lines.push(Line::from(vec![
                    Span::styled("Assistant", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw(": "),
                    Span::raw(content),
                ]));
            } else {
                // Generic message
                lines.push(Line::from(Span::raw(message.as_str())));
            }

            // Add spacing between messages (except for the last one)
            if i + 1 < self.messages.len() {
                lines.push(Line::from(""));
            }
        }

        // Add current prompt input if present
        if let Some(prompt) = self.prompt_input {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }

            if prompt.is_empty() {
                // Show placeholder when input is empty
                lines.push(Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        "Ask Slide Code to do anything",
                        Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
                    ),
                ]));
            } else {
                // Show actual input
                lines.push(Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(Color::Cyan)),
                    Span::raw(prompt),
                ]));
            }
        }

        // Show welcome message when no content
        if lines.is_empty() {
            lines.push(Line::from(
                Span::styled(
                    "Welcome to Slide Code! Type your message below.",
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
                ),
            ));
        }

        lines
    }
}

impl<'a> ratatui::widgets::Widget for ChatWidget<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let lines = self.build_lines();
        let text = Text::from(lines);

        // シンプルなParagraphウィジェットで表示（枠線なし、Codex風）
        let paragraph = Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);

        paragraph.render(area, buf);
    }
}