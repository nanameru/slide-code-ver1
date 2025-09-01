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