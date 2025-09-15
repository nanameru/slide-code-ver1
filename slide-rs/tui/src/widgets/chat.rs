use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct ChatWidget<'a> {
    messages: &'a [String],
    scroll_top: usize,
    viewport_height: usize,
    prompt_input: Option<&'a str>,
}

impl<'a> ChatWidget<'a> {
    pub fn new(messages: &'a [String]) -> Self {
        Self { messages, scroll_top: 0, viewport_height: 0, prompt_input: None }
    }

    pub fn with_scroll(mut self, scroll_top: usize, viewport_height: usize) -> Self {
        self.scroll_top = scroll_top;
        self.viewport_height = viewport_height;
        self
    }

    pub fn with_prompt(mut self, prompt: &'a str) -> Self {
        self.prompt_input = Some(prompt);
        self
    }
}

impl<'a> ratatui::widgets::Widget for ChatWidget<'a> {
    fn render(self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let mut lines: Vec<Line> = Vec::with_capacity(self.messages.len() * 2 + 1);
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
        // Append prompt input as the last line (terminal-like)
        if let Some(p) = self.prompt_input {
            if p.is_empty() {
                // Cyan prompt with placeholder when empty
                lines.push(Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(Color::Cyan)),
                    Span::styled("Ask Slide Code to do anything", Style::default().fg(Color::Cyan)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(Color::Cyan)),
                    Span::raw(p.to_string()),
                ]));
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
        let vp = if self.viewport_height == 0 { area.height as usize } else { self.viewport_height };
        let max_top = total.saturating_sub(vp);
        let top = self.scroll_top.min(max_top);
        let bottom = (top + vp).min(total);
        // 右端1列をスクロールバーに充てる
        let content_width = area.width.saturating_sub(1);
        let content_area = Rect { x: area.x, y: area.y, width: content_width, height: area.height };
        let view = &lines[top..bottom];
        let text = Text::from(view.to_vec());
        // Codex風: 枠線なし・タイトル無し
        let widget = Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);
        widget.render(content_area, buf);

        // 簡易スクロールバーを右端に描画（トラック＋サム）
        let bar_x = area.x.saturating_add(area.width.saturating_sub(1));
        let bar_area = Rect { x: bar_x, y: area.y, width: 1, height: area.height };
        if total > vp && bar_area.width > 0 && bar_area.height > 0 {
            // トラック
            for row in 0..bar_area.height {
                let y = bar_area.y + row;
                buf.set_string(bar_area.x, y, "│", Style::default().fg(Color::DarkGray));
            }
            // サム（最低高さ1）: 比率に基づく
            let vp_u = vp as u16;
            let total_u = total as u16;
            let h = bar_area.height.max(1);
            // hが2未満だとトラックとサムの区別が難しいのでそのまま1描画
            let thumb_h = ((h as u32 * vp_u as u32) / total_u as u32).max(1) as u16;
            let max_thumb_top = h.saturating_sub(thumb_h);
            let thumb_top = if max_top == 0 { 0 } else { ((h as u32 - thumb_h as u32) * top as u32 / max_top as u32) as u16 };
            let thumb_top = thumb_top.min(max_thumb_top);
            for row in 0..thumb_h {
                let y = bar_area.y + thumb_top + row;
                buf.set_string(bar_area.x, y, "█", Style::default().fg(Color::Cyan));
            }
        }
    }
}

