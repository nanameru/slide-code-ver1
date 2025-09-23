use slide_core::protocol::ReasoningEffort;
use slide_core::protocol::{CoreAskForApproval as AskForApproval, CoreSandboxPolicy as SandboxPolicy};
use slide_core::codex::{Event, Op, ReviewDecision};

use crate::history_cell::HistoryCellTrait;

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum AppEvent {
    CodexEvent(Event),

    /// Start a new session.
    NewSession,

    /// Request to exit the application gracefully.
    ExitRequest,

    /// Forward an `Op` to the Agent. Using an `AppEvent` for this avoids
    /// bubbling channels through layers of widgets.
    CodexOp(Op),

    /// Result of computing a `/diff` command.
    DiffResult(String),

    InsertHistoryCell(Box<dyn HistoryCellTrait>),

    StartCommitAnimation,
    StopCommitAnimation,
    CommitTick,

    /// Update the current reasoning effort in the running app and widget.
    UpdateReasoningEffort(Option<ReasoningEffort>),

    /// Update the current model slug in the running app and widget.
    UpdateModel(String),

    /// Persist the selected model and reasoning effort to the appropriate config.
    PersistModelSelection {
        model: String,
        effort: Option<ReasoningEffort>,
    },

    /// Update the current approval policy in the running app and widget.
    UpdateAskForApprovalPolicy(AskForApproval),

    /// Update the current sandbox policy in the running app and widget.
    UpdateSandboxPolicy(SandboxPolicy),

    /// Generic tool execution textual output to display in history
    ToolOutput {
        text: String,
    },

    // Additional events for compatibility
    ExecApproval {
        id: String,
        decision: ReviewDecision,
    },
    PatchApproval {
        id: String,
        decision: ReviewDecision,
    },
    FileReadRequest {
        path: String,
    },
    FileReadResult {
        path: String,
        content: Result<String, String>,
    },
    FileSearchResults {
        query: String,
        matches: Vec<crate::bottom_pane::file_search_popup::FileMatch>,
    },
    StartFileSearch {
        query: String,
    },
}
