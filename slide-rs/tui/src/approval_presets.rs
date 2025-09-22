use slide_core::protocol::{CoreAskForApproval as AskForApproval, CoreSandboxPolicy as SandboxPolicy};

#[derive(Debug, Clone)]
pub struct ApprovalPreset {
    pub name: &'static str,
    pub description: &'static str,
    pub approval: AskForApproval,
    pub sandbox: SandboxPolicy,
}

pub fn builtin_approval_presets() -> Vec<ApprovalPreset> {
    vec![
        ApprovalPreset {
            name: "Read Only",
            description: "Read files; approval required for edits/exec/network",
            approval: AskForApproval::OnRequest,
            sandbox: SandboxPolicy::ReadOnly,
        },
        ApprovalPreset {
            name: "Auto",
            description: "Workspace write; approval for outside workspace/no network",
            approval: AskForApproval::OnRequest,
            sandbox: SandboxPolicy::WorkspaceWrite {
                writable_roots: Vec::new(),
                network_access: false,
                exclude_tmpdir_env_var: false,
                exclude_system_tmp: false,
            },
        },
        ApprovalPreset {
            name: "Full Access",
            description: "Edits/exec/network without approval — use with caution",
            approval: AskForApproval::Never,
            sandbox: SandboxPolicy::DangerFullAccess,
        },
    ]
}


