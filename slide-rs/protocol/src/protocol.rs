//! Defines the protocol for a Codex session between a client and an agent.
//!
//! Uses a SQ (Submission Queue) / EQ (Event Queue) pattern to asynchronously communicate
//! between user and agent.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use crate::custom_prompts::CustomPrompt;
use mcp_types::CallToolResult;
use mcp_types::Tool as McpTool;
use serde::Deserialize;
use serde::Serialize;
use serde_bytes::ByteBuf;
use strum_macros::Display;
use ts_rs::TS;
use uuid::Uuid;

use crate::config_types::ReasoningEffort as ReasoningEffortConfig;
use crate::config_types::ReasoningSummary as ReasoningSummaryConfig;
use crate::message_history::HistoryEntry;
use crate::models::ResponseItem;
use crate::parse_command::ParsedCommand;
use crate::plan_tool::UpdatePlanArgs;

/// Submission Queue Entry - requests from user
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Submission {
    /// Unique id for this Submission to correlate with Events
    pub id: String,
    /// Payload
    pub op: Op,
}

/// Submission operation
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
#[non_exhaustive]
pub enum Op {
    /// Abort current task.
    /// This server sends [`EventMsg::TurnAborted`] in response.
    Interrupt,

    /// Input from the user
    UserInput { items: Vec<InputItem> },

    /// Similar to [`Op::UserInput`], but contains additional context required
    /// for a turn of a [`crate::codex_conversation::CodexConversation`].
    UserTurn {
        items: Vec<InputItem>,
        cwd: PathBuf,
        approval_policy: AskForApproval,
        sandbox_policy: SandboxPolicy,
        model: String,
        effort: ReasoningEffortConfig,
        summary: ReasoningSummaryConfig,
    },

    /// Override parts of the persistent turn context for subsequent turns.
    OverrideTurnContext {
        #[serde(skip_serializing_if = "Option::is_none")] cwd: Option<PathBuf>,
        #[serde(skip_serializing_if = "Option::is_none")] approval_policy: Option<AskForApproval>,
        #[serde(skip_serializing_if = "Option::is_none")] sandbox_policy: Option<SandboxPolicy>,
        #[serde(skip_serializing_if = "Option::is_none")] model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")] effort: Option<ReasoningEffortConfig>,
        #[serde(skip_serializing_if = "Option::is_none")] summary: Option<ReasoningSummaryConfig>,
    },

    /// Approvals
    ExecApproval { id: String, decision: ReviewDecision },
    PatchApproval { id: String, decision: ReviewDecision },

    /// History
    AddToHistory { text: String },
    GetHistoryEntryRequest { offset: usize, log_id: u64 },
    GetHistory,

    /// Listings
    ListMcpTools,
    ListCustomPrompts,

    /// Utilities
    Compact,
    Shutdown,
}

/// Determines the conditions under which the user is consulted to approve
/// running the command proposed by Codex.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display, TS)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum AskForApproval { UnlessTrusted, OnFailure, #[default] OnRequest, Never }

/// Determines execution restrictions for model shell commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display, TS)]
#[strum(serialize_all = "kebab-case")]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum SandboxPolicy {
    #[serde(rename = "danger-full-access")] DangerFullAccess,
    #[serde(rename = "read-only")] ReadOnly,
    #[serde(rename = "workspace-write")] WorkspaceWrite {
        #[serde(default, skip_serializing_if = "Vec::is_empty")] writable_roots: Vec<PathBuf>,
        #[serde(default)] network_access: bool,
        #[serde(default)] exclude_tmpdir_env_var: bool,
        #[serde(default)] exclude_slash_tmp: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableRoot { pub root: PathBuf, pub read_only_subpaths: Vec<PathBuf> }

impl WritableRoot { pub fn is_path_writable(&self, path: &Path) -> bool { path.starts_with(&self.root) && !self.read_only_subpaths.iter().any(|p| path.starts_with(p)) } }

impl FromStr for SandboxPolicy { type Err = serde_json::Error; fn from_str(s: &str) -> Result<Self, Self::Err> { serde_json::from_str(s) } }

impl SandboxPolicy {
    pub fn new_read_only_policy() -> Self { SandboxPolicy::ReadOnly }
    pub fn new_workspace_write_policy() -> Self { SandboxPolicy::WorkspaceWrite { writable_roots: vec![], network_access: false, exclude_tmpdir_env_var: false, exclude_slash_tmp: false } }
    pub fn has_full_disk_read_access(&self) -> bool { true }
    pub fn has_full_disk_write_access(&self) -> bool { matches!(self, SandboxPolicy::DangerFullAccess) }
    pub fn has_full_network_access(&self) -> bool { matches!(self, SandboxPolicy::DangerFullAccess | SandboxPolicy::WorkspaceWrite { network_access: true, .. }) }
    pub fn get_writable_roots_with_cwd(&self, cwd: &Path) -> Vec<WritableRoot> {
        match self {
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ReadOnly => Vec::new(),
            SandboxPolicy::WorkspaceWrite { writable_roots, exclude_tmpdir_env_var, exclude_slash_tmp, .. } => {
                let mut roots = writable_roots.clone();
                roots.push(cwd.to_path_buf());
                if cfg!(unix) && !exclude_slash_tmp { let p = PathBuf::from("/tmp"); if p.is_dir() { roots.push(p); } }
                if !exclude_tmpdir_env_var { if let Some(tmpdir) = std::env::var_os("TMPDIR") { if !tmpdir.is_empty() { roots.push(PathBuf::from(tmpdir)); } } }
                roots.into_iter().map(|r| { let mut ro = Vec::new(); let git = r.join(".git"); if git.is_dir() { ro.push(git); } WritableRoot { root: r, read_only_subpaths: ro } }).collect()
            }
        }
    }
}

/// User input
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem { Text { text: String }, Image { image_url: String }, LocalImage { path: PathBuf } }

/// Event Queue Entry - events from agent
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Event { pub id: String, pub msg: EventMsg }

/// Response event from the agent
#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(tag = "type", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EventMsg {
    Error(ErrorEvent), TaskStarted(TaskStartedEvent), TaskComplete(TaskCompleteEvent), TokenCount(TokenUsage),
    AgentMessage(AgentMessageEvent), AgentMessageDelta(AgentMessageDeltaEvent),
    AgentReasoning(AgentReasoningEvent), AgentReasoningDelta(AgentReasoningDeltaEvent),
    AgentReasoningRawContent(AgentReasoningRawContentEvent), AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent), AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent),
    SessionConfigured(SessionConfiguredEvent),
    McpToolCallBegin(McpToolCallBeginEvent), McpToolCallEnd(McpToolCallEndEvent),
    WebSearchBegin(WebSearchBeginEvent), WebSearchEnd(WebSearchEndEvent),
    ExecCommandBegin(ExecCommandBeginEvent), ExecCommandOutputDelta(ExecCommandOutputDeltaEvent), ExecCommandEnd(ExecCommandEndEvent),
    ExecApprovalRequest(ExecApprovalRequestEvent), ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent),
    BackgroundEvent(BackgroundEventEvent), StreamError(StreamErrorEvent),
    PatchApplyBegin(PatchApplyBeginEvent), PatchApplyEnd(PatchApplyEndEvent),
    TurnDiff(TurnDiffEvent),
    GetHistoryEntryResponse(GetHistoryEntryResponseEvent), McpListToolsResponse(McpListToolsResponseEvent), ListCustomPromptsResponse(ListCustomPromptsResponseEvent),
    PlanUpdate(UpdatePlanArgs), TurnAborted(TurnAbortedEvent), ShutdownComplete, ConversationHistory(ConversationHistoryResponseEvent),
}

// Payloads
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct ErrorEvent { pub message: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct TaskCompleteEvent { pub last_agent_message: Option<String> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct TaskStartedEvent { pub model_context_window: Option<u64> }
#[derive(Debug, Clone, Deserialize, Serialize, Default)] pub struct TokenUsage { pub input_tokens: u64, pub cached_input_tokens: Option<u64>, pub output_tokens: u64, pub reasoning_output_tokens: Option<u64>, pub total_tokens: u64 }

impl TokenUsage { pub fn is_zero(&self) -> bool { self.total_tokens == 0 } pub fn cached_input(&self) -> u64 { self.cached_input_tokens.unwrap_or(0) } pub fn non_cached_input(&self) -> u64 { self.input_tokens.saturating_sub(self.cached_input()) } pub fn blended_total(&self) -> u64 { self.non_cached_input() + self.output_tokens } pub fn tokens_in_context_window(&self) -> u64 { self.total_tokens.saturating_sub(self.reasoning_output_tokens.unwrap_or(0)) } pub fn percent_of_context_window_remaining(&self, context_window: u64, baseline_used_tokens: u64) -> u8 { if context_window <= baseline_used_tokens { return 0; } let effective_window = context_window - baseline_used_tokens; let used = self.tokens_in_context_window().saturating_sub(baseline_used_tokens); let remaining = effective_window.saturating_sub(used); ((remaining as f32 / effective_window as f32) * 100.0).clamp(0.0, 100.0) as u8 } }

#[derive(Debug, Clone, Deserialize, Serialize)] pub struct FinalOutput { pub token_usage: TokenUsage }
impl From<TokenUsage> for FinalOutput { fn from(token_usage: TokenUsage) -> Self { Self { token_usage } } }
impl fmt::Display for FinalOutput { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { let t = &self.token_usage; write!(f, "Token usage: total={} input={}{} output={}{}", t.blended_total(), t.non_cached_input(), if t.cached_input() > 0 { format!(" (+ {} cached)", t.cached_input()) } else { String::new() }, t.output_tokens, t.reasoning_output_tokens.map(|r| format!(" (reasoning {r})")).unwrap_or_default()) } }

#[derive(Debug, Clone, Deserialize, Serialize)] pub struct AgentMessageEvent { pub message: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct AgentMessageDeltaEvent { pub delta: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct AgentReasoningEvent { pub text: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct AgentReasoningRawContentEvent { pub text: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct AgentReasoningRawContentDeltaEvent { pub delta: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct AgentReasoningSectionBreakEvent {}
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct AgentReasoningDeltaEvent { pub delta: String }

#[derive(Debug, Clone, Deserialize, Serialize)] pub struct McpInvocation { pub server: String, pub tool: String, pub arguments: Option<serde_json::Value> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct McpToolCallBeginEvent { pub call_id: String, pub invocation: McpInvocation }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct McpToolCallEndEvent { pub call_id: String, pub invocation: McpInvocation, pub duration: Duration, pub result: Result<CallToolResult, String> }
impl McpToolCallEndEvent { pub fn is_success(&self) -> bool { match &self.result { Ok(r) => !r.is_error.unwrap_or(false), Err(_) => false } } }

#[derive(Debug, Clone, Deserialize, Serialize)] pub struct WebSearchBeginEvent { pub call_id: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct WebSearchEndEvent { pub call_id: String, pub query: String }

#[derive(Debug, Clone, Deserialize, Serialize)] pub struct ConversationHistoryResponseEvent { pub conversation_id: Uuid, pub entries: Vec<ResponseItem> }

#[derive(Debug, Clone, Deserialize, Serialize)] pub struct ExecCommandBeginEvent { pub call_id: String, pub command: Vec<String>, pub cwd: PathBuf, pub parsed_cmd: Vec<ParsedCommand> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct ExecCommandEndEvent { pub call_id: String, pub stdout: String, pub stderr: String, #[serde(default)] pub aggregated_output: String, pub exit_code: i32, pub duration: Duration, pub formatted_output: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecOutputStream { Stdout, Stderr }

#[derive(Debug, Clone, Deserialize, Serialize)] pub struct ExecCommandOutputDeltaEvent { pub call_id: String, pub stream: ExecOutputStream, #[serde(with = "serde_bytes")] pub chunk: ByteBuf }

#[derive(Debug, Clone, Deserialize, Serialize)] pub struct ExecApprovalRequestEvent { pub call_id: String, pub command: Vec<String>, pub cwd: PathBuf, #[serde(skip_serializing_if = "Option::is_none")] pub reason: Option<String> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct ApplyPatchApprovalRequestEvent { pub call_id: String, pub changes: HashMap<PathBuf, FileChange>, #[serde(skip_serializing_if = "Option::is_none")] pub reason: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] pub grant_root: Option<PathBuf> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct BackgroundEventEvent { pub message: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct StreamErrorEvent { pub message: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct PatchApplyBeginEvent { pub call_id: String, pub auto_approved: bool, pub changes: HashMap<PathBuf, FileChange> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct PatchApplyEndEvent { pub call_id: String, pub stdout: String, pub stderr: String, pub success: bool }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct TurnDiffEvent { pub unified_diff: String }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct GetHistoryEntryResponseEvent { pub offset: usize, pub log_id: u64, #[serde(skip_serializing_if = "Option::is_none")] pub entry: Option<HistoryEntry> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct McpListToolsResponseEvent { pub tools: std::collections::HashMap<String, McpTool> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct ListCustomPromptsResponseEvent { pub custom_prompts: Vec<CustomPrompt> }
#[derive(Debug, Default, Clone, Deserialize, Serialize)] pub struct SessionConfiguredEvent { pub session_id: Uuid, pub model: String, pub history_log_id: u64, pub history_entry_count: usize }
#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, TS)] #[serde(rename_all = "snake_case")] pub enum ReviewDecision { Approved, ApprovedForSession, #[default] Denied, Abort }
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)] #[serde(rename_all = "snake_case")] pub enum FileChange { Add { content: String }, Delete, Update { unified_diff: String, move_path: Option<PathBuf> } }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct Chunk { pub orig_index: u32, pub deleted_lines: Vec<String>, pub inserted_lines: Vec<String> }
#[derive(Debug, Clone, Deserialize, Serialize)] pub struct TurnAbortedEvent { pub reason: TurnAbortReason }
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)] #[serde(rename_all = "snake_case")] pub enum TurnAbortReason { Interrupted, Replaced }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serialize_event() {
        let session_id: Uuid = uuid::uuid!("67e55044-10b1-426f-9247-bb680e5fe0c8");
        let event = Event { id: "1234".to_string(), msg: EventMsg::SessionConfigured(SessionConfiguredEvent { session_id, model: "codex-mini-latest".to_string(), history_log_id: 0, history_entry_count: 0 }) };
        let serialized = serde_json::to_string(&event).unwrap();
        assert_eq!(serialized, r#"{"id":"1234","msg":{"type":"session_configured","session_id":"67e55044-10b1-426f-9247-bb680e5fe0c8","model":"codex-mini-latest","history_log_id":0,"history_entry_count":0}}"#);
    }
}


