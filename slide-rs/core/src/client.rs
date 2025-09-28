use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;
use crate::conversation_history::ResponseItem;
use protocol::protocol::TokenUsage;

#[derive(Debug, Clone)]
pub enum ResponseEvent {
    // 既存のイベント（互換性維持）
    TextDelta(String),
    Completed,
    Error(String),
    
    // codex-1レベルの拡張イベント
    Created,
    OutputItemDone(ResponseItem),
    CompletedWithDetails {
        response_id: String,
        token_usage: Option<TokenUsage>,
    },
    OutputTextDelta(String),
    ReasoningSummaryDelta(String),
    ReasoningContentDelta(String),
    ReasoningSummaryPartAdded,
    WebSearchCallBegin {
        call_id: String,
    },
    RateLimits(RateLimitSnapshot),
}

// RateLimitSnapshot構造体を定義
#[derive(Debug, Clone)]
pub struct RateLimitSnapshot {
    pub requests_remaining: Option<u32>,
    pub requests_reset_at: Option<u64>,
    pub tokens_remaining: Option<u32>,
    pub tokens_reset_at: Option<u64>,
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
        let _ = tx
            .send(ResponseEvent::TextDelta(format!("echo: {}", prompt)))
            .await;
        let _ = tx.send(ResponseEvent::Completed).await;
        Ok(rx)
    }
}

/// Adapter to wrap OpenAiModelClient into ModelClient
pub struct OpenAiAdapter {
    inner: slide_chatgpt::OpenAiModelClient,
}

impl OpenAiAdapter {
    pub fn new(api_key: String) -> Self {
        Self {
            inner: slide_chatgpt::OpenAiModelClient::new(api_key),
        }
    }

    pub fn new_with_model(api_key: String, model: String) -> Self {
        Self {
            inner: slide_chatgpt::OpenAiModelClient::new_with_model(api_key, model),
        }
    }
}

#[async_trait]
impl ModelClient for OpenAiAdapter {
    async fn stream(&self, prompt: String) -> Result<Receiver<ResponseEvent>> {
        let mut rx_text = self.inner.stream_chat(prompt).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(delta) = rx_text.recv().await {
                if delta.is_empty() {
                    let _ = tx.send(ResponseEvent::Completed).await;
                    break;
                }
                // Detect tool-call marker lines emitted by slide-chatgpt client
                if let Some(rest) = delta.strip_prefix("__TOOL_CALL__") {
                    // rest is JSON like {"name":"...","arguments":{...}}
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                        let name = v["name"].as_str().unwrap_or("").to_string();
                        let args = v["arguments"].to_string();
                        let item = crate::conversation_history::ResponseItem::FunctionCall {
                            id: None,
                            name,
                            arguments: args,
                            call_id: uuid::Uuid::new_v4().to_string(),
                        };
                        let _ = tx.send(ResponseEvent::OutputItemDone(item)).await;
                        continue;
                    }
                }
                if tx.send(ResponseEvent::TextDelta(delta)).await.is_err() {
                    break;
                }
            }
        });
        Ok(rx)
    }
}
