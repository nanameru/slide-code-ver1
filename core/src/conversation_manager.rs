use crate::codex2::{Codex, Event, Op};
use crate::error::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ConversationManager {
    codex: Arc<Mutex<Option<Codex>>>,
}

impl ConversationManager {
    pub fn new() -> Self {
        Self {
            codex: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_codex(&self, codex: Codex) {
        let mut guard = self.codex.lock().await;
        *guard = Some(codex);
    }

    pub async fn submit_input(&self, text: String) -> Result<()> {
        let guard = self.codex.lock().await;
        if let Some(ref codex) = *guard {
            codex.submit(Op::UserInput { text }).await
                .map_err(|e| crate::error::CodexError::Generic(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn next_event(&self) -> Option<Event> {
        let guard = self.codex.lock().await;
        if let Some(ref codex) = *guard {
            codex.next_event().await
        } else {
            None
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        let guard = self.codex.lock().await;
        if let Some(ref codex) = *guard {
            codex.submit(Op::Shutdown).await
                .map_err(|e| crate::error::CodexError::Generic(e.to_string()))?;
        }
        Ok(())
    }
}