use slide_core::codex::ReviewDecision;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub enum AppEvent {
    ExecApproval {
        id: String,
        decision: ReviewDecision,
    },
    PatchApproval {
        id: String,
        decision: ReviewDecision,
    },
}

#[derive(Clone, Default)]
pub struct AppEventSender(Option<UnboundedSender<AppEvent>>);

impl AppEventSender {
    pub fn new(tx: UnboundedSender<AppEvent>) -> Self {
        Self(Some(tx))
    }
    pub fn noop() -> Self {
        Self(None)
    }
    pub fn send(&self, event: AppEvent) {
        if let Some(tx) = &self.0 {
            let _ = tx.send(event);
        }
    }
}
