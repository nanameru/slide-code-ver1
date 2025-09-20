use crate::widgets::banner::{banner_lines, MESSAGE_PREFIX as BANNER_PREFIX};
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

    /// メッセージを行に変換（ツール実行結果を含むCodex風フォーマット）
    fn build_lines(&self) -> Vec<Line<'_>> {
        let mut lines: Vec<Line> = Vec::with_capacity(self.messages.len() * 2);

        for (i, message) in self.messages.iter().enumerate() {
            let mut handled = false;
            if message.starts_with(BANNER_PREFIX) {
                lines.extend(banner_lines());
                handled = true;
            }

            if !handled {
                // ツール実行結果の特別処理
                if self.is_tool_execution_result(message) {
                    lines.extend(self.format_tool_execution(message));
                // メッセージの種別によってスタイルを変更
                } else if message.starts_with("You:") {
                    let content = message.strip_prefix("You:").unwrap_or(message).trim();
                    lines.push(Line::from(vec![
                        Span::styled(
                            "You",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
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
                        Span::styled(
                            "Assistant",
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(": "),
                        Span::raw(content),
                    ]));
                } else {
                    // Generic message
                    lines.push(Line::from(Span::raw(message.as_str())));
                }
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
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::DIM),
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
            lines.push(Line::from(Span::styled(
                "Welcome to Slide Code! Type your message below.",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            )));
        }

        lines
    }

    /// ツール実行結果かどうかを判定
    fn is_tool_execution_result(&self, message: &str) -> bool {
        message.contains("Updated Plan")
            || message.contains("Proposed Change")
            || message.contains("Change Approved")
            || message.contains("Explored")
            || message.contains("[Tool Execution]")
            || message.contains("[Tool Execution Result]")
            || message.contains("*** Begin Patch")
            || message.contains("*** End Patch")
    }

    /// ツール実行結果をフォーマット
    fn format_tool_execution(&self, message: &str) -> Vec<Line<'_>> {
        let mut lines = Vec::new();
        let content_lines: Vec<&str> = message.lines().collect();

        for line in content_lines {
            let trimmed = line.trim();

            // セクションヘッダーの判定とスタイリング
            if trimmed.starts_with("Updated Plan") {
                lines.push(Line::from(vec![Span::styled(
                    "Updated Plan",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                )]));
            } else if trimmed.starts_with("Proposed Change") {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Proposed Change",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        trimmed["Proposed Change".len()..].to_string(),
                        Style::default().fg(Color::White),
                    ),
                ]));
            } else if trimmed.starts_with("Change Approved") {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Change Approved",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        trimmed["Change Approved".len()..].to_string(),
                        Style::default().fg(Color::White),
                    ),
                ]));
            } else if trimmed.starts_with("Explored") {
                lines.push(Line::from(vec![Span::styled(
                    "Explored",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )]));
            } else if trimmed.starts_with("[Tool Execution]") {
                lines.push(Line::from(vec![Span::styled(
                    "[Tool Execution]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )]));
            } else if trimmed.starts_with("[Tool Execution Result]") {
                lines.push(Line::from(vec![Span::styled(
                    "[Tool Execution Result]",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                )]));
            } else if trimmed.starts_with("▶") {
                lines.push(Line::from(vec![Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Yellow),
                )]));
            // 差分表示の色分け
            } else if trimmed.starts_with("+") {
                lines.push(Line::from(vec![Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Green),
                )]));
            } else if trimmed.starts_with("-") {
                lines.push(Line::from(vec![Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Red),
                )]));
            } else if trimmed.starts_with("@@") {
                lines.push(Line::from(vec![Span::styled(
                    line.to_string(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )]));
            // チェックボックス付きタスクリスト
            } else if trimmed.starts_with("□") || trimmed.starts_with("☑") {
                let (checkbox, rest) = if trimmed.starts_with("□") {
                    ("□", &trimmed[3..])
                } else {
                    ("☑", &trimmed[3..])
                };

                let checkbox_color = if checkbox == "☑" {
                    Color::Green
                } else {
                    Color::Gray
                };

                lines.push(Line::from(vec![
                    Span::raw("  "), // インデント
                    Span::styled(checkbox, Style::default().fg(checkbox_color)),
                    Span::raw(" "),
                    Span::raw(rest.to_string()),
                ]));
            // ファイルパスのハイライト
            } else if trimmed.contains(".rs")
                || trimmed.contains(".toml")
                || trimmed.contains(".md")
            {
                lines.push(Line::from(vec![Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::LightBlue),
                )]));
            // その他の行
            } else {
                lines.push(Line::from(Span::raw(line.to_string())));
            }
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
