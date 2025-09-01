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

use codex_protocol::custom_prompts::CustomPrompt;
use codex_protocol::message_history::HistoryEntry;
use codex_protocol::models::ResponseItem;
use codex_protocol::parse_command::ParsedCommand;
use codex_protocol::plan_tool::UpdatePlanArgs;
use codex_protocol::config_types::ReasoningEffort as ReasoningEffortConfig;
use codex_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use mcp_types::CallToolResult;
use mcp_types::Tool as McpTool;
use serde::Deserialize;
use serde::Serialize;
use serde_bytes::ByteBuf;
use uuid::Uuid;

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
    UserInput {
        /// User input items, see `InputItem`
        items: Vec<InputItem>,
    },

    /// Similar to [`Op::UserInput`], but contains additional context required
    /// for a turn of a [`crate::codex_conversation::CodexConversation`].
    UserTurn {
        /// User input items, see `InputItem`
        items: Vec<InputItem>,

        /// `cwd` to use with the [`SandboxPolicy`] and potentially tool calls
        /// such as `local_shell`.
        cwd: PathBuf,

        /// Policy to use for command approval.
        approval_policy: AskForApproval,

        /// Policy to use for tool calls such as `local_shell`.
        sandbox_policy: SandboxPolicy,

        /// Must be a valid model slug for the [`crate::client::ModelClient`]
        /// associated with this conversation.
        model: String,

        /// Will only be honored if the model is configured to use reasoning.
        effort: ReasoningEffortConfig,

        /// Will only be honored if the model is configured to use reasoning.
        summary: ReasoningSummaryConfig,
    },

    /// Override parts of the persistent turn context for subsequent turns.
    ///
    /// All fields are optional; when omitted, the existing value is preserved.
    /// This does not enqueue any input – it only updates defaults used for
    /// future `UserInput` turns.
    OverrideTurnContext {
        /// Updated `cwd` for sandbox/tool calls.
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<PathBuf>,

        /// Updated command approval policy.
        #[serde(skip_serializing_if = "Option::is_none")]
        approval_policy: Option<AskForApproval>,

        /// Updated sandbox policy for tool calls.
        #[serde(skip_serializing_if = "Option::is_none")]
        sandbox_policy: Option<SandboxPolicy>,

        /// Updated model slug. When set, the model family is derived
        /// automatically.
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,

        /// Updated reasoning effort (honored only for reasoning-capable models).
        #[serde(skip_serializing_if = "Option::is_none")]
        effort: Option<ReasoningEffortConfig>,

        /// Updated reasoning summary preference (honored only for reasoning-capable models).
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<ReasoningSummaryConfig>,
    },

    /// Approve a command execution
    ExecApproval {
        /// The id of the submission we are approving
        id: String,
        /// The user's decision in response to the request.
        decision: ReviewDecision,
    },

    /// Approve a code patch
    PatchApproval {
        /// The id of the submission we are approving
        id: String,
        /// The user's decision in response to the request.
        decision: ReviewDecision,
    },

    /// Append an entry to the persistent cross-session message history.
    ///
    /// Note the entry is not guaranteed to be logged if the user has
    /// history disabled, it matches the list of "sensitive" patterns, etc.
    AddToHistory {
        /// The message text to be stored.
        text: String,
    },

    /// Request a single history entry identified by `log_id` + `offset`.
    GetHistoryEntryRequest { offset: usize, log_id: u64 },

    /// Request the full in-memory conversation transcript for the current session.
    /// Reply is delivered via `EventMsg::ConversationHistory`.
    GetHistory,

    /// Request the list of MCP tools available across all configured servers.
    /// Reply is delivered via `EventMsg::McpListToolsResponse`.
    ListMcpTools,

    /// Request the list of available custom prompts.
    ListCustomPrompts,

    /// Request the agent to summarize the current conversation context.
    /// The agent will use its existing context (either conversation history or previous response id)
    /// to generate a summary which will be returned as an AgentMessage event.
    Compact,
    /// Request to shut down codex instance.
    Shutdown,
}

/// Determines the conditions under which the user is consulted to approve
/// running the command proposed by Codex.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AskForApproval {
    /// Under this policy, only "known safe" commands—as determined by
    /// `is_safe_command()`—that **only read files** are auto‑approved.
    /// Everything else will ask the user to approve.
    #[serde(rename = "untrusted")]
    UnlessTrusted,

    /// *All* commands are auto‑approved, but they are expected to run inside a
    /// sandbox where network access is disabled and writes are confined to a
    /// specific set of paths. If the command fails, it will be escalated to
    /// the user to approve execution without a sandbox.
    OnFailure,

    /// The model decides when to ask the user for approval.
    #[default]
    OnRequest,

    /// Never ask the user to approve commands. Failures are immediately returned
    /// to the model, and never escalated to the user for approval.
    Never,
}

/// Determines execution restrictions for model shell commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum SandboxPolicy {
    /// No restrictions whatsoever. Use with caution.
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,

    /// Read-only access to the entire file-system.
    #[serde(rename = "read-only")]
    ReadOnly,

    /// Same as `ReadOnly` but additionally grants write access to the current
    /// working directory ("workspace").
    #[serde(rename = "workspace-write")]
    WorkspaceWrite {
        /// Additional folders (beyond cwd and possibly TMPDIR) that should be
        /// writable from within the sandbox.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writable_roots: Vec<PathBuf>,

        /// When set to `true`, outbound network access is allowed. `false` by
        /// default.
        #[serde(default)]
        network_access: bool,

        /// When set to `true`, will NOT include the per-user `TMPDIR`
        /// environment variable among the default writable roots. Defaults to
        /// `false`.
        #[serde(default)]
        exclude_tmpdir_env_var: bool,

        /// When set to `true`, will NOT include the `/tmp` among the default
        /// writable roots on UNIX. Defaults to `false`.
        #[serde(default)]
        exclude_slash_tmp: bool,
    },
}

/// A writable root path accompanied by a list of subpaths that should remain
/// read‑only even when the root is writable. This is primarily used to ensure
/// top‑level VCS metadata directories (e.g. `.git`) under a writable root are
/// not modified by the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableRoot {
    /// Absolute path, by construction.
    pub root: PathBuf,

    /// Also absolute paths, by construction.
    pub read_only_subpaths: Vec<PathBuf>,
}

impl WritableRoot {
    pub fn is_path_writable(&self, path: &Path) -> bool {
        // Check if the path is under the root.
        if !path.starts_with(&self.root) {
            return false;
        }

        // Check if the path is under any of the read-only subpaths.
        for subpath in &self.read_only_subpaths {
            if path.starts_with(subpath) {
                return false;
            }
        }

        true
    }
}

impl FromStr for SandboxPolicy {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl SandboxPolicy {
    /// Returns a policy with read-only disk access and no network.
    pub fn new_read_only_policy() -> Self {
        SandboxPolicy::ReadOnly
    }

    /// Returns a policy that can read the entire disk, but can only write to
    /// the current working directory and the per-user tmp dir on macOS. It does
    /// not allow network access.
    pub fn new_workspace_write_policy() -> Self {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![],
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    }

    /// Always returns `true`; restricting read access is not supported.
    pub fn has_full_disk_read_access(&self) -> bool {
        true
    }

    pub fn has_full_disk_write_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ReadOnly => false,
            SandboxPolicy::WorkspaceWrite { .. } => false,
        }
    }

    pub fn has_full_network_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ReadOnly => false,
            SandboxPolicy::WorkspaceWrite { network_access, .. } => *network_access,
        }
    }

    /// Returns the list of writable roots (tailored to the current working
    /// directory) together with subpaths that should remain read‑only under
    /// each writable root.
    pub fn get_writable_roots_with_cwd(&self, cwd: &Path) -> Vec<WritableRoot> {
        match self {
            SandboxPolicy::DangerFullAccess => Vec::new(),
            SandboxPolicy::ReadOnly => Vec::new(),
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                exclude_tmpdir_env_var,
                exclude_slash_tmp,
                network_access: _,
            } => {
                // Start from explicitly configured writable roots.
                let mut roots: Vec<PathBuf> = writable_roots.clone();

                // Always include defaults: cwd, /tmp (if present on Unix), and
                // on macOS, the per-user TMPDIR unless explicitly excluded.
                roots.push(cwd.to_path_buf());

                // Include /tmp on Unix unless explicitly excluded.
                if cfg!(unix) && !exclude_slash_tmp {
                    let slash_tmp = PathBuf::from("/tmp");
                    if slash_tmp.is_dir() {
                        roots.push(slash_tmp);
                    }
                }

                // Include $TMPDIR unless explicitly excluded.
                if !exclude_tmpdir_env_var
                    && let Some(tmpdir) = std::env::var_os("TMPDIR")
                    && !tmpdir.is_empty()
                {
                    roots.push(PathBuf::from(tmpdir));
                }

                // For each root, compute subpaths that should remain read-only.
                roots
                    .into_iter()
                    .map(|writable_root| {
                        let mut subpaths = Vec::new();
                        let top_level_git = writable_root.join(".git");
                        if top_level_git.is_dir() {
                            subpaths.push(top_level_git);
                        }
                        WritableRoot {
                            root: writable_root,
                            read_only_subpaths: subpaths,
                        }
                    })
                    .collect()
            }
        }
    }
}

/// User input
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem {
    Text { text: String },
    /// Pre‑encoded data: URI image.
    Image { image_url: String },
    /// Local image path provided by the user.
    LocalImage { path: std::path::PathBuf },
}

/// Event Queue Entry - events from agent
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Event {
    /// Submission `id` that this event is correlated with.
    pub id: String,
    /// Payload
    pub msg: EventMsg,
}

/// Response event from the agent
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventMsg {
    /// Error while executing a submission
    Error(ErrorEvent),

    /// Agent has started a task
    TaskStarted(TaskStartedEvent),

    /// Agent has completed all actions
    TaskComplete(TaskCompleteEvent),

    /// Token count event
    TokenCount(TokenUsage),

    /// Agent text output message
    AgentMessage(AgentMessageEvent),
    /// Agent text output delta message
    AgentMessageDelta(AgentMessageDeltaEvent),

    /// Reasoning events
    AgentReasoning(AgentReasoningEvent),
    AgentReasoningDelta(AgentReasoningDeltaEvent),
    AgentReasoningRawContent(AgentReasoningRawContentEvent),
    AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent),
    AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent),

    /// Ack the client's configure message.
    SessionConfigured(SessionConfiguredEvent),

    McpToolCallBegin(McpToolCallBeginEvent),
    McpToolCallEnd(McpToolCallEndEvent),

    WebSearchBegin(WebSearchBeginEvent),
    WebSearchEnd(WebSearchEndEvent),

    /// Exec events
    ExecCommandBegin(ExecCommandBeginEvent),
    ExecCommandOutputDelta(ExecCommandOutputDeltaEvent),
    ExecCommandEnd(ExecCommandEndEvent),

    /// Approval requests
    ExecApprovalRequest(ExecApprovalRequestEvent),
    ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent),

    /// Background/stream notifications
    BackgroundEvent(BackgroundEventEvent),
    StreamError(StreamErrorEvent),

    /// Patch application notifications
    PatchApplyBegin(PatchApplyBeginEvent),
    PatchApplyEnd(PatchApplyEndEvent),

    TurnDiff(TurnDiffEvent),

    /// History/mcp/custom prompts
    GetHistoryEntryResponse(GetHistoryEntryResponseEvent),
    McpListToolsResponse(McpListToolsResponseEvent),
    ListCustomPromptsResponse(ListCustomPromptsResponseEvent),

    /// Plan update
    PlanUpdate(UpdatePlanArgs),

    /// Turn aborted
    TurnAborted(TurnAbortedEvent),

    /// Shutdown
    ShutdownComplete,

    /// Full conversation history
    ConversationHistory(ConversationHistoryResponseEvent),
}

// Individual event payload types

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskCompleteEvent {
    pub last_agent_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskStartedEvent {
    pub model_context_window: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: u64,
}

impl TokenUsage {
    pub fn is_zero(&self) -> bool { self.total_tokens == 0 }
    pub fn cached_input(&self) -> u64 { self.cached_input_tokens.unwrap_or(0) }
    pub fn non_cached_input(&self) -> u64 { self.input_tokens.saturating_sub(self.cached_input()) }
    pub fn blended_total(&self) -> u64 { self.non_cached_input() + self.output_tokens }
    pub fn tokens_in_context_window(&self) -> u64 {
        self.total_tokens.saturating_sub(self.reasoning_output_tokens.unwrap_or(0))
    }
    pub fn percent_of_context_window_remaining(
        &self,
        context_window: u64,
        baseline_used_tokens: u64,
    ) -> u8 {
        if context_window <= baseline_used_tokens { return 0; }
        let effective_window = context_window - baseline_used_tokens;
        let used = self.tokens_in_context_window().saturating_sub(baseline_used_tokens);
        let remaining = effective_window.saturating_sub(used);
        ((remaining as f32 / effective_window as f32) * 100.0).clamp(0.0, 100.0) as u8
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FinalOutput { pub token_usage: TokenUsage }

impl From<TokenUsage> for FinalOutput { fn from(token_usage: TokenUsage) -> Self { Self { token_usage } } }

impl fmt::Display for FinalOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let token_usage = &self.token_usage;
        write!(
            f,
            "Token usage: total={} input={}{} output={}{}",
            token_usage.blended_total(),
            token_usage.non_cached_input(),
            if token_usage.cached_input() > 0 {
                format!(" (+ {} cached)", token_usage.cached_input())
            } else { String::new() },
            token_usage.output_tokens,
            token_usage
                .reasoning_output_tokens
                .map(|r| format!(" (reasoning {r})"))
                .unwrap_or_default()
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentMessageEvent { pub message: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentMessageDeltaEvent { pub delta: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningEvent { pub text: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningRawContentEvent { pub text: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningRawContentDeltaEvent { pub delta: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningSectionBreakEvent {}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningDeltaEvent { pub delta: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpInvocation {
    /// Name of the MCP server as defined in the config.
    pub server: String,
    /// Name of the tool as given by the MCP server.
    pub tool: String,
    /// Arguments to the tool call.
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolCallBeginEvent { pub call_id: String, pub invocation: McpInvocation }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolCallEndEvent {
    pub call_id: String,
    pub invocation: McpInvocation,
    pub duration: Duration,
    /// Result of the tool call. Note this could be an error.
    pub result: Result<CallToolResult, String>,
}

impl McpToolCallEndEvent { pub fn is_success(&self) -> bool { match &self.result { Ok(result) => !result.is_error.unwrap_or(false), Err(_) => false } } }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebSearchBeginEvent { pub call_id: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebSearchEndEvent { pub call_id: String, pub query: String }

/// Response payload for `Op::GetHistory` containing the current session's in-memory transcript.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationHistoryResponseEvent { pub conversation_id: Uuid, pub entries: Vec<ResponseItem> }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecCommandBeginEvent {
    pub call_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub parsed_cmd: Vec<ParsedCommand>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecCommandEndEvent {
    pub call_id: String,
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub aggregated_output: String,
    pub exit_code: i32,
    pub duration: Duration,
    pub formatted_output: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecOutputStream { Stdout, Stderr }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecCommandOutputDeltaEvent {
    pub call_id: String,
    pub stream: ExecOutputStream,
    #[serde(with = "serde_bytes")]
    pub chunk: ByteBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecApprovalRequestEvent {
    pub call_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApplyPatchApprovalRequestEvent {
    pub call_id: String,
    pub changes: HashMap<PathBuf, FileChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackgroundEventEvent { pub message: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StreamErrorEvent { pub message: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatchApplyBeginEvent {
    pub call_id: String,
    pub auto_approved: bool,
    pub changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatchApplyEndEvent {
    pub call_id: String,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnDiffEvent { pub unified_diff: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetHistoryEntryResponseEvent {
    pub offset: usize,
    pub log_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<HistoryEntry>,
}

/// Response payload for `Op::ListMcpTools`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpListToolsResponseEvent { pub tools: std::collections::HashMap<String, McpTool> }

/// Response payload for `Op::ListCustomPrompts`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListCustomPromptsResponseEvent { pub custom_prompts: Vec<CustomPrompt> }

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct SessionConfiguredEvent {
    pub session_id: Uuid,
    pub model: String,
    pub history_log_id: u64,
    pub history_entry_count: usize,
}

/// User's decision in response to an ExecApprovalRequest.
#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ApprovedForSession,
    #[default]
    Denied,
    Abort,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    Add { content: String },
    Delete,
    Update { unified_diff: String, move_path: Option<PathBuf> },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Chunk {
    pub orig_index: u32,
    pub deleted_lines: Vec<String>,
    pub inserted_lines: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnAbortedEvent { pub reason: TurnAbortReason }

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TurnAbortReason { Interrupted, Replaced }

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize Event to verify that its JSON representation has the expected amount of nesting.
    #[test]
    fn serialize_event() {
        let session_id: Uuid = uuid::uuid!("67e55044-10b1-426f-9247-bb680e5fe0c8");
        let event = Event {
            id: "1234".to_string(),
            msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
                session_id,
                model: "codex-mini-latest".to_string(),
                history_log_id: 0,
                history_entry_count: 0,
            }),
        };
        let serialized = serde_json::to_string(&event).unwrap();
        assert_eq!(
            serialized,
            r#"{"id":"1234","msg":{"type":"session_configured","session_id":"67e55044-10b1-426f-9247-bb680e5fe0c8","model":"codex-mini-latest","history_log_id":0,"history_entry_count":0}}"#
        );
    }
}

use serde::{Deserialize, Serialize};
use slide_common::ApprovalMode;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessageDeltaEvent {
    pub delta: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessageEvent {
    pub content: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReasoningEvent {
    pub reasoning: String,
    pub reasoning_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReasoningRawContentEvent {
    pub content: String,
    pub reasoning_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfiguredEvent {
    pub session_id: String,
    pub config: SessionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub model: String,
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnStartEvent {
    pub turn_id: String,
    pub user_input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCompleteEvent {
    pub turn_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxPolicy {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewDecision {
    Approved,
    ApprovedForSession,  
    Denied,
    Abort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileChange {
    Add { content: String },
    Delete,
    Update { 
        unified_diff: String, 
        move_path: Option<PathBuf> 
    },
}

pub type AskForApproval = ApprovalMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEvent {
    pub role: String,
    pub content: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub error: String,
    pub error_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionEvent {
    pub turn_id: String,
    pub success: bool,
}

// Additional types needed by codex.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationHistoryResponseEvent {
    pub conversation_id: String,
    pub messages: Vec<MessageEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStartedEvent {
    pub task_id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnAbortReason {
    UserRequested,
    Timeout,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnAbortedEvent {
    pub turn_id: String,
    pub reason: TurnAbortReason,
}

// Additional event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReasoningDeltaEvent {
    pub delta: String,
    pub reasoning_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReasoningRawContentDeltaEvent {
    pub delta: String,
    pub reasoning_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReasoningSectionBreakEvent {
    pub reasoning_id: String,
    pub section: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchApprovalRequestEvent {
    pub request_id: String,
    pub patch_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundEventEvent {
    pub event_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecApprovalRequestEvent {
    pub request_id: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecCommandBeginEvent {
    pub command_id: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecCommandEndEvent {
    pub command_id: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyBeginEvent {
    pub patch_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyEndEvent {
    pub patch_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamErrorEvent {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompleteEvent {
    pub task_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnDiffEvent {
    pub turn_id: String,
    pub diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchBeginEvent {
    pub search_id: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchEndEvent {
    pub search_id: String,
    pub results_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListCustomPromptsResponseEvent {
    pub prompts: Vec<String>,
}

// Union type for all events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    AgentMessageDelta(AgentMessageDeltaEvent),
    AgentMessage(AgentMessageEvent),
    AgentReasoning(AgentReasoningEvent),
    Error(ErrorEvent),
    SessionConfigured(SessionConfiguredEvent),
    TurnStart(TurnStartEvent),
    TurnComplete(TurnCompleteEvent),
    ConversationHistoryResponse(ConversationHistoryResponseEvent),
    TaskStarted(TaskStartedEvent),
    TurnAborted(TurnAbortedEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMsg {
    pub event: Event,
    pub timestamp: u64,
}

// Input and submission types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputItem {
    pub content: String,
    pub item_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub input: Vec<InputItem>,
    pub submission_id: String,
}

// Operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
    Create,
    Update,
    Delete,
}

// Token usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: u64,
}