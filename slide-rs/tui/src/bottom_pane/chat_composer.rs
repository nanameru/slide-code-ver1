use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{buffer::Buffer, layout::{Constraint, Layout, Rect}, prelude::Stylize, style::{Color, Modifier, Style}, text::Line, widgets::{Block, BorderType, Borders, StatefulWidgetRef, WidgetRef}};

use super::{chat_composer_history::ChatComposerHistory, textarea::{TextArea, TextAreaState}};

/// 入力結果
#[derive(Debug, PartialEq, Clone)]
pub enum InputResult { Submitted(String), None }

/// チャット入力コンポーネント（簡易版）
pub struct ChatComposer {
    pub textarea: TextArea,
    textarea_state: TextAreaState,
    history: ChatComposerHistory,
    has_focus: bool,
    placeholder_text: String,
}

impl ChatComposer {
    pub fn new_minimal(has_input_focus: bool, placeholder_text: String) -> Self {
        Self {
            textarea: TextArea::new(),
            textarea_state: TextAreaState::default(),
            history: ChatComposerHistory::new(),
            has_focus: has_input_focus,
            placeholder_text,
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        // テキストエリア + 1 行のヒント
        self.textarea.desired_height(width.saturating_sub(1)).saturating_add(1)
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        match key_event {
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, .. } => {
                let mut text = self.textarea.text().to_string();
                self.textarea.set_text("");
                text = text.trim().to_string();
                if !text.is_empty() { self.history.record_local_submission(&text); }
                (InputResult::Submitted(text), true)
            }
            KeyEvent { code: KeyCode::Up | KeyCode::Down, .. } => {
                let text = self.textarea.text();
                let cur = self.textarea.cursor();
                if self.history.should_handle_navigation(text, cur) {
                    let next = if matches!(key_event.code, KeyCode::Up) { self.history.navigate_up() } else { self.history.navigate_down() };
                    if let Some(t) = next { self.textarea.set_text(&t); self.textarea.set_cursor(0); return (InputResult::None, true); }
                }
                self.textarea.input(key_event);
                (InputResult::None, true)
            }
            other => { self.textarea.input(other); (InputResult::None, true) }
        }
    }
}

impl WidgetRef for ChatComposer {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let [textarea_rect, footer_rect] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        // 左側縦ボーダー
        let border_style = if self.has_focus { Style::default().fg(Color::Cyan) } else { Style::default().add_modifier(Modifier::DIM) };
        Block::default().borders(Borders::LEFT).border_type(BorderType::QuadrantOutside).border_style(border_style)
            .render_ref(Rect::new(textarea_rect.x, textarea_rect.y, 1, textarea_rect.height), buf);
        let mut ta_rect = textarea_rect; ta_rect.width = ta_rect.width.saturating_sub(1); ta_rect.x += 1;
        let mut state = self.textarea_state;
        StatefulWidgetRef::render_ref(&(&self.textarea), ta_rect, buf, &mut state);
        // プレースホルダ
        if self.textarea.text().is_empty() {
            Line::from(self.placeholder_text.as_str()).style(Style::default().dim())
                .render_ref(ta_rect, buf);
        }
        // フッタヒント
        Line::from(" Enter: send  |  Ctrl+J: newline ").style(Style::default().dim())
            .render_ref(footer_rect, buf);
    }
}

