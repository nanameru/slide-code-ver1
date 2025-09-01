use crate::seatbelt::SandboxPolicy;
use slide_common::ApprovalMode;

#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub sandbox_policy: SandboxPolicy,
    pub approval_mode: ApprovalMode,
    pub include_view_image_tool: bool,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self { sandbox_policy: SandboxPolicy::WorkspaceWrite, approval_mode: ApprovalMode::default(), include_view_image_tool: false }
    }
}

