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

use crate::animations::AnimationManager;
use crate::ui_consts::LIVE_PREFIX_COLS;

use super::{
    chat_composer_history::ChatComposerHistory,
    paste_burst::{CharDecision, FlushResult, PasteBurst},
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
    animations: AnimationManager,
    is_task_running: bool,
    paste_burst: PasteBurst,
}

impl ChatComposer {
    pub fn new_minimal(has_input_focus: bool, placeholder_text: String) -> Self {
        Self {
            textarea: TextArea::new(),
            textarea_state: RefCell::new(TextAreaState::default()),
            history: ChatComposerHistory::new(),
            has_focus: true,  // 🎯 強制的にフォーカス状態をtrueに（テスト用）
            placeholder_text,
            ctrl_c_quit_hint: false,
            esc_backtrack_hint: false,
            use_shift_enter_hint: true,
            show_hints: true,
            animations: AnimationManager::new(),
            is_task_running: false,
            paste_burst: PasteBurst::default(),
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
        // Leave columns for the left border and padding
        let inner_width = width.saturating_sub(LIVE_PREFIX_COLS);
        let textarea_height = self.textarea.desired_height(inner_width);
        let hints_height = if self.show_hints { 1 } else { 0 };
        textarea_height.saturating_add(hints_height)
    }

    fn handle_paste_burst_flush(&mut self, now: std::time::Instant) -> bool {
        match self.paste_burst.flush_if_due(now) {
            FlushResult::Paste(pasted) => {
                self.handle_paste(pasted);
                true
            }
            FlushResult::Typed(ch) => {
                self.textarea.insert_str(&ch.to_string());
                true
            }
            FlushResult::None => false,
        }
    }

    /// Handle pasted text (e.g., from explicit paste events or drag-and-drop).
    pub fn handle_paste(&mut self, pasted: String) -> bool {
        use crate::clipboard_paste::{normalize_pasted_path, pasted_image_format};
        
        let char_count = pasted.chars().count();
        
        // Try to interpret as an image path
        if char_count > 1 {
            if let Some(path_buf) = normalize_pasted_path(&pasted) {
                // Check if it's a valid image file
                if let Ok((w, h)) = image::image_dimensions(&path_buf) {
                    let format_label = pasted_image_format(&path_buf).label();
                    tracing::info!("Pasted image path detected: {} ({}x{}, {})", 
                        path_buf.display(), w, h, format_label);
                    // For now, just insert a placeholder. In a full implementation,
                    // this would attach the image to the submission.
                    self.textarea.insert_str(&format!("[Image: {}]", path_buf.display()));
                    self.paste_burst.clear_after_explicit_paste();
                    return true;
                }
            }
        }
        
        // Otherwise, insert as plain text
        self.textarea.insert_str(&pasted);
        
        // Explicit paste events should not trigger Enter suppression.
        self.paste_burst.clear_after_explicit_paste();
        
        true
    }

    /// Flush the paste burst buffer if enough time has elapsed.
    pub fn flush_paste_burst_if_due(&mut self) -> bool {
        let now = std::time::Instant::now();
        self.handle_paste_burst_flush(now)
    }

    /// Check if a paste burst is currently being buffered.
    pub fn is_in_paste_burst(&self) -> bool {
        self.paste_burst.is_active()
    }

    /// Recommended delay between paste burst flush checks (for periodic ticks).
    pub fn recommended_paste_flush_delay() -> std::time::Duration {
        use super::paste_burst::PASTE_BURST_CHAR_INTERVAL;
        PASTE_BURST_CHAR_INTERVAL
    }

    /// Clamp a byte position to the nearest char boundary.
    fn clamp_to_char_boundary(text: &str, pos: usize) -> usize {
        if pos >= text.len() {
            return text.len();
        }
        let mut clamped = pos;
        while !text.is_char_boundary(clamped) && clamped > 0 {
            clamped -= 1;
        }
        clamped
    }

    /// Handle non-ASCII char by flushing any active burst and inserting directly.
    fn handle_non_ascii_char(&mut self, input: KeyEvent) -> (InputResult, bool) {
        let now = std::time::Instant::now();
        if let Some(pasted) = self.paste_burst.flush_before_modified_input() {
            self.textarea.insert_str(&pasted);
        }
        self.textarea.input(input);
        (InputResult::None, true)
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        if key_event.kind != KeyEventKind::Press {
            return (InputResult::None, false);
        }

        self.clear_hints();

        // Flush any pending paste burst before handling new input (codex-1 style)
        let now = std::time::Instant::now();
        self.handle_paste_burst_flush(now);

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
                // Intercept plain Char inputs to optionally accumulate into a burst buffer
                let now = std::time::Instant::now();
                
                if let KeyEvent {
                    code: KeyCode::Char(ch),
                    modifiers,
                    ..
                } = other
                {
                    let has_ctrl_or_alt =
                        modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT);
                    if !has_ctrl_or_alt {
                        // Non-ASCII characters (e.g., from IMEs) can arrive in quick bursts and be
                        // misclassified by paste heuristics. Flush any active burst buffer and insert
                        // non-ASCII characters directly.
                        if !ch.is_ascii() {
                            return self.handle_non_ascii_char(other);
                        }

                        match self.paste_burst.on_plain_char(ch, now) {
                            CharDecision::BufferAppend => {
                                self.paste_burst.append_char_to_buffer(ch, now);
                                return (InputResult::None, true);
                            }
                            CharDecision::BeginBuffer { retro_chars } => {
                                let cur = self.textarea.cursor();
                                let txt = self.textarea.text();
                                let safe_cur = Self::clamp_to_char_boundary(txt, cur);
                                let before = &txt[..safe_cur];
                                if let Some(grab) =
                                    self.paste_burst
                                        .decide_begin_buffer(now, before, retro_chars as usize)
                                {
                                    if !grab.grabbed.is_empty() {
                                        self.textarea.replace_range(grab.start_byte..safe_cur, "");
                                    }
                                    self.paste_burst.begin_with_retro_grabbed(grab.grabbed, now);
                                    self.paste_burst.append_char_to_buffer(ch, now);
                                    return (InputResult::None, true);
                                }
                                // If decide_begin_buffer opted not to start buffering,
                                // fall through to normal insertion below.
                            }
                            CharDecision::BeginBufferFromPending => {
                                // First char was held; now append the current one.
                                self.paste_burst.append_char_to_buffer(ch, now);
                                return (InputResult::None, true);
                            }
                            CharDecision::RetainFirstChar => {
                                // Keep the first fast char pending momentarily.
                                return (InputResult::None, true);
                            }
                        }
                    }
                    if let Some(pasted) = self.paste_burst.flush_before_modified_input() {
                        self.textarea.insert_str(&pasted);
                    }
                }

                // For non-char inputs (or after flushing), handle normally
                self.textarea.input(other);
                
                // Update paste-burst heuristic for plain Char (no Ctrl/Alt) events
                if let KeyEvent {
                    code: KeyCode::Char(_),
                    modifiers,
                    ..
                } = other
                {
                    let has_ctrl_or_alt = modifiers.contains(KeyModifiers::CONTROL)
                        || modifiers.contains(KeyModifiers::ALT);
                    if has_ctrl_or_alt {
                        self.paste_burst.clear_window_after_non_char();
                    }
                }
                
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

        // 📐 テキストエリア用の調整（render_refと同じ計算）
        let mut content_area = textarea_rect;
        // Leave space for border and padding
        content_area.width = content_area.width.saturating_sub(LIVE_PREFIX_COLS);
        content_area.x = content_area.x.saturating_add(LIVE_PREFIX_COLS);

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

    pub fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;
    }

    pub fn set_has_focus(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
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

        // 🎨 左側青色バー（codex-1風）
        let border_style = if self.has_focus {
            Style::default().fg(Color::Cyan)    // フォーカス時: シアン（青）
        } else {
            Style::default().add_modifier(Modifier::DIM)    // 非フォーカス時: 薄い
        };
        
        Block::default()
            .borders(Borders::LEFT)             // 左側ボーダーのみ
            .border_type(BorderType::QuadrantOutside)
            .border_style(border_style)
            .render_ref(
                Rect::new(textarea_rect.x, textarea_rect.y, 1, textarea_rect.height),
                buf,
            );

        // 📐 テキストエリア用の調整
        let mut content_area = textarea_rect;
        // Leave space for border and padding
        content_area.width = content_area.width.saturating_sub(LIVE_PREFIX_COLS);
        content_area.x = content_area.x.saturating_add(LIVE_PREFIX_COLS);

        // 📝 テキストエリア描画
        {
            let mut state = self.textarea_state.borrow_mut();
            StatefulWidgetRef::render_ref(&&self.textarea, content_area, buf, &mut *state);
        }

        // 💭 プレースホルダー表示（アニメーション付き）
        if self.textarea.is_empty() && !self.placeholder_text.is_empty() {
            let placeholder_spans = if self.is_task_running {
                // 🎬 タスク実行中はシマーエフェクト
                self.animations.shimmer_spans(&self.placeholder_text)
            } else {
                // 🔘 通常時は薄いグレー
                vec![Span::styled(
                    self.placeholder_text.clone(),
                    Style::default().add_modifier(Modifier::DIM)
                )]
            };
            
            let placeholder_line = Line::from(placeholder_spans);
            Paragraph::new(vec![placeholder_line])
                .render_ref(content_area, buf);
        }

        // 💡 ヒント表示
        if self.show_hints && hint_rect.height > 0 {
            self.render_hints(hint_rect, buf);
        }
    }
}
