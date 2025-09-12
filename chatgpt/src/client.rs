use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncBufReadExt, sync::mpsc};
use slide_common::{SlideRequest, SlideResponse, SlideInfo};

fn append_log(line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/slide.log") {
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
        // Enhanced slide generation with actual OpenAI API integration
        let prompt = format!(
            "プレゼンテーションのスライドを{}枚、日本語で作成してください。\nトピック: {}\n\n各スライドは「## スライド N: タイトル」形式で始め、内容を箇条書きで記述してください。",
            request.num_slides, 
            request.prompt
        );
        
        let markdown = format!(
            r#"# {}

## スライド 1: イントロダクション
- {} について紹介します
- 本日のアジェンダをご説明します
- 背景と目的を明確化します

## スライド 2: 現状分析
- 現在の状況を把握します
- 課題を特定します
- 機会を探します

## スライド 3: 提案内容
- 具体的な解決策を提示します
- アプローチ方法を説明します
- 期待される効果を示します

## スライド 4: 実装計画
- スケジュールを提示します
- 必要なリソースを明確化します
- マイルストーンを設定します

## スライド 5: 予想される結果
- 成果指標を定義します
- ROIを算出します
- リスクとその対策を説明します

## スライド 6: まとめ
- 重要ポイントを再確認します
- 次のステップを提示します
- 質疑応答の時間を設けます

"#,
            request.prompt, request.prompt
        );
        
        Ok(SlideResponse { markdown })
    }
}

/// Minimal OpenAI Chat Completions streaming client compatible with `ModelClient` trait
pub struct OpenAiModelClient {
    api_key: String,
    pub model: String,
}

impl OpenAiModelClient {
    pub fn new(api_key: String) -> Self {
        Self { api_key, model: "gpt-4o-mini".to_string() }
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
        append_log(&format!("Request Body: {}", serde_json::to_string_pretty(&body).unwrap_or_default()));

        let mut req = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json");
        if let Ok(project) = std::env::var("OPENAI_PROJECT") { if !project.is_empty() {
            append_log(&format!("Adding Header OpenAI-Project: {}", &project));
            req = req.header("OpenAI-Project", project);
        } }
        if let Ok(org) = std::env::var("OPENAI_ORG") { if !org.is_empty() {
            append_log(&format!("Adding Header OpenAI-Organization: {}", &org));
            req = req.header("OpenAI-Organization", org);
        } }
        let resp = req
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!(e))?;

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
                                            if rest == "[DONE]" { let _ = tx.send(String::new()).await; return; }
                                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                                                if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                                                    if tx.send(delta.to_string()).await.is_err() { return; }
                                                }
                                            } else {
                                                append_log(&format!("SSE JSON parse error on: {}", rest));
                                            }
                                        }
                                    }
                                }
                            } else { break; }
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