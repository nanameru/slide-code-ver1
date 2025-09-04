use anyhow::Result;
use slide_core::codex::{Codex, CodexSpawnOk, Event as CoreEvent, Op};
use slide_core::client::{ModelClient, OpenAiAdapter, StubClient};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct AgentHandle {
    codex: Codex,
    pub rx: mpsc::Receiver<CoreEvent>,
}

impl AgentHandle {
    pub async fn spawn() -> Result<Self> {
        // Prefer OpenAI if API key present; fallback to stub
        let client: Arc<dyn ModelClient + Send + Sync> = if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            Arc::new(OpenAiAdapter::new(key))
        } else {
            Arc::new(StubClient)
        };
        let CodexSpawnOk { codex, .. } = slide_core::codex::Codex::spawn(client).await?;
        // Forward events to a local channel
        let (tx, rx) = mpsc::channel(256);
        let mut codex_ev = codex.clone();
        tokio::spawn(async move {
            while let Some(ev) = codex_ev.next_event().await {
                if tx.send(ev).await.is_err() { break; }
            }
        });
        Ok(Self { codex, rx })
    }

    pub async fn submit_text(&self, text: String) -> Result<()> {
        let _id = self.codex.submit(Op::UserInput { text }).await?;
        Ok(())
    }

    pub fn submit_text_bg(&self, text: String) {
        let c = self.codex.clone();
        tokio::spawn(async move {
            let _ = c.submit(Op::UserInput { text }).await;
        });
    }
}

