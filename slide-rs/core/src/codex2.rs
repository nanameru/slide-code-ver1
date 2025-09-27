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
use crate::mcp_connection_manager::McpConnectionManager;
use crate::mcp_tool_call::handle_mcp_tool_call;
use crate::conversation_history::{ResponseItem, ResponseInputItem};
use crate::event_mapping::map_response_item_to_event_messages;
use slide_chatgpt::client::{ChatGptClient, SlideRequest};
use regex;
use uuid;

// MEE-24: 処理済みレスポンスアイテム
#[derive(Debug, Clone)]
struct ProcessedResponseItem {
    item: ResponseItem,
    response: Option<ResponseInputItem>,
}

// MEE-24: ターン実行結果
#[derive(Debug, Clone)]
struct TurnRunResult {
    processed_items: Vec<ProcessedResponseItem>,
    token_usage: Option<protocol::protocol::TokenUsage>,
}

// MCP関数呼び出しの構造体
#[derive(Debug, Clone)]
struct McpFunctionCall {
    call_id: String,
    server: String,
    tool_name: String,
    arguments: String,
}

// MCP関数呼び出しを検出する関数
fn extract_mcp_function_call(text: &str) -> Option<McpFunctionCall> {
    // 簡易的な正規表現でMCP関数呼び出しを検出
    // 実際の実装では、より堅牢なパーサーを使用することを推奨
    let re = regex::Regex::new(r#"<function_calls>\s*<invoke\s+name="([^"]+)"[^>]*>\s*<parameter\s+name="([^"]+)">([^<]*)</parameter>\s*</invoke>\s*</function_calls>"#).ok()?;
    
    if let Some(captures) = re.captures(text) {
        let full_name = captures.get(1)?.as_str();
        let arguments = captures.get(3)?.as_str();
        
        // MCP tool nameの形式: "server/tool" を解析
        if let Some((server, tool_name)) = full_name.split_once('/') {
            return Some(McpFunctionCall {
                call_id: format!("call_{}", uuid::Uuid::new_v4()),
                server: server.to_string(),
                tool_name: tool_name.to_string(),
                arguments: arguments.to_string(),
            });
        }
    }
    
    None
}

// MEE-24: missing_calls処理関数
fn process_missing_calls(
    conversation_history: &[(String, String)],
) -> Vec<ResponseItem> {
    // 簡略化: 実際の実装では会話履歴から未完了のツール呼び出しを検出
    // 現在は空のベクターを返す
    Vec::new()
}

// MEE-24: レスポンスアイテム処理関数
async fn handle_response_item(
    mcp_manager: &McpConnectionManager,
    tool_executor: &mut ToolExecutor,
    tx_event: &mpsc::Sender<Event>,
    sub_id: &str,
    item: ResponseItem,
) -> Option<ResponseInputItem> {
    match item {
        ResponseItem::Message { .. } => {
            // メッセージイベントをUIに送信
            let events = map_response_item_to_event_messages(&item, false);
            for event_msg in events {
                let event = Event::AgentMessage {
                    message: format!("{:?}", event_msg), // 簡略化
                };
                let _ = tx_event.send(event).await;
            }
            None
        }
        ResponseItem::FunctionCall { name, arguments, call_id, .. } => {
            // 関数呼び出し処理
            if let Some((server, tool_name)) = mcp_manager.parse_tool_name(&name) {
                // MCP関数呼び出し
                let result = handle_mcp_tool_call(
                    mcp_manager,
                    sub_id,
                    call_id.clone(),
                    server,
                    tool_name,
                    arguments,
                    None,
                ).await;
                Some(result)
            } else {
                // 通常のツール実行
                match tool_executor.execute_function_call(&name, &arguments).await {
                    Ok(output) => Some(ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: crate::conversation_history::FunctionCallOutputPayload {
                            content: output,
                            success: Some(true),
                        },
                    }),
                    Err(e) => Some(ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: crate::conversation_history::FunctionCallOutputPayload {
                            content: format!("Error: {}", e),
                            success: Some(false),
                        },
                    }),
                }
            }
        }
        ResponseItem::Reasoning { .. } | ResponseItem::WebSearchCall { .. } => {
            // 推論やWeb検索のイベントをUIに送信
            let events = map_response_item_to_event_messages(&item, false);
            for event_msg in events {
                let event = Event::AgentMessage {
                    message: format!("{:?}", event_msg), // 簡略化
                };
                let _ = tx_event.send(event).await;
            }
            None
        }
        _ => None,
    }
}

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
            
            // Initialize MCP Connection Manager
            let mcp_manager = McpConnectionManager::default();
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
                                            // まず、MCPツール呼び出しをチェック
                                            if let Some(mcp_call) = extract_mcp_function_call(&assembled_resp) {
                                                // MCP関数呼び出しを処理
                                                let mcp_result = handle_mcp_tool_call(
                                                    &mcp_manager,
                                                    "user_input",
                                                    mcp_call.call_id.clone(),
                                                    mcp_call.server,
                                                    mcp_call.tool_name,
                                                    mcp_call.arguments,
                                                    None, // timeout
                                                ).await;
                                                
                                                // MCP結果をconversationに追加
                                                match mcp_result {
                                                    crate::conversation_history::ResponseInputItem::McpToolCallOutput { call_id: _, result } => {
                                                        let result_text = match result {
                                                            Ok(call_result) => format!("MCP Tool Result: {}", serde_json::to_string_pretty(&call_result).unwrap_or_else(|_| "Success".to_string())),
                                                            Err(e) => format!("MCP Tool Error: {}", e),
                                                        };
                                                        convo.push(("assistant".to_string(), result_text));
                                                    }
                                                    _ => {
                                                        // 他のResponseInputItemタイプの場合
                                                        convo.push(("assistant".to_string(), "MCP tool executed".to_string()));
                                                    }
                                                }
                                            } else {
                                                // 通常のツール実行処理
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
                                        ResponseEvent::OutputItemDone(item) => {
                                            // MEE-24: OutputItemDone処理
                                            if let Some(_response) = handle_response_item(
                                                &mcp_manager,
                                                &mut tool_executor,
                                                &tx_event,
                                                "user_input",
                                                item,
                                            ).await {
                                                // レスポンスがある場合は会話履歴に追加
                                                // 簡略化: 実際の実装では適切な会話管理が必要
                                            }
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
                                        ResponseEvent::ReasoningSummaryPartAdded => {
                                            // 推論サマリー区切り処理（現在は未対応）
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
                                        ResponseEvent::OutputItemDone(item) => {
                                            // MEE-24: OutputItemDone処理
                                            if let Some(_response) = handle_response_item(
                                                &mcp_manager,
                                                &mut tool_executor,
                                                &tx_event,
                                                "user_input",
                                                item,
                                            ).await {
                                                // レスポンスがある場合は会話履歴に追加
                                                // 簡略化: 実際の実装では適切な会話管理が必要
                                            }
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
                                        ResponseEvent::ReasoningSummaryPartAdded => {
                                            // 推論サマリー区切り処理（現在は未対応）
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
