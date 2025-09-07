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

/// Adapter to wrap OpenAiModelClient into ModelClient
pub struct OpenAiAdapter {
    inner: slide_chatgpt::OpenAiModelClient,
}

impl OpenAiAdapter {
    pub fn new(api_key: String) -> Self { Self { inner: slide_chatgpt::OpenAiModelClient::new(api_key) } }

    pub fn new_with_model(api_key: String, model: String) -> Self {
        Self { inner: slide_chatgpt::OpenAiModelClient::new_with_model(api_key, model) }
    }
}

#[async_trait]
impl ModelClient for OpenAiAdapter {
    async fn stream(&self, prompt: String) -> Result<Receiver<ResponseEvent>> {
        let mut rx_text = self.inner.stream_chat(prompt).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(delta) = rx_text.recv().await {
                if delta.is_empty() { let _ = tx.send(ResponseEvent::Completed).await; break; }
                if tx.send(ResponseEvent::TextDelta(delta)).await.is_err() { break; }
            }
        });
        Ok(rx)
    }
}

