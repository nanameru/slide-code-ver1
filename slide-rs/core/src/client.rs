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
    fn stream_max_retries(&self) -> usize { 3 }
    fn model_context_window(&self) -> Option<u64> { None }
    fn supports_responses_api(&self) -> bool { false }
    fn provider_name(&self) -> &'static str { "unknown" }
    fn model_name(&self) -> Option<String> { None }
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
    fn stream_max_retries(&self) -> usize { 1 }
    fn model_context_window(&self) -> Option<u64> { None }
    fn supports_responses_api(&self) -> bool { false }
    fn provider_name(&self) -> &'static str { "stub" }
    fn model_name(&self) -> Option<String> { None }
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
        // If Responses API is supported, stream via /responses with structured payload
        if self.supports_responses_api() {
            // Build minimal responses payload (input, optional tools/system added by caller)
            let payload = serde_json::json!({
                "input": [{"role":"user","content": [{"type":"input_text","text": prompt}]}]
            });
            let meta = self.inner.stream_responses_with_meta(payload).await?;
            let (tx, rx) = tokio::sync::mpsc::channel(128);
            if let Some(info) = meta.rate_limits {
                let snapshot = RateLimitSnapshot {
                    requests_remaining: info.requests_remaining,
                    requests_reset_at: info.requests_reset_at,
                    tokens_remaining: info.tokens_remaining,
                    tokens_reset_at: info.tokens_reset_at,
                };
                let _ = tx.send(ResponseEvent::RateLimits(snapshot)).await;
            }
            let mut rx_text = meta.rx;
            tokio::spawn(async move {
                while let Some(delta) = rx_text.recv().await {
                    if delta.is_empty() { let _ = tx.send(ResponseEvent::Completed).await; break; }
                    if let Some(rest) = delta.strip_prefix("__TOOL_CALL__") {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                            let name = v["name"].as_str().unwrap_or("").to_string();
                            let args = v["arguments"].to_string();
                            let item = crate::conversation_history::ResponseItem::FunctionCall { id: None, name, arguments: args, call_id: uuid::Uuid::new_v4().to_string() };
                            let _ = tx.send(ResponseEvent::OutputItemDone(item)).await;
                            continue;
                        }
                    }
                    if let Some(rest) = delta.strip_prefix("__USAGE__") {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                            let usage = protocol::protocol::TokenUsage {
                                input_tokens: v["input_tokens"].as_u64().unwrap_or(0),
                                cached_input_tokens: None,
                                output_tokens: v["output_tokens"].as_u64().unwrap_or(0),
                                reasoning_output_tokens: v["reasoning_output_tokens"].as_u64(),
                                total_tokens: v["total_tokens"].as_u64().unwrap_or(0),
                            };
                            let _ = tx.send(ResponseEvent::CompletedWithDetails { response_id: uuid::Uuid::new_v4().to_string(), token_usage: Some(usage) }).await;
                            continue;
                        }
                    }
                    if tx.send(ResponseEvent::TextDelta(delta)).await.is_err() { break; }
                }
            });
            return Ok(rx);
        }
        // Prefer meta-aware streaming to capture RateLimit headers when possible (Chat Completions)
        let meta = match self.inner.stream_chat_with_meta(prompt).await {
            Ok(m) => m,
            Err(_e) => {
                // Fallback to legacy API
                let mut rx_text = self.inner.stream_chat(String::new()).await?;
                let (tx, rx) = tokio::sync::mpsc::channel(64);
                tokio::spawn(async move {
                    while let Some(delta) = rx_text.recv().await {
                        if delta.is_empty() { let _ = tx.send(ResponseEvent::Completed).await; break; }
                        if let Some(rest) = delta.strip_prefix("__TOOL_CALL__") {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                                let name = v["name"].as_str().unwrap_or("").to_string();
                                let args = v["arguments"].to_string();
                                let item = crate::conversation_history::ResponseItem::FunctionCall { id: None, name, arguments: args, call_id: uuid::Uuid::new_v4().to_string() };
                                let _ = tx.send(ResponseEvent::OutputItemDone(item)).await;
                                continue;
                            }
                        }
                        if tx.send(ResponseEvent::TextDelta(delta)).await.is_err() { break; }
                    }
                });
                return Ok(rx);
            }
        };
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        // Emit RateLimits snapshot once if present
        if let Some(info) = meta.rate_limits {
            let snapshot = RateLimitSnapshot {
                requests_remaining: info.requests_remaining,
                requests_reset_at: info.requests_reset_at,
                tokens_remaining: info.tokens_remaining,
                tokens_reset_at: info.tokens_reset_at,
            };
            let _ = tx.send(ResponseEvent::RateLimits(snapshot)).await;
        }
        let mut rx_text = meta.rx;
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
    fn stream_max_retries(&self) -> usize { 5 }
    fn model_context_window(&self) -> Option<u64> { Some(128_000) }
    fn supports_responses_api(&self) -> bool { true }
    fn provider_name(&self) -> &'static str { "openai" }
    fn model_name(&self) -> Option<String> { Some(self.inner.model.clone()) }
}
