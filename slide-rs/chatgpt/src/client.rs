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
        Self { api_key, model: "gpt-5".to_string() }
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
        let mut req = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json");
        if let Ok(project) = std::env::var("OPENAI_PROJECT") { if !project.is_empty() { req = req.header("OpenAI-Project", project); } }
        if let Ok(org) = std::env::var("OPENAI_ORG") { if !org.is_empty() { req = req.header("OpenAI-Organization", org); } }
        let resp = req
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!(e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(format!("openai http {}: {}", status, text)));
        }

        let stream = resp.bytes_stream();
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
        tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut buf = Vec::new();
            let mut stream = Box::pin(stream);
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buf.extend_from_slice(&bytes);
                        // Process Server-Sent Events lines split by "\n\n"
                        loop {
                            if let Some(pos) = memchr::memmem::find(&buf, b"\n\n") {
                                let part = buf.drain(..pos + 2).collect::<Vec<u8>>();
                                if let Ok(text) = String::from_utf8(part) {
                                    for line in text.lines() {
                                        let line = line.trim_start();
                                        if let Some(rest) = line.strip_prefix("data: ") {
                                            if rest == "[DONE]" { let _ = tx.send(String::new()).await; return; }
                                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                                                if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                                                    if tx.send(delta.to_string()).await.is_err() { return; }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else { break; }
                        }
                    }
                    Err(_) => { let _ = tx.send("".into()).await; return; }
                }
            }
            let _ = tx.send("".into()).await;
        });
        Ok(rx)
    }
}