use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
    UserInput { text: String },
    Interrupt,
    ExecApproval { id: String, decision: ReviewDecision },
    PatchApproval { id: String, decision: ReviewDecision },
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewDecision { 
    Approved, 
    ApprovedForSession, 
    Denied, 
    Abort 
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub id: String,
    pub op: Op,
}

impl Submission {
    pub fn new(op: Op) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            op,
        }
    }
}