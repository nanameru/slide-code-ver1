use crossterm::event::KeyEvent;
use ratatui::{buffer::Buffer, layout::Rect, widgets::WidgetRef};

use crate::app_event_sender::AppEventSender;
use crate::user_approval_widget::{ApprovalRequest, UserApprovalWidget};

use super::{bottom_pane_view::BottomPaneView, BottomPane, CancellationEvent};

/// 危険操作の承認モーダル（承認/却下の受け付け）
pub(crate) struct ApprovalModalView {
    current: UserApprovalWidget,
    queue: Vec<ApprovalRequest>,
    app_event_tx: AppEventSender,
}

impl ApprovalModalView {
    pub fn new(request: ApprovalRequest, app_event_tx: AppEventSender) -> Self {
        Self {
            current: UserApprovalWidget::new(request, app_event_tx.clone()),
            queue: Vec::new(),
            app_event_tx,
        }
    }
    pub fn enqueue_request(&mut self, req: ApprovalRequest) {
        self.queue.push(req);
    }
    fn maybe_advance(&mut self) {
        if self.current.is_complete() {
            if let Some(req) = self.queue.pop() {
                self.current = UserApprovalWidget::new(req, self.app_event_tx.clone());
            }
        }
    }
}

impl BottomPaneView for ApprovalModalView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane, key_event: KeyEvent) {
        self.current.handle_key_event(key_event);
        self.maybe_advance();
    }
    fn on_ctrl_c(&mut self, _pane: &mut BottomPane) -> CancellationEvent {
        self.current.on_ctrl_c();
        self.queue.clear();
        CancellationEvent::Handled
    }
    fn is_complete(&self) -> bool {
        self.current.is_complete() && self.queue.is_empty()
    }
    fn desired_height(&self, width: u16) -> u16 {
        self.current.desired_height(width)
    }
    fn render(&self, area: Rect, buf: &mut Buffer) {
        (&self.current).render_ref(area, buf);
    }
    // Additional queuing API can be added to the trait in the future; for now, queue is internal
}
