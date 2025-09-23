//! Bottom pane: shows the composer or an overlay view.
use crossterm::event::KeyEvent;
use ratatui::widgets::WidgetRef;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
};

mod bottom_pane_view;
pub(crate) use bottom_pane_view::BottomPaneView;
pub mod approval_modal_view;
pub mod chat_composer;
pub mod chat_composer_history;
pub mod command_popup;
pub mod file_search_popup;
pub mod list_selection_view;
pub mod paste_burst;
pub mod popup_consts;
pub mod scroll_state;
pub mod selection_popup_common;
pub mod textarea;
use crate::app_event_sender::AppEventSender;
use crate::status_indicator_widget::StatusIndicatorWidget;
use crate::user_approval_widget::ApprovalRequest;
use approval_modal_view::ApprovalModalView;
pub use chat_composer::{ChatComposer, InputResult};
use file_search_popup::FileSearchPopup;
use list_selection_view::{ListSelectionView, SelectionAction, SelectionItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancellationEvent {
    Ignored,
    Handled,
}

/// 下部ペイン。通常はコンポーザーを表示し、モーダル等がある場合はビューを差し替える。
pub(crate) struct BottomPane {
    // コンポーザー
    composer: ChatComposer,

    /// アクティブビュー（ある場合はコンポーザーの代わりに描画）
    active_view: Option<Box<dyn BottomPaneView>>,
    file_search: Option<FileSearchPopup>,

    has_input_focus: bool,
    is_task_running: bool,

    /// Inline status indicator shown above the composer while a task is running.
    status: Option<StatusIndicatorWidget>,
    /// Queued user messages to show under the status indicator.
    queued_user_messages: Vec<String>,
    /// Pending local images to attach with the next submission
    recent_submission_images: Vec<std::path::PathBuf>,
}

pub(crate) struct BottomPaneParams {
    pub(crate) has_input_focus: bool,
    pub(crate) placeholder_text: String,
}

impl BottomPane {
    const BOTTOM_PAD_LINES: u16 = 1;

    pub fn new(params: BottomPaneParams) -> Self {
        let mut composer =
            ChatComposer::new_minimal(params.has_input_focus, params.placeholder_text);
        composer.set_show_hints(true);

        Self {
            composer,
            active_view: None,
            file_search: None,
            has_input_focus: params.has_input_focus,
            is_task_running: false,
            status: None,
            queued_user_messages: Vec::new(),
            recent_submission_images: Vec::new(),
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        let mut base = if let Some(view) = self.active_view.as_ref() {
            view.desired_height(width)
        } else {
            self.composer.desired_height(width)
        };

        // If a status indicator is active and no modal is covering the composer,
        // include its height above the composer plus spacing.
        if self.active_view.is_none() {
            if let Some(status) = self.status.as_ref() {
                base = base.saturating_add(status.desired_height(width));
                base = base.saturating_add(1); // Add spacing between status and composer
            }
        }

        base = base.saturating_add(Self::BOTTOM_PAD_LINES);
        base
    }

    fn layout(&self, area: Rect) -> [Rect; 2] {
        let status_height = if self.active_view.is_none() {
            if let Some(status) = self.status.as_ref() {
                status.desired_height(area.width)
            } else {
                0
            }
        } else {
            0
        };

        let top_margin = if self.active_view.is_some() { 0 } else { 1 };
        let bottom_pad = if area.height > 0 {
            Self::BOTTOM_PAD_LINES.min(area.height)
        } else {
            0
        };

        // Add spacing between status and content when status is visible
        let spacing = if status_height > 0 { 1 } else { 0 };

        let [_, status, _spacing, content, _] = Layout::vertical([
            Constraint::Max(top_margin),
            Constraint::Max(status_height),
            Constraint::Length(spacing),
            Constraint::Min(1),
            Constraint::Max(bottom_pad),
        ])
        .areas(area);

        [status, content]
    }

    /// 画面上のカーソル位置（本簡易版では None）
    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        // Hide the cursor whenever an overlay view is active (e.g. the
        // status indicator shown while a task is running, or approval modal).
        // In these states the textarea is not interactable, so we should not
        // show its caret.
        if self.active_view.is_some() {
            None
        } else {
            let [_, content] = self.layout(area);
            self.composer.cursor_pos(content)
        }
    }

    /// キーイベント委譲
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<InputResult> {
        if let Some(mut view) = self.active_view.take() {
            view.handle_key_event(self, key_event);
            if !view.is_complete() {
                self.active_view = Some(view);
            }
            None
        } else {
            // If a task is running and a status line is visible, allow Esc to
            // send an interrupt even while the composer has focus.
            if matches!(key_event.code, crossterm::event::KeyCode::Esc)
                && self.is_task_running
                && self.status.is_some()
            {
                // TODO: Send Op::Interrupt when we have the event sender
                return None;
            }
            // Intercept Ctrl+V here to enqueue an image attachment (codex-1 準拠)
            if let KeyEvent { code: crossterm::event::KeyCode::Char('v'), modifiers: crossterm::event::KeyModifiers::CONTROL, .. } = key_event {
                if let Ok((path, _info)) = crate::clipboard_paste::paste_image_to_temp_png() {
                    self.recent_submission_images.push(path);
                    if let Some(status) = self.status.as_mut() {
                        status.set_attachments_count(self.recent_submission_images.len());
                    }
                    return None;
                }
            }

            let (res, _redraw) = self.composer.handle_key_event(key_event);
            // Detect @query and open file search popup with that query.
            // We keep this logic simple: if popup not open and composer text contains '@...'
            if self.file_search.is_none() {
                if let Some(q) = crate::bottom_pane::chat_composer::extract_at_search_query(self.composer.text()) {
                    self.show_file_search();
                    if let Some(p) = self.file_search.as_mut() {
                        p.set_query(&q);
                    }
                }
            }
            // If popup is visible and user hits Enter without navigating, accept selection and
            // insert selected path into the composer at cursor.
            if let Some(p) = self.file_search.as_mut() {
                if let crossterm::event::KeyEvent { code: crossterm::event::KeyCode::Enter, .. } = key_event {
                    if let Some(sel) = p.selected_match() {
                        self.composer.insert_str(sel);
                        self.hide_file_search();
                        return Some(InputResult::None);
                    }
                }
            }
            match res {
                InputResult::Submitted(_) => Some(res),
                _ => None,
            }
        }
    }

    /// Ctrl-C の処理（ビューがあれば優先）
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        if let Some(mut view) = self.active_view.take() {
            let ev = view.on_ctrl_c(self);
            if !view.is_complete() {
                self.active_view = Some(view);
            }
            ev
        } else {
            CancellationEvent::Ignored
        }
    }

    pub(crate) fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;
        
        // タスク状態をChatComposerに通知（アニメーション表示用）
        self.composer.set_task_running(running);
    }

    pub(crate) fn set_has_focus(&mut self, has_focus: bool) {
        self.has_input_focus = has_focus;
        self.composer.set_has_focus(has_focus);
    }

    fn _setup_task_status(&mut self, running: bool) {
        if running {
            if self.status.is_none() {
                self.status = Some(StatusIndicatorWidget::new());
            }
            if let Some(status) = self.status.as_mut() {
                status.set_queued_messages(self.queued_user_messages.clone());
            }
        } else {
            // Hide the status indicator when a task completes, but keep other modal views.
            self.status = None;
        }
    }

}

impl WidgetRef for &BottomPane {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // 🚨 問題：フォーカス状態の設定ができない（&selfのため）
        // codex-1では別の方法でフォーカス状態を管理している可能性
        
        let [status_area, content] = self.layout(area);

        // When a modal view is active, it owns the whole content area.
        if let Some(view) = &self.active_view {
            view.render(content, buf);
        } else {
            // No active modal:
            // If a status indicator is active, render it above the composer.
            if let Some(status) = &self.status {
                status.render_ref(status_area, buf);
            }

            // Render the composer or the file-search popup
            if let Some(p) = &self.file_search {
                p.render_ref(content, buf);
            } else {
                (&self.composer).render_ref(content, buf);
            }
        }
    }
}

impl BottomPane {
    /// Whether there is an active overlay view that should intercept input
    pub fn is_intercepting_input(&self) -> bool {
        self.active_view.is_some() || self.file_search.is_some()
    }

    pub fn show_file_search(&mut self) {
        self.file_search = Some(FileSearchPopup::new());
    }

    pub fn hide_file_search(&mut self) {
        self.file_search = None;
    }

    pub fn file_search_mut(&mut self) -> Option<&mut FileSearchPopup> {
        self.file_search.as_mut()
    }

    pub fn show_selection_view(
        &mut self,
        title: String,
        subtitle: Option<String>,
        footer_hint: Option<String>,
        items: Vec<SelectionItem>,
        app_event_tx: AppEventSender,
    ) {
        self.active_view = Some(Box::new(ListSelectionView::new(
            title, subtitle, footer_hint, items, app_event_tx,
        )));
    }

    /// 承認モーダルの表示
    pub fn show_approval_modal(&mut self, req: ApprovalRequest, tx: AppEventSender) {
        self.active_view = Some(Box::new(ApprovalModalView::new(req, tx)));
    }

    /// Update the animated header shown to the left of the brackets in the
    /// status indicator (defaults to "Working"). No-ops if the status
    /// indicator is not active.
    pub(crate) fn update_status_header(&mut self, header: String) {
        if let Some(status) = self.status.as_mut() {
            status.update_header(header);
        }
    }

    pub(crate) fn set_composer_placeholder(&mut self, text: String) {
        self.composer.set_placeholder_text(text);
    }

    /// Update the queued messages shown under the status header.
    pub(crate) fn set_queued_user_messages(&mut self, queued: Vec<String>) {
        self.queued_user_messages = queued.clone();
        if let Some(status) = self.status.as_mut() {
            status.set_queued_messages(queued);
        }
    }

    pub(crate) fn is_task_running(&self) -> bool {
        self.is_task_running
    }

    /// Drain and return images queued for the most recent submission.
    pub(crate) fn take_recent_submission_images(&mut self) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        std::mem::swap(&mut out, &mut self.recent_submission_images);
        if let Some(status) = self.status.as_mut() {
            status.set_attachments_count(0);
        }
        out
    }
}
