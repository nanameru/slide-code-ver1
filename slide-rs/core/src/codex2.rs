use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use std::time::Instant;
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::process::Stdio;
use tokio::sync::Mutex;

use crate::client::{ModelClient, ResponseEvent, OpenAiAdapter, StubClient};
use crate::openai_tools::{render_tools_instructions, ToolsConfig, ToolsConfigParams};
use crate::protocol::{ReasoningEffort, ReasoningSummary};
use protocol::protocol::InputItem;
use crate::tool_executor::{ToolExecutor, ToolCall};
use slide_chatgpt::client::{ChatGptClient, SlideRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    Approved,
    ApprovedForSession,
    Denied,
    Abort,
}

#[derive(Debug, Clone)]
pub enum Event {
    SessionConfigured {},
    TaskStarted,
    AgentMessageDelta {
        delta: String,
    },
    AgentMessage {
        message: String,
    },
    /// Explicit tool lifecycle events (codex-1 parity)
    ToolBegin {
        id: u64,
        kind: ToolKind,
        summary: String,
        cwd: PathBuf,
    },
    ToolOutput {
        id: u64,
        stream: ToolStream,
        line: String,
    },
    ToolEnd {
        id: u64,
        ok: bool,
        exit_code: Option<i32>,
        took_ms: u128,
    },
    ExecCommandBegin {
        command: Vec<String>,
        cwd: PathBuf,
    },
    ExecCommandEnd {
        exit_code: i32,
    },
    ApplyPatchApprovalRequest {
        id: String,
        changes: HashMap<PathBuf, String>,
        reason: Option<String>,
    },
    PatchApplyBegin {},
    PatchApplyEnd {
        success: bool,
    },
    TurnDiff {
        unified_diff: String,
    },
    TaskComplete,
    Error {
        message: String,
    },
    ShutdownComplete,
    ExecApprovalRequest {
        id: String,
        command: Vec<String>,
        cwd: PathBuf,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ToolKind {
    Exec,
    Mcp,
    Search,
    Info,
}

#[derive(Debug, Clone, Copy)]
pub enum ToolStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub enum Op {
    UserInput {
        text: String,
    },
    /// Rich user input including local image attachments (TUI parity with codex-1)
    UserInputItems {
        items: Vec<InputItem>,
    },
    /// Override parts of the persistent turn context for subsequent turns (model/effort/etc.)
    OverrideTurnContext {
        cwd: Option<PathBuf>,
        approval_policy: Option<crate::approval_manager::AskForApproval>,
        sandbox_policy: Option<crate::seatbelt::SandboxPolicy>,
        model: Option<String>,
        effort: Option<ReasoningEffort>,
        summary: Option<ReasoningSummary>,
    },
    Interrupt,
    ExecApproval {
        id: String,
        decision: ReviewDecision,
    },
    PatchApproval {
        id: String,
        decision: ReviewDecision,
    },
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
            let slide_client = ChatGptClient::new(api_key.clone());
            // Keep recent conversation messages (role, text). Oldest first.
            let mut convo: Vec<(String, String)> = Vec::new();
            // Monotonic identifier for tool lifecycles
            let mut next_tool_id: u64 = 1;
            // Persisted turn-overrides (minimal): applied to future turns
            let mut current_model: Option<String> = std::env::var("SLIDE_MODEL").ok();
            let mut current_effort: Option<ReasoningEffort> = None;
            let mut current_approval: crate::approval_manager::AskForApproval = crate::approval_manager::AskForApproval::default();
            let mut current_sandbox: crate::seatbelt::SandboxPolicy = crate::seatbelt::SandboxPolicy::default();

            // Build initial model client (OpenAI or Stub)
            let mut current_model_client: Arc<dyn ModelClient + Send + Sync> = if api_key.is_empty() {
                Arc::new(StubClient)
            } else if let Some(ref m) = current_model {
                Arc::new(OpenAiAdapter::new_with_model(api_key.clone(), m.clone()))
            } else {
                Arc::new(OpenAiAdapter::new(api_key.clone()))
            };
            // Handle to the currently running shell process (for interrupt)
            let running_child: Arc<tokio::sync::Mutex<Option<(u64, tokio::process::Child, Instant)>>> =
                Arc::new(tokio::sync::Mutex::new(None));
            while let Some(op) = rx_submit.recv().await {
                match op {
                    Op::OverrideTurnContext { cwd: _cwd, approval_policy, sandbox_policy, model, effort, summary: _ } => {
                        if model.is_some() { current_model = model; }
                        if effort.is_some() { current_effort = effort; }
                        if let Some(ap) = approval_policy { current_approval = ap; }
                        if let Some(sb) = sandbox_policy { current_sandbox = sb; }
                        // Rebuild model client if model changed
                        if !api_key.is_empty() {
                            current_model_client = if let Some(ref m) = current_model {
                                Arc::new(OpenAiAdapter::new_with_model(api_key.clone(), m.clone()))
                            } else {
                                Arc::new(OpenAiAdapter::new(api_key.clone()))
                            };
                        }
                        // No immediate event; next turn will use updated context annotations.
                    }
                    Op::UserInput { text } => {
                        let _ = tx_event.send(Event::TaskStarted).await;
                        if let Some(prompt) = text.strip_prefix("/slide ") {
                            match slide_client
                                .generate_slides(SlideRequest {
                                    prompt: prompt.to_string(),
                                    num_slides: 6,
                                    language: "ja".to_string(),
                                })
                                .await
                            {
                                Ok(resp) => {
                                    for line in resp.markdown.lines() {
                                        let delta = format!("{}\n", line);
                                        let _ =
                                            tx_event.send(Event::AgentMessageDelta { delta }).await;
                                    }
                                    let save_path = PathBuf::from("slides").join("draft.md");
                                    if let Some(parent) = save_path.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    if let Err(e) =
                                        std::fs::write(&save_path, resp.markdown.as_bytes())
                                    {
                                        let _ = tx_event
                                            .send(Event::Error {
                                                message: format!("failed to save slides: {e}"),
                                            })
                                            .await;
                                    } else {
                                        let _ = tx_event
                                            .send(Event::AgentMessage {
                                                message: format!(
                                                    "Saved to {}",
                                                    save_path.display()
                                                ),
                                            })
                                            .await;
                                    }
                                    let _ = tx_event.send(Event::TaskComplete).await;
                                }
                                Err(e) => {
                                    let _ = tx_event
                                        .send(Event::Error {
                                            message: e.to_string(),
                                        })
                                        .await;
                                }
                            }
                            continue;
                        }
                        // Prefix prompt with tool instructions so the model can propose edits/execs.
                        let approval_hint = std::env::var("SLIDE_APPROVAL_MODE").ok();
                        let model_family = crate::model_family::find_family_for_model("gpt-5").unwrap_or_else(|| crate::model_family::derive_default_model_family("gpt-5"));
                        let tools_cfg = ToolsConfig::new(&ToolsConfigParams {
                            model_family: &model_family,
                            experimental_unified_exec_tool: true,
                            include_plan_tool: true,
                            include_apply_patch_tool: true,
                            include_view_image_tool: false,
                            include_web_search_request: false,
                            use_streamable_shell_tool: true,
                            include_slides_tools: true,
                            approval_policy: crate::approval_manager::AskForApproval::default(),
                            sandbox_policy: crate::seatbelt::SandboxPolicy::default(),
                        });
                        let tool_instructions =
                            render_tools_instructions(&tools_cfg, approval_hint.as_deref());
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
                                let tag = if role == "assistant" {
                                    "Assistant"
                                } else {
                                    "User"
                                };
                                history_block.push_str(tag);
                                history_block.push_str(": ");
                                history_block.push_str(msg);
                                if !msg.ends_with('\n') {
                                    history_block.push('\n');
                                }
                            }
                        }
                        let mut context_note = String::new();
                        if let Some(m) = &current_model {
                            context_note.push_str(&format!("\n[Model: {}]", m));
                        }
                        if let Some(e) = current_effort {
                            context_note.push_str(&format!("\n[Reasoning: {}]", e.to_string()));
                        }
                        context_note.push_str(&format!("\n[Approval: {:?}]", current_approval));
                        context_note.push_str(&format!("\n[Sandbox: {:?}]", current_sandbox));
                        let composed = format!(
                            "{}{}{}\n\nUser: {}",
                            tool_instructions, context_note, history_block, text
                        );
                        // ツール実行エンジンを作成（ToolsConfigParamsから設定を取得）
                        let approval_policy = crate::approval_manager::AskForApproval::default();
                        let sandbox_policy = crate::seatbelt::SandboxPolicy::default();
                        let mut tool_executor = ToolExecutor::new(
                            approval_policy,
                            sandbox_policy,
                            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                            crate::config_types::ShellEnvironmentPolicy::default(),
                        );

                        let mut tool_executor = ToolExecutor::new(
                            crate::approval_manager::AskForApproval::default(),
                            crate::seatbelt::SandboxPolicy::default(),
                            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                            crate::config_types::ShellEnvironmentPolicy::default(),
                        );

                        let mut tool_executor = ToolExecutor::new(
                            crate::approval_manager::AskForApproval::default(),
                            crate::seatbelt::SandboxPolicy::default(),
                            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                            crate::config_types::ShellEnvironmentPolicy::default(),
                        );
                        match current_model_client.stream(composed).await {
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
                                            // AIレスポンス完了時にツール実行を処理
                                            match tool_executor.extract_tool_calls(&assembled_resp)
                                            {
                                                Ok(tool_calls) => {
                                                    if tool_calls.is_empty() {
                                                        if !assembled_resp.is_empty() {
                                                            convo.push((
                                                                "assistant".to_string(),
                                                                assembled_resp.clone(),
                                                            ));
                                                            if convo.len() > MAX_HISTORY_MESSAGES {
                                                                let drop = convo.len()
                                                                    - MAX_HISTORY_MESSAGES;
                                                                convo.drain(0..drop);
                                                            }
                                                        }
                                                    } else {
                                                        let mut appended = String::new();

                                                        for tool_call in tool_calls {
                                                            let id = next_tool_id; next_tool_id += 1;
                                                            let summary = tool_call.summary();
                                                            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                                                            let _ = tx_event.send(Event::ToolBegin { id, kind: ToolKind::Exec, summary: summary.clone(), cwd: cwd.clone() }).await;
                                                            // Fallback heuristic line for current TUI
                                                            let announce = format!("\n\n[Tool Execution]\n▶ {}", summary);
                                                            let _ = tx_event.send(Event::AgentMessageDelta { delta: announce.clone() }).await;
                                                            appended.push_str(&announce);

                                                            let started = Instant::now();
                                                            match tool_call {
                                                                ToolCall::Shell { command, working_dir, with_escalated_permissions, justification: _j, timeout_ms } => {
                                                                    // Streaming execution
                                                                    let mut cmd = Command::new(&command[0]);
                                                                    cmd.args(&command[1..]);
                                                                    let run_cwd = working_dir.unwrap_or_else(|| cwd.clone());
                                                                    cmd.current_dir(&run_cwd);
                                                                    cmd.stdin(Stdio::null());
                                                                    cmd.stdout(Stdio::piped());
                                                                    cmd.stderr(Stdio::piped());
                                                                    // minimal env policy (reuse existing creator)
                                                                    let env_map = crate::exec_env::create_env(&crate::config_types::ShellEnvironmentPolicy::default());
                                                                    cmd.env_clear();
                                                                    cmd.envs(env_map);
                                                                    let child = match cmd.spawn() {
                                                                        Ok(c) => c,
                                                                        Err(e) => {
                                                                            let err_text = e.to_string();
                                                                            let took_ms = started.elapsed().as_millis();
                                                                            let _ = tx_event.send(Event::ToolEnd { id, ok: false, exit_code: None, took_ms }).await;
                                                                            let block = format!("\n\n[Tool Execution Result]\nFailed: {}", err_text);
                                                                            let _ = tx_event.send(Event::AgentMessageDelta { delta: block }).await;
                                                                            break;
                                                                        }
                                                                    };
                                                                    // Save handle for possible interrupt
                                                                    {
                                                                        let mut g = running_child.lock().await;
                                                                        *g = Some((id, child, started));
                                                                    }
                                                                    // take stdout/stderr from the saved handle
                                                                    let out_opt = {
                                                                        let mut g = running_child.lock().await;
                                                                        g.as_mut().and_then(|(_, ch, _)| ch.stdout.take())
                                                                    };
                                                                    if let Some(out) = out_opt {
                                                                        let mut br = BufReader::new(out).lines();
                                                                        let tx = tx_event.clone();
                                                                        let idc = id;
                                                                        tokio::spawn(async move {
                                                                            while let Ok(Some(line)) = br.next_line().await {
                                                                                let _ = tx.send(Event::ToolOutput { id: idc, stream: ToolStream::Stdout, line }).await;
                                                                            }
                                                                        });
                                                                    }
                                                                    let err_opt = {
                                                                        let mut g = running_child.lock().await;
                                                                        g.as_mut().and_then(|(_, ch, _)| ch.stderr.take())
                                                                    };
                                                                    if let Some(err) = err_opt {
                                                                        let mut br = BufReader::new(err).lines();
                                                                        let tx = tx_event.clone();
                                                                        let idc = id;
                                                                        tokio::spawn(async move {
                                                                            while let Ok(Some(line)) = br.next_line().await {
                                                                                let _ = tx.send(Event::ToolOutput { id: idc, stream: ToolStream::Stderr, line }).await;
                                                                            }
                                                                        });
                                                                    }
                                                                    // Wait for completion (or detect that it was killed)
                                                                    let status_opt = {
                                                                        let mut g = running_child.lock().await;
                                                                        if let Some((_, ch, _)) = g.as_mut() { Some(ch.wait().await) } else { None }
                                                                    };
                                                                    if let Some(wait_res) = status_opt {
                                                                        let status = match wait_res {
                                                                            Ok(s) => s,
                                                                            Err(e) => {
                                                                                let err_text = e.to_string();
                                                                                let took_ms = {
                                                                                    let mut g = running_child.lock().await;
                                                                                    g.take().map(|(_, _, st)| st.elapsed().as_millis()).unwrap_or_else(|| started.elapsed().as_millis())
                                                                                };
                                                                                let _ = tx_event.send(Event::ToolEnd { id, ok: false, exit_code: None, took_ms }).await;
                                                                                let block = format!("\n\n[Tool Execution Result]\nFailed: {}", err_text);
                                                                                let _ = tx_event.send(Event::AgentMessageDelta { delta: block }).await;
                                                                                break;
                                                                            }
                                                                        };
                                                                        let code = status.code();
                                                                        let took_ms = {
                                                                            let mut g = running_child.lock().await;
                                                                            g.take().map(|(_, _, st)| st.elapsed().as_millis()).unwrap_or_else(|| started.elapsed().as_millis())
                                                                        };
                                                                        let _ = tx_event.send(Event::ToolEnd { id, ok: status.success(), exit_code: code, took_ms }).await;
                                                                        // keep fallback block minimal (no duplication of streamed lines)
                                                                        let fallback = format!("\n\n[Tool Execution Result]\nexit {:?}", code);
                                                                        let _ = tx_event.send(Event::AgentMessageDelta { delta: fallback.clone() }).await;
                                                                        appended.push_str(&fallback);
                                                                    } else {
                                                                        // Already killed (interrupt)
                                                                        let took_ms = {
                                                                            let mut g = running_child.lock().await;
                                                                            g.take().map(|(_, _, st)| st.elapsed().as_millis()).unwrap_or_else(|| started.elapsed().as_millis())
                                                                        };
                                                                        let _ = tx_event.send(Event::ToolEnd { id, ok: false, exit_code: None, took_ms }).await;
                                                                        continue;
                                                                    }
                                                                }
                                                                other_call => {
                                                                    match tool_executor.execute_tool_call(other_call).await {
                                                                     Ok(exec_output) => {
                                                                         for ln in exec_output.lines() {
                                                                             let _ = tx_event.send(Event::ToolOutput { id, stream: ToolStream::Stdout, line: ln.to_string() }).await;
                                                                         }
                                                                         let took_ms = started.elapsed().as_millis();
                                                                         let _ = tx_event.send(Event::ToolEnd { id, ok: true, exit_code: None, took_ms }).await;
                                                                         // Fallback block
                                                                         let block = format!("\n\n[Tool Execution Result]\n{}", exec_output);
                                                                         let _ = tx_event.send(Event::AgentMessageDelta { delta: block.clone() }).await;
                                                                         appended.push_str(&block);
                                                                }
                                                                Err(err) => {
                                                                    let err_text = err.to_string();
                                                                         let took_ms = started.elapsed().as_millis();
                                                                         let _ = tx_event.send(Event::ToolEnd { id, ok: false, exit_code: None, took_ms }).await;
                                                                         let block = format!("\n\n[Tool Execution Result]\nFailed: {}", err_text);
                                                                         let _ = tx_event.send(Event::AgentMessageDelta { delta: block.clone() }).await;
                                                                         let _ = tx_event.send(Event::Error { message: format!("Tool execution failed: {}", err_text) }).await;
                                                                    appended.push_str(&block);
                                                                    break;
                                                                     }
                                                                }
                                                            }
                                                        }

                                                        let enriched = format!(
                                                            "{}{}",
                                                            assembled_resp, appended
                                                        );

                                                        if !enriched.is_empty() {
                                                            convo.push((
                                                                "assistant".to_string(),
                                                                enriched,
                                                            ));
                                                            if convo.len() > MAX_HISTORY_MESSAGES {
                                                                let drop = convo.len()
                                                                    - MAX_HISTORY_MESSAGES;
                                                                convo.drain(0..drop);
                                                            }
                                                        }
                                                }
                                            }
                                            }
                                            Err(e) => {
                                                    let _ = tx_event
                                                        .send(Event::Error {
                                                            message: format!(
                                                                "Tool parsing failed: {}",
                                                                e
                                                            ),
                                                        })
                                                        .await;

                                                    if !assembled_resp.is_empty() {
                                                        convo.push((
                                                            "assistant".to_string(),
                                                            assembled_resp.clone(),
                                                        ));
                                                        if convo.len() > MAX_HISTORY_MESSAGES {
                                                            let drop =
                                                                convo.len() - MAX_HISTORY_MESSAGES;
                                                            convo.drain(0..drop);
                                                        }
                                                    }
                                                }
                                            }
                                            let _ = tx_event.send(Event::TaskComplete).await;
                                            break;
                                        }
                                        ResponseEvent::Error(message) => {
                                            let _ = tx_event.send(Event::Error { message }).await;
                                            break;
                                        }
                                        // 新しいResponseEventの処理
                                        ResponseEvent::Created => {
                                            // Created イベントは特別な処理は不要
                                        }
                                        ResponseEvent::OutputItemDone(_item) => {
                                            // OutputItemDone は現在の実装では未対応
                                            // 将来的にはhandle_response_itemで処理
                                        }
                                        ResponseEvent::CompletedWithDetails { response_id: _, token_usage: _ } => {
                                            // 詳細な完了情報付きの処理（現在は基本のCompletedと同じ）
                                            break;
                                        }
                                        ResponseEvent::OutputTextDelta(delta) => {
                                            // OutputTextDelta は TextDelta と同じ処理
                                            assembled_resp.push_str(&delta);
                                            let _ = tx_event
                                                .send(Event::AgentMessageDelta { delta })
                                                .await;
                                        }
                                        ResponseEvent::ReasoningSummaryDelta(delta) => {
                                            // 推論サマリーのデルタ処理
                                            let _ = tx_event
                                                .send(Event::AgentMessageDelta { delta })
                                                .await;
                                        }
                                        ResponseEvent::ReasoningContentDelta(delta) => {
                                            // 推論コンテンツのデルタ処理
                                            let _ = tx_event
                                                .send(Event::AgentMessageDelta { delta })
                                                .await;
                                        }
                                        ResponseEvent::WebSearchCallBegin { call_id: _ } => {
                                            // Web検索開始イベント（現在は未対応）
                                        }
                                        ResponseEvent::RateLimits(_snapshot) => {
                                            // レート制限情報の処理（現在は未対応）
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx_event
                                    .send(Event::Error {
                                        message: e.to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                    Op::UserInputItems { items } => {
                        let _ = tx_event.send(Event::TaskStarted).await;
                        // Build a prompt similar to above; for parity we include text and note images
                        let approval_hint = std::env::var("SLIDE_APPROVAL_MODE").ok();
                        let model_family = crate::model_family::find_family_for_model("gpt-5").unwrap_or_else(|| crate::model_family::derive_default_model_family("gpt-5"));
                        let tools_cfg = ToolsConfig::new(&ToolsConfigParams {
                            model_family: &model_family,
                            experimental_unified_exec_tool: true,
                            include_plan_tool: true,
                            include_apply_patch_tool: true,
                            include_view_image_tool: true,
                            include_web_search_request: false,
                            use_streamable_shell_tool: true,
                            include_slides_tools: true,
                            approval_policy: crate::approval_manager::AskForApproval::default(),
                            sandbox_policy: crate::seatbelt::SandboxPolicy::default(),
                        });
                        let tool_instructions =
                            render_tools_instructions(&tools_cfg, approval_hint.as_deref());

                        let text_part = items.iter().find_map(|it| if let InputItem::Text { text } = it { Some(text.clone()) } else { None }).unwrap_or_default();
                        let img_count = items.iter().filter(|it| matches!(it, InputItem::LocalImage { .. } | InputItem::Image { .. })).count();

                        convo.push(("user".to_string(), text_part.clone()));
                        const MAX_HISTORY_MESSAGES: usize = 12;
                        if convo.len() > MAX_HISTORY_MESSAGES {
                            let drop = convo.len() - MAX_HISTORY_MESSAGES;
                            convo.drain(0..drop);
                        }
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
                        let mut context_note = String::new();
                        if let Some(m) = &current_model { context_note.push_str(&format!("\n[Model: {}]", m)); }
                        if let Some(e) = current_effort { context_note.push_str(&format!("\n[Reasoning: {}]", e.to_string())); }
                        context_note.push_str(&format!("\n[Approval: {:?}]", current_approval));
                        context_note.push_str(&format!("\n[Sandbox: {:?}]", current_sandbox));
                        let mut composed = format!("{}{}{}\n\nUser: {}", tool_instructions, context_note, history_block, text_part);
                        if img_count > 0 { composed.push_str(&format!("\n\n[{} image attachment(s)]", img_count)); }

                        let mut tool_executor = ToolExecutor::new(
                            crate::approval_manager::AskForApproval::default(),
                            crate::seatbelt::SandboxPolicy::default(),
                            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                            crate::config_types::ShellEnvironmentPolicy::default(),
                        );
                        match current_model_client.stream(composed).await {
                            Ok(mut rx) => {
                                let mut assembled_resp = String::new();
                                while let Some(ev) = rx.recv().await {
                                    match ev {
                                        ResponseEvent::TextDelta(delta) => {
                                            assembled_resp.push_str(&delta);
                                            let _ = tx_event.send(Event::AgentMessageDelta { delta }).await;
                                        }
                                        ResponseEvent::Completed => {
                                            // Process tools as usual
                                            // (same as above branch)
                                            match tool_executor.extract_tool_calls(&assembled_resp) {
                                                Ok(tool_calls) => {
                                                    if tool_calls.is_empty() {
                                                        if !assembled_resp.is_empty() {
                                                            convo.push(("assistant".to_string(), assembled_resp.clone()));
                                                            if convo.len() > MAX_HISTORY_MESSAGES {
                                                                let drop = convo.len() - MAX_HISTORY_MESSAGES;
                                                                convo.drain(0..drop);
                                                            }
                                                        }
                                                    } else {
                                                        let mut appended = String::new();
                                                        for tool_call in tool_calls {
                                                            let announce = format!("\n\n[Tool Execution]\n▶ {}", tool_call.summary());
                                                            let _ = tx_event.send(Event::AgentMessageDelta { delta: announce.clone() }).await;
                                                            appended.push_str(&announce);
                                                            match tool_executor.execute_tool_call(tool_call).await {
                                                                Ok(exec_output) => {
                                                                    let block = format!("\n\n[Tool Execution Result]\n{}", exec_output);
                                                                    let _ = tx_event.send(Event::AgentMessageDelta { delta: block.clone() }).await;
                                                                    appended.push_str(&block);
                                                                }
                                                                Err(err) => {
                                                                    let err_text = err.to_string();
                                                                    let block = format!("\n\n[Tool Execution Result]\nFailed: {}", err_text);
                                                                    let _ = tx_event.send(Event::AgentMessageDelta { delta: block.clone() }).await;
                                                                    let _ = tx_event.send(Event::Error { message: format!("Tool execution failed: {}", err_text) }).await;
                                                                    appended.push_str(&block);
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                        let enriched = format!("{}{}", assembled_resp, appended);
                                                        if !enriched.is_empty() {
                                                            convo.push(("assistant".to_string(), enriched));
                                                            if convo.len() > MAX_HISTORY_MESSAGES {
                                                                let drop = convo.len() - MAX_HISTORY_MESSAGES;
                                                                convo.drain(0..drop);
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    let _ = tx_event.send(Event::Error { message: format!("Tool parsing failed: {}", e) }).await;
                                                    if !assembled_resp.is_empty() {
                                                        convo.push(("assistant".to_string(), assembled_resp.clone()));
                                                        if convo.len() > MAX_HISTORY_MESSAGES {
                                                            let drop = convo.len() - MAX_HISTORY_MESSAGES;
                                                            convo.drain(0..drop);
                                                        }
                                                    }
                                                }
                                            }
                                            let _ = tx_event.send(Event::TaskComplete).await;
                                            break;
                                        }
                                        ResponseEvent::Error(message) => {
                                            let _ = tx_event.send(Event::Error { message }).await;
                                            break;
                                        }
                                        // 新しいResponseEventの処理
                                        ResponseEvent::Created => {
                                            // Created イベントは特別な処理は不要
                                        }
                                        ResponseEvent::OutputItemDone(_item) => {
                                            // OutputItemDone は現在の実装では未対応
                                            // 将来的にはhandle_response_itemで処理
                                        }
                                        ResponseEvent::CompletedWithDetails { response_id: _, token_usage: _ } => {
                                            // 詳細な完了情報付きの処理（現在は基本のCompletedと同じ）
                                            break;
                                        }
                                        ResponseEvent::OutputTextDelta(delta) => {
                                            // OutputTextDelta は TextDelta と同じ処理
                                            assembled_resp.push_str(&delta);
                                            let _ = tx_event
                                                .send(Event::AgentMessageDelta { delta })
                                                .await;
                                        }
                                        ResponseEvent::ReasoningSummaryDelta(delta) => {
                                            // 推論サマリーのデルタ処理
                                            let _ = tx_event
                                                .send(Event::AgentMessageDelta { delta })
                                                .await;
                                        }
                                        ResponseEvent::ReasoningContentDelta(delta) => {
                                            // 推論コンテンツのデルタ処理
                                            let _ = tx_event
                                                .send(Event::AgentMessageDelta { delta })
                                                .await;
                                        }
                                        ResponseEvent::WebSearchCallBegin { call_id: _ } => {
                                            // Web検索開始イベント（現在は未対応）
                                        }
                                        ResponseEvent::RateLimits(_snapshot) => {
                                            // レート制限情報の処理（現在は未対応）
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx_event.send(Event::Error { message: e.to_string() }).await;
                            }
                        }
                    }
                    Op::Interrupt => {
                        let mut g = running_child.lock().await;
                        if let Some((rid, mut child, st)) = g.take() {
                            let _ = child.kill().await;
                            let took_ms = st.elapsed().as_millis();
                            let _ = tx_event.send(Event::ToolEnd { id: rid, ok: false, exit_code: None, took_ms }).await;
                        }
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

        let inner = Arc::new(Inner {
            tx_submit,
            rx_event: Mutex::new(rx_event),
            conversation: Mutex::new(Vec::new()),
        });
        Ok(CodexSpawnOk {
            codex: Codex { inner },
        })
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
