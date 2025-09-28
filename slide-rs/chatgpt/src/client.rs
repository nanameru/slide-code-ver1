use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncBufReadExt, sync::mpsc};

#[derive(Debug, Serialize)]
pub struct SlideRequest {
    pub prompt: String,
    pub num_slides: usize,
    pub language: String,
}

#[derive(Debug, Deserialize)]
pub struct SlideResponse {
    pub markdown: String,
}

fn append_log(line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/slide.log")
    {
        let _ = writeln!(f, "[chatgpt-client] {}", line);
    }
}

pub struct ChatGptClient {
    #[allow(dead_code)]
    api_key: String,
}

impl ChatGptClient {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }

    pub async fn generate_slides(&self, request: SlideRequest) -> Result<SlideResponse> {
        // TODO: Implement actual OpenAI API call
        // For now, return a mock response
        let mock_markdown = format!(
            r#"# {}

## Slide 1: Introduction
- Point A
- Point B

## Slide 2: Content
- Content point 1
- Content point 2

"#,
            request.prompt
        );

        Ok(SlideResponse {
            markdown: mock_markdown,
        })
    }
}

/// Minimal OpenAI Chat Completions streaming client compatible with `ModelClient` trait
pub struct OpenAiModelClient {
    api_key: String,
    pub model: String,
}

impl OpenAiModelClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "gpt-5".to_string(),
        }
    }

    pub fn new_with_model(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }

    pub async fn stream_chat(&self, prompt: String) -> Result<mpsc::Receiver<String>> {
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role":"user","content": prompt}],
            "stream": true,
        });
        append_log(&format!(
            "Request Body: {}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        ));

        let mut req = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json");
        if let Ok(project) = std::env::var("OPENAI_PROJECT") {
            if !project.is_empty() {
                append_log(&format!("Adding Header OpenAI-Project: {}", &project));
                req = req.header("OpenAI-Project", project);
            }
        }
        if let Ok(org) = std::env::var("OPENAI_ORG") {
            if !org.is_empty() {
                append_log(&format!("Adding Header OpenAI-Organization: {}", &org));
                req = req.header("OpenAI-Organization", org);
            }
        }
        let resp = req.json(&body).send().await.map_err(|e| anyhow!(e))?;

        let status = resp.status();
        append_log(&format!("Response Status: {}", status));

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let log_msg = format!("openai http {}: {}", status, text);
            append_log(&log_msg);
            return Err(anyhow!(log_msg));
        }

        let stream = resp.bytes_stream();
        let (tx, rx) = mpsc::channel::<String>(64);
        tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut buf = Vec::new();
            let mut stream = Box::pin(stream);
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        append_log(&format!("Received chunk ({} bytes)", bytes.len()));
                        buf.extend_from_slice(&bytes);
                        // Process Server-Sent Events lines split by "\n\n"
                        loop {
                            if let Some(pos) = memchr::memmem::find(&buf, b"\n\n") {
                                let part = buf.drain(..pos + 2).collect::<Vec<u8>>();
                                if let Ok(text) = String::from_utf8(part) {
                                    for line in text.lines() {
                                        let line = line.trim_start();
                                        if let Some(rest) = line.strip_prefix("data: ") {
                                            if rest == "[DONE]" {
                                                let _ = tx.send(String::new()).await;
                                                return;
                                            }
                                            if let Ok(v) =
                                                serde_json::from_str::<serde_json::Value>(rest)
                                            {
                                                // Try Chat Completions: choices.0.delta.content as string
                                                if let Some(s) = v["choices"][0]["delta"]["content"].as_str() {
                                                    if !s.is_empty() {
                                                        if tx.send(s.to_string()).await.is_err() { return; }
                                                    }
                                                } else {
                                                    // Try Responses-like: choices.0.delta.content as array of blocks
                                                    if let Some(arr) = v["choices"][0]["delta"]["content"].as_array() {
                                                        for item in arr {
                                                            let t = item["text"].as_str().or_else(|| item["content"].as_str());
                                                            if let Some(text) = t { if !text.is_empty() { if tx.send(text.to_string()).await.is_err() { return; } } }
                                                        }
                                                    }
                                                    // If function/tool calls appear, surface a marker line consumers can parse
                                                    if let Some(tc) = v["choices"][0]["delta"]["tool_calls"].as_array() {
                                                        for call in tc {
                                                            let name = call["function"]["name"].as_str().unwrap_or("");
                                                            let args = call["function"]["arguments"].as_str().unwrap_or("");
                                                            let marker = format!("__TOOL_CALL__{{\"name\":\"{}\",\"arguments\":{}}}", name, args);
                                                            if tx.send(marker).await.is_err() { return; }
                                                        }
                                                    }
                                                }
                                            } else {
                                                append_log(&format!(
                                                    "SSE JSON parse error on: {}",
                                                    rest
                                                ));
                                            }
                                        }
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        append_log(&format!("Stream chunk error: {}", e));
                        let _ = tx.send("".into()).await;
                        return;
                    }
                }
            }
            append_log("Stream finished");
            let _ = tx.send("".into()).await;
        });
        Ok(rx)
    }

    /// Enhanced streaming that also returns initial rate limit meta and the receiver.
    /// This is used by the higher-level adapter to emit ResponseEvent::RateLimits once.
    pub async fn stream_chat_with_meta(&self, prompt: String) -> Result<StreamWithMeta> {
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role":"user","content": prompt}],
            "stream": true,
        });
        append_log(&format!(
            "Request Body(meta): {}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        ));

        let mut req = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json");
        if let Ok(project) = std::env::var("OPENAI_PROJECT") {
            if !project.is_empty() {
                append_log(&format!("Adding Header OpenAI-Project: {}", &project));
                req = req.header("OpenAI-Project", project);
            }
        }
        if let Ok(org) = std::env::var("OPENAI_ORG") {
            if !org.is_empty() {
                append_log(&format!("Adding Header OpenAI-Organization: {}", &org));
                req = req.header("OpenAI-Organization", org);
            }
        }
        let resp = req.json(&body).send().await.map_err(|e| anyhow!(e))?;

        let status = resp.status();
        append_log(&format!("Response Status(meta): {}", status));
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let log_msg = format!("openai http {}: {}", status, text);
            append_log(&log_msg);
            return Err(anyhow!(log_msg));
        }

        // Extract basic rate limit info from headers if present
        let headers = resp.headers().clone();
        let parse_u32 = |k: &str| -> Option<u32> {
            headers.get(k).and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<u32>().ok())
        };
        let parse_u64 = |k: &str| -> Option<u64> {
            headers.get(k).and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<u64>().ok())
        };
        let rate_limits = RateLimitInfo {
            requests_remaining: parse_u32("x-ratelimit-remaining-requests"),
            requests_reset_at: parse_u64("x-ratelimit-reset-requests"),
            tokens_remaining: parse_u32("x-ratelimit-remaining-tokens"),
            tokens_reset_at: parse_u64("x-ratelimit-reset-tokens"),
        };

        let stream = resp.bytes_stream();
        let (tx, rx) = mpsc::channel::<String>(64);
        tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut buf = Vec::new();
            let mut stream = Box::pin(stream);
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        append_log(&format!("Received chunk(meta) ({} bytes)", bytes.len()));
                        buf.extend_from_slice(&bytes);
                        loop {
                            if let Some(pos) = memchr::memmem::find(&buf, b"\n\n") {
                                let part = buf.drain(..pos + 2).collect::<Vec<u8>>();
                                if let Ok(text) = String::from_utf8(part) {
                                    for line in text.lines() {
                                        let line = line.trim_start();
                                        if let Some(rest) = line.strip_prefix("data: ") {
                                            if rest == "[DONE]" {
                                                let _ = tx.send(String::new()).await;
                                                return;
                                            }
                                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                                                if let Some(s) = v["choices"][0]["delta"]["content"].as_str() {
                                                    if !s.is_empty() { if tx.send(s.to_string()).await.is_err() { return; } }
                                                } else {
                                                    if let Some(arr) = v["choices"][0]["delta"]["content"].as_array() {
                                                        for item in arr {
                                                            let t = item["text"].as_str().or_else(|| item["content"].as_str());
                                                            if let Some(text) = t { if !text.is_empty() { if tx.send(text.to_string()).await.is_err() { return; } } }
                                                        }
                                                    }
                                                    if let Some(tc) = v["choices"][0]["delta"]["tool_calls"].as_array() {
                                                        for call in tc {
                                                            let name = call["function"]["name"].as_str().unwrap_or("");
                                                            let args = call["function"]["arguments"].as_str().unwrap_or("");
                                                            let marker = format!("__TOOL_CALL__{{\"name\":\"{}\",\"arguments\":{}}}", name, args);
                                                            if tx.send(marker).await.is_err() { return; }
                                                        }
                                                    }
                                                }
                                            } else {
                                                append_log(&format!("SSE JSON parse error on(meta): {}", rest));
                                            }
                                        }
                                    }
                                }
                            } else { break; }
                        }
                    }
                    Err(e) => {
                        append_log(&format!("Stream chunk error(meta): {}", e));
                        let _ = tx.send(String::new()).await;
                        return;
                    }
                }
            }
            append_log("Stream finished(meta)");
            let _ = tx.send(String::new()).await;
        });
        Ok(StreamWithMeta { rx, rate_limits: Some(rate_limits) })
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub requests_remaining: Option<u32>,
    pub requests_reset_at: Option<u64>,
    pub tokens_remaining: Option<u32>,
    pub tokens_reset_at: Option<u64>,
}

pub struct StreamWithMeta {
    pub rx: mpsc::Receiver<String>,
    pub rate_limits: Option<RateLimitInfo>,
}
