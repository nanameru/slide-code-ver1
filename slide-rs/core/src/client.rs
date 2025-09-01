use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;

#[derive(Debug, Clone)]
pub enum ResponseEvent {
    TextDelta(String),
    Completed,
    Error(String),
}

#[async_trait]
pub trait ModelClient {
    async fn stream(&self, prompt: String) -> Result<Receiver<ResponseEvent>>;
}

/// A very small stub client for testing the flow.
pub struct StubClient;

#[async_trait]
impl ModelClient for StubClient {
    async fn stream(&self, prompt: String) -> Result<Receiver<ResponseEvent>> {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let _ = tx.send(ResponseEvent::TextDelta(format!("echo: {}", prompt))).await;
        let _ = tx.send(ResponseEvent::Completed).await;
        Ok(rx)
    }
}

