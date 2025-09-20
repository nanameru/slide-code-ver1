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
                                                if let Some(s) =
                                                    v["choices"][0]["delta"]["content"].as_str()
                                                {
                                                    if !s.is_empty() {
                                                        if tx.send(s.to_string()).await.is_err() {
                                                            return;
                                                        }
                                                    }
                                                } else {
                                                    // Try Responses-like: choices.0.delta.content as array of blocks
                                                    if let Some(arr) = v["choices"][0]["delta"]
                                                        ["content"]
                                                        .as_array()
                                                    {
                                                        for item in arr {
                                                            let t = item["text"].as_str().or_else(
                                                                || item["content"].as_str(),
                                                            );
                                                            if let Some(text) = t {
                                                                if !text.is_empty() {
                                                                    if tx
                                                                        .send(text.to_string())
                                                                        .await
                                                                        .is_err()
                                                                    {
                                                                        return;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    // Minimal surfacing for tool_calls (show that a tool was requested)
                                                    if v["choices"][0]["delta"]["tool_calls"]
                                                        .is_array()
                                                    {
                                                        let _ = tx.send("[tool_call] model proposed a tool operation".to_string()).await;
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
}
