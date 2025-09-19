// Re-export the standalone `protocol` crate so existing `crate::protocol::*`
// imports continue to work within the `core` crate and downstream crates.
pub use protocol::*;

// Export our enhanced types for compatibility
pub use crate::approval_manager::{AskForApproval as CoreAskForApproval, ApprovalManager, ApprovalRequest, ApprovalResponse};
pub use crate::seatbelt::SandboxPolicy as CoreSandboxPolicy;
pub use crate::exec_sandboxed::{SandboxedExecutor, ExecParams as CoreExecParams, ExecResult as CoreExecResult};