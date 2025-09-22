use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Margin, Rect},
    style::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, StatefulWidgetRef, WidgetRef, Wrap},
};
use std::cell::RefCell;

use super::{
    chat_composer_history::ChatComposerHistory,
    textarea::{TextArea, TextAreaState},
};
// Clipboard paste is intercepted at BottomPane to enqueue images (codex-1 準拠)

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
        // Account for borders (2 lines) and icon/spacing (doesn't affect height)
        let inner_width = width.saturating_sub(4); // 2 for borders + 2 for icon and spacing
        let textarea_height = self.textarea.desired_height(inner_width);
        let hints_height = if self.show_hints { 1 } else { 0 };
        // Add 2 for top and bottom borders
        textarea_height.saturating_add(hints_height).saturating_add(2)
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        if key_event.kind != KeyEventKind::Press {
            return (InputResult::None, false);
        }

        self.clear_hints();

        match key_event {
            // Ctrl+V is handled at BottomPane level to enqueue image attachments.
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
                // Before mutating textarea, capture @search short pattern for immediate popup
                if let KeyEvent { code: KeyCode::Char(c), modifiers: KeyModifiers::NONE, .. } = other {
                    // Append char locally to inspect pattern without committing state first
                    let mut preview = self.textarea.text().to_string();
                    preview.push(c);
                    if extract_at_search_query(&preview).is_some() {
                        // Store back the key into textarea and ask caller to redraw
                        self.textarea.input(other);
                        return (InputResult::None, true);
                    }
                }
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

        // Use Block's inner method to get the correct content area
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        let content_area = block.inner(textarea_rect);

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

    pub fn set_show_hints(&mut self, show: bool) {
        self.show_hints = show;
    }

    pub fn set_placeholder_text(&mut self, text: String) {
        self.placeholder_text = text;
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

    fn render_hints(&self, area: Rect, buf: &mut Buffer) {
        let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];

        if self.ctrl_c_quit_hint {
            spans.push("Ctrl+C".cyan().bold());
            spans.push(Span::raw(" again to quit"));
        } else {
            spans.push("⏎".cyan());
            spans.push(Span::raw(" send"));
            spans.push(Span::raw("   "));

            if self.use_shift_enter_hint {
                spans.push("Shift+⏎".cyan());
            } else {
                spans.push("Ctrl+J".cyan());
            }
            spans.push(Span::raw(" newline"));
            spans.push(Span::raw("   "));

            spans.push("Ctrl+T".cyan());
            spans.push(Span::raw(" transcript"));
            spans.push(Span::raw("   "));

            spans.push("Ctrl+C".cyan());
            spans.push(Span::raw(" quit"));

            if self.esc_backtrack_hint {
                spans.push(Span::raw("   "));
                spans.push("Esc".cyan());
                spans.push(Span::raw(" edit prev"));
            }
        }

        let hint_line = Line::from(spans).style(Style::default().add_modifier(Modifier::DIM));
        Paragraph::new(vec![hint_line])
            .wrap(Wrap { trim: false })
            .render_ref(area, buf);
    }
}

/// Extract `@query` at the end of the input for file search trigger.
pub(crate) fn extract_at_search_query(input: &str) -> Option<String> {
    // Find last '@' and take trailing token (letters, digits, separators '-', '_', '.', '/')
    let last_at = input.rfind('@')?;
    let tail = &input[(last_at + 1)..];
    if tail.is_empty() {
        return None;
    }
    // Stop on whitespace
    let mut end = tail.len();
    for (i, ch) in tail.char_indices() {
        if ch.is_whitespace() {
            end = i; break;
        }
    }
    let q = &tail[..end];
    if q.is_empty() { None } else { Some(q.to_string()) }
}

impl WidgetRef for &ChatComposer {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let [textarea_rect, hint_rect] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(if self.show_hints { 1 } else { 0 }),
        ])
        .areas(area);

        let border_style = if self.has_focus {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        };

        // Create a Block with icon as title
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(Line::from(vec![Span::styled("→ ", border_style)]));

        // Get the inner area after applying the block
        let inner_area = block.inner(textarea_rect);

        // Render the block
        block.render_ref(textarea_rect, buf);

        // Render textarea in the inner area
        {
            let mut state = self.textarea_state.borrow_mut();
            StatefulWidgetRef::render_ref(&&self.textarea, inner_area, buf, &mut *state);
        }

        // Render placeholder if textarea is empty
        if self.textarea.is_empty() && !self.placeholder_text.is_empty() {
            let placeholder_line = Line::from(self.placeholder_text.as_str())
                .style(Style::default().add_modifier(Modifier::DIM));
            Paragraph::new(vec![placeholder_line])
                .render_ref(inner_area, buf);
        }

        // Render hints if enabled
        if self.show_hints && hint_rect.height > 0 {
            self.render_hints(hint_rect, buf);
        }
    }
}
