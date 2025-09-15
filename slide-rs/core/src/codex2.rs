use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::client::{ModelClient, ResponseEvent};
use slide_chatgpt::client::{ChatGptClient, SlideRequest};
use crate::openai_tools::{ToolsConfig, ToolsConfigParams, render_tools_instructions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision { Approved, ApprovedForSession, Denied, Abort }

#[derive(Debug, Clone)]
pub enum Event {
    SessionConfigured {},
    TaskStarted,
    AgentMessageDelta { delta: String },
    AgentMessage { message: String },
    ExecCommandBegin { command: Vec<String>, cwd: PathBuf },
    ExecCommandEnd { exit_code: i32 },
    ApplyPatchApprovalRequest {
        id: String,
        changes: HashMap<PathBuf, String>,
        reason: Option<String>,
    },
    PatchApplyBegin {},
    PatchApplyEnd { success: bool },
    TurnDiff { unified_diff: String },
    TaskComplete,
    Error { message: String },
    ShutdownComplete,
    ExecApprovalRequest {
        id: String,
        command: Vec<String>,
        cwd: PathBuf,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum Op {
    UserInput { text: String },
    Interrupt,
    ExecApproval { id: String, decision: ReviewDecision },
    PatchApproval { id: String, decision: ReviewDecision },
    Shutdown,
}

#[derive(Clone)]
pub struct Codex {
    inner: Arc<Inner>,
}

struct Inner {
    tx_submit: mpsc::Sender<Op>,
    rx_event: Mutex<mpsc::Receiver<Event>>,
    /// Minimal in-session conversation memory: sequence of (role, text)
    conversation: Mutex<Vec<(String, String)>>,
}

pub struct CodexSpawnOk {
    pub codex: Codex,
}

impl Codex {
    pub async fn spawn(client: Arc<dyn ModelClient + Send + Sync>) -> Result<CodexSpawnOk> {
        let (tx_submit, mut rx_submit) = mpsc::channel::<Op>(64);
        let (tx_event, rx_event) = mpsc::channel::<Event>(256);

        // Send initial configured event to signal readiness
        let _ = tx_event.send(Event::SessionConfigured {}).await;

        // Background task processing submissions
        tokio::spawn(async move {
            let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
            let slide_client = ChatGptClient::new(api_key);
            // Keep recent conversation messages (role, text). Oldest first.
            let mut convo: Vec<(String, String)> = Vec::new();
            while let Some(op) = rx_submit.recv().await {
                match op {
                    Op::UserInput { text } => {
                        let _ = tx_event.send(Event::TaskStarted).await;
                        if let Some(prompt) = text.strip_prefix("/slide ") {
                            match slide_client.generate_slides(SlideRequest { prompt: prompt.to_string(), num_slides: 6, language: "ja".to_string() }).await {
                                Ok(resp) => {
                                    for line in resp.markdown.lines() {
                                        let delta = format!("{}\n", line);
                                        let _ = tx_event.send(Event::AgentMessageDelta { delta }).await;
                                    }
                                    let save_path = PathBuf::from("slides").join("draft.md");
                                    if let Some(parent) = save_path.parent() { let _ = std::fs::create_dir_all(parent); }
                                    if let Err(e) = std::fs::write(&save_path, resp.markdown.as_bytes()) {
                                        let _ = tx_event.send(Event::Error { message: format!("failed to save slides: {e}") }).await;
                                    } else {
                                        let _ = tx_event.send(Event::AgentMessage { message: format!("Saved to {}", save_path.display()) }).await;
                                    }
                                    let _ = tx_event.send(Event::TaskComplete).await;
                                }
                                Err(e) => { let _ = tx_event.send(Event::Error { message: e.to_string() }).await; }
                            }
                            continue;
                        }
                        // Prefix prompt with tool instructions so the model can propose edits/execs.
                        let approval_hint = std::env::var("SLIDE_APPROVAL_MODE").ok();
                        let tools_cfg = ToolsConfig::new(&ToolsConfigParams {
                            include_plan_tool: true,
                            include_apply_patch_tool: true,
                            include_view_image_tool: false,
                            include_web_search_request: false,
                            use_streamable_shell_tool: true,
                            include_slides_tools: true,
                        });
                        let tool_instructions = render_tools_instructions(&tools_cfg, approval_hint.as_deref());
                        // Append user message to conversation memory
                        convo.push(("user".to_string(), text.clone()));
                        // Cap memory to recent N entries to fit token budget
                        const MAX_HISTORY_MESSAGES: usize = 12; // messages, not turns
                        if convo.len() > MAX_HISTORY_MESSAGES {
                            let drop = convo.len() - MAX_HISTORY_MESSAGES;
                            convo.drain(0..drop);
                        }
                        // Render recent conversation as plain lines
                        let mut history_block = String::new();
                        if !convo.is_empty() {
                            history_block.push_str("\n\nConversation so far:\n");
                            for (role, msg) in &convo {
                                let tag = if role == "assistant" { "Assistant" } else { "User" };
                                history_block.push_str(tag);
                                history_block.push_str(": ");
                                history_block.push_str(msg);
                                if !msg.ends_with('\n') { history_block.push('\n'); }
                            }
                        }
                        let composed = format!("{}{}\n\nUser: {}", tool_instructions, history_block, text);
                        match client.stream(composed).await {
                            Ok(mut rx) => {
                                let mut assembled_resp = String::new();
                                while let Some(ev) = rx.recv().await {
                                    match ev {
                                        ResponseEvent::TextDelta(delta) => {
                                            assembled_resp.push_str(&delta);
                                            let _ = tx_event
                                                .send(Event::AgentMessageDelta { delta })
                                                .await;
                                        }
                                        ResponseEvent::Completed => {
                                            // Store assistant message into conversation
                                            if !assembled_resp.is_empty() {
                                                convo.push(("assistant".to_string(), assembled_resp.clone()));
                                                if convo.len() > MAX_HISTORY_MESSAGES {
                                                    let drop = convo.len() - MAX_HISTORY_MESSAGES;
                                                    convo.drain(0..drop);
                                                }
                                            }
                                            let _ = tx_event.send(Event::TaskComplete).await;
                                            break;
                                        }
                                        ResponseEvent::Error(message) => {
                                            let _ = tx_event.send(Event::Error { message }).await;
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx_event
                                    .send(Event::Error { message: e.to_string() })
                                    .await;
                            }
                        }
                    }
                    Op::Interrupt => {
                        // Minimal implementation: no-op for now
                    }
                    Op::ExecApproval { .. } => {
                        // Minimal placeholder: in full core this would resolve a pending approval
                    }
                    Op::PatchApproval { .. } => {
                        // Minimal placeholder
                    }
                    Op::Shutdown => {
                        let _ = tx_event.send(Event::ShutdownComplete).await;
                        break;
                    }
                }
            }
        });

        let inner = Arc::new(Inner { tx_submit, rx_event: Mutex::new(rx_event), conversation: Mutex::new(Vec::new()) });
        Ok(CodexSpawnOk { codex: Codex { inner } })
    }

    pub async fn submit(&self, op: Op) -> Result<()> {
        self.inner
            .tx_submit
            .send(op)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn next_event(&self) -> Option<Event> {
        let mut rx = self.inner.rx_event.lock().await;
        rx.recv().await
    }
}
