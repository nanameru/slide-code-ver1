//! Bottom pane: shows the composer or an overlay view.
use crossterm::event::KeyEvent;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};
use ratatui::widgets::WidgetRef;

mod bottom_pane_view;
pub(crate) use bottom_pane_view::BottomPaneView;
pub mod chat_composer;
pub mod chat_composer_history;
pub mod textarea;
pub mod command_popup;
pub mod file_search_popup;
pub mod list_selection_view;
pub mod selection_popup_common;
pub mod popup_consts;
pub mod scroll_state;
pub mod approval_modal_view;
pub mod paste_burst;
pub use chat_composer::{ChatComposer, InputResult};
use crate::app_event_sender::AppEventSender;
use crate::user_approval_widget::ApprovalRequest;
use approval_modal_view::ApprovalModalView;

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

    has_input_focus: bool,
    is_task_running: bool,
}

pub(crate) struct BottomPaneParams {
    pub(crate) has_input_focus: bool,
    pub(crate) placeholder_text: String,
}

impl BottomPane {
    const BOTTOM_PAD_LINES: u16 = 1;

    pub fn new(params: BottomPaneParams) -> Self {
        Self {
            composer: ChatComposer::new_minimal(params.has_input_focus, params.placeholder_text),
            active_view: None,
            has_input_focus: params.has_input_focus,
            is_task_running: false,
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        let mut base = if let Some(view) = self.active_view.as_ref() {
            view.desired_height(width)
        } else {
            self.composer.desired_height(width)
        };
        base = base.saturating_add(Self::BOTTOM_PAD_LINES);
        base
    }

    fn layout(&self, area: Rect) -> Rect {
        // 余白 + コンテンツ（最低1行）+ 下パディング
        let [_, content, _] = Layout::vertical([
            Constraint::Max(0),
            Constraint::Min(1),
            Constraint::Max(Self::BOTTOM_PAD_LINES),
        ])
        .areas(area);
        content
    }

    /// 画面上のカーソル位置（本簡易版では None）
    pub fn cursor_pos(&self, _area: Rect) -> Option<(u16, u16)> {
        if self.active_view.is_some() {
            None
        } else if self.has_input_focus {
            None
        } else {
            None
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
            let (res, _redraw) = self.composer.handle_key_event(key_event);
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
    }

    /// 簡易描画（Paragraph ベース）
    pub fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let content = self.layout(area);
        if let Some(view) = &self.active_view {
            view.render(content, buf);
            return;
        }
        // Composer
        // ChatComposer implements WidgetRef for &Self
        self.composer.render_ref(content, buf);
    }

    /// Whether there is an active overlay view that should intercept input
    pub fn is_intercepting_input(&self) -> bool {
        self.active_view.is_some()
    }
}

impl BottomPane {
    /// 承認モーダルの表示
    pub fn show_approval_modal(&mut self, req: ApprovalRequest, tx: AppEventSender) {
        self.active_view = Some(Box::new(ApprovalModalView::new(req, tx)));
    }
}
