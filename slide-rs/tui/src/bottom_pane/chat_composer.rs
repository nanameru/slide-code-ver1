use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, StatefulWidgetRef, WidgetRef, Wrap},
};
use std::cell::RefCell;
use std::time::{Duration, Instant};

use super::{
    chat_composer_history::ChatComposerHistory,
    textarea::{TextArea, TextAreaState},
};

/// 入力結果
#[derive(Debug, PartialEq, Clone)]
pub enum InputResult {
    Submitted(String),
    None,
}

/// チャット入力コンポーネント（Codex風高機能版）
pub struct ChatComposer {
    textarea: TextArea,
    textarea_state: RefCell<TextAreaState>,
    history: ChatComposerHistory,
    has_focus: bool,
    placeholder_text: String,
    ctrl_c_quit_hint: bool,
    esc_backtrack_hint: bool,
    use_shift_enter_hint: bool,
    last_activity: Instant,
    show_hints: bool,
}

impl ChatComposer {
    pub fn new_minimal(has_input_focus: bool, placeholder_text: String) -> Self {
        Self {
            textarea: TextArea::new(),
            textarea_state: RefCell::new(TextAreaState::default()),
            history: ChatComposerHistory::new(),
            has_focus: has_input_focus,
            placeholder_text,
            ctrl_c_quit_hint: false,
            esc_backtrack_hint: false,
            use_shift_enter_hint: true,
            last_activity: Instant::now(),
            show_hints: true,
        }
    }

    pub fn new(
        has_input_focus: bool,
        placeholder_text: String,
        enhanced_keys_supported: bool,
    ) -> Self {
        let mut composer = Self::new_minimal(has_input_focus, placeholder_text);
        composer.use_shift_enter_hint = enhanced_keys_supported;
        composer
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        let textarea_height = self.textarea.desired_height(width.saturating_sub(1));
        let hints_height = if self.show_hints { 1 } else { 0 };
        textarea_height.saturating_add(hints_height)
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        if key_event.kind != KeyEventKind::Press {
            return (InputResult::None, false);
        }

        self.last_activity = Instant::now();
        self.clear_hints();

        match key_event {
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let text = self.textarea.text().trim().to_string();
                if !text.is_empty() {
                    self.history.record_local_submission(&text);
                    self.textarea.set_text("");
                    (InputResult::Submitted(text), true)
                } else {
                    (InputResult::None, false)
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::SHIFT,
                ..
            } if self.use_shift_enter_hint => {
                self.textarea.insert_str("\n");
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Char('j' | 'm'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.textarea.insert_str("\n");
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Up | KeyCode::Down,
                ..
            } => {
                let text = self.textarea.text();
                let cursor = self.textarea.cursor();
                if self.history.should_handle_navigation(text, cursor) {
                    let next = if matches!(key_event.code, KeyCode::Up) {
                        self.history.navigate_up()
                    } else {
                        self.history.navigate_down()
                    };
                    if let Some(t) = next {
                        self.textarea.set_text(&t);
                        self.textarea.set_cursor(t.len());
                        return (InputResult::None, true);
                    }
                }
                self.textarea.input(key_event);
                (InputResult::None, true)
            }
            other => {
                self.textarea.input(other);
                (InputResult::None, true)
            }
        }
    }

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        if !self.has_focus {
            return None;
        }

        let [textarea_rect, _] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(if self.show_hints { 1 } else { 0 }),
        ])
        .areas(area);

        let content_area = Rect {
            x: textarea_rect.x + 2, // Account for double left border
            y: textarea_rect.y,
            width: textarea_rect.width.saturating_sub(2),
            height: textarea_rect.height,
        };

        let state = self.textarea_state.borrow();
        self.textarea.cursor_pos_with_state(content_area, &*state)
    }

    pub fn text(&self) -> &str {
        self.textarea.text()
    }

    pub fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }

    pub fn composer_is_empty(&self) -> bool {
        self.textarea.is_empty()
    }

    pub fn insert_str(&mut self, text: &str) {
        self.textarea.insert_str(text);
    }

    pub fn set_focus(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
    }

    pub fn set_text(&mut self, text: &str) {
        self.textarea.set_text(text);
    }

    pub fn clear(&mut self) {
        self.textarea.set_text("");
    }

    pub fn show_ctrl_c_quit_hint(&mut self) {
        self.ctrl_c_quit_hint = true;
    }

    pub fn clear_ctrl_c_quit_hint(&mut self) {
        self.ctrl_c_quit_hint = false;
    }

    pub fn show_esc_backtrack_hint(&mut self) {
        self.esc_backtrack_hint = true;
    }

    pub fn clear_esc_backtrack_hint(&mut self) {
        self.esc_backtrack_hint = false;
    }

    fn clear_hints(&mut self) {
        self.ctrl_c_quit_hint = false;
        self.esc_backtrack_hint = false;
    }

    fn should_show_inactive_hints(&self) -> bool {
        self.last_activity.elapsed() > Duration::from_secs(3)
    }

    fn render_hints(&self, area: Rect, buf: &mut Buffer) {
        let mut hints = Vec::new();

        if self.ctrl_c_quit_hint {
            hints.push(("Ctrl+C", "quit"));
        } else if self.esc_backtrack_hint {
            hints.push(("Esc", "back"));
        } else if self.should_show_inactive_hints() {
            if self.use_shift_enter_hint {
                hints.push(("Enter", "send"));
                hints.push(("Shift+Enter", "newline"));
            } else {
                hints.push(("Enter", "send"));
                hints.push(("Ctrl+J/M", "newline"));
            }
            hints.push(("↑/↓", "history"));
        }

        if hints.is_empty() {
            return;
        }

        let mut spans = vec![Span::raw(" ")];
        for (i, (key, desc)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(*key, Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                *desc,
                Style::default().add_modifier(Modifier::DIM),
            ));
        }

        let hint_line = Line::from(spans);
        let paragraph = Paragraph::new(vec![hint_line])
            .wrap(Wrap { trim: false })
            .style(Style::default().add_modifier(Modifier::DIM));
        paragraph.render_ref(area, buf);
    }
}

impl WidgetRef for &ChatComposer {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let [textarea_rect, hint_rect] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(if self.show_hints { 1 } else { 0 }),
        ])
        .areas(area);

        // Left border: always light green regardless of focus
        // Using RGB for a soft light‑green tone.
        let border_style = Style::default().fg(Color::Rgb(144, 238, 144));

        Block::default()
            .borders(Borders::LEFT)
            .border_type(BorderType::Plain)
            .border_style(border_style)
            .render_ref(
                Rect::new(textarea_rect.x, textarea_rect.y, 1, textarea_rect.height),
                buf,
            );

        // Second column to make the border appear 2x thicker
        Block::default()
            .borders(Borders::LEFT)
            .border_type(BorderType::Plain)
            .border_style(border_style)
            .render_ref(
                Rect::new(textarea_rect.x + 1, textarea_rect.y, 1, textarea_rect.height),
                buf,
            );

        // Content area (excluding left border)
        let content_area = Rect {
            x: textarea_rect.x + 2,
            y: textarea_rect.y,
            width: textarea_rect.width.saturating_sub(2),
            height: textarea_rect.height,
        };

        if self.textarea.is_empty() && !self.placeholder_text.is_empty() {
            // Show placeholder
            let placeholder_line = Line::from(Span::styled(
                &self.placeholder_text,
                Style::default().add_modifier(Modifier::DIM),
            ));
            let placeholder_paragraph = Paragraph::new(vec![placeholder_line]);
            placeholder_paragraph.render_ref(content_area, buf);
        } else {
            // Render textarea with state
            let mut state = self.textarea_state.borrow_mut();
            StatefulWidgetRef::render_ref(&&self.textarea, content_area, buf, &mut *state);
        }

        // Render hints if enabled
        if self.show_hints && hint_rect.height > 0 {
            self.render_hints(hint_rect, buf);
        }
    }
}
