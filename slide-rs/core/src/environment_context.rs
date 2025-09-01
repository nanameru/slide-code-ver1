use crate::seatbelt::SandboxPolicy;
use crate::shell::Shell;
use slide_common::ApprovalMode;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

const ENVIRONMENT_CONTEXT_START: &str = "<environment_context>";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, Default)]
pub struct EnvironmentContext {
    pub cwd: Option<PathBuf>,
    pub approval_policy: Option<ApprovalMode>,
    pub sandbox_mode: Option<SandboxMode>,
    pub shell: Option<Shell>,
}

impl EnvironmentContext {
    pub fn new(
        cwd: Option<PathBuf>,
        approval_policy: Option<ApprovalMode>,
        sandbox_policy: Option<SandboxPolicy>,
        shell: Option<Shell>,
    ) -> Self {
        Self {
            cwd,
            approval_policy,
            sandbox_mode: match sandbox_policy {
                Some(SandboxPolicy::ReadOnly) => Some(SandboxMode::ReadOnly),
                Some(SandboxPolicy::WorkspaceWrite) => Some(SandboxMode::WorkspaceWrite),
                Some(SandboxPolicy::DangerFullAccess) => Some(SandboxMode::DangerFullAccess),
                None => None,
            },
            shell,
        }
    }

    /// Serializes the environment context to XML format
    pub fn serialize_to_xml(self) -> String {
        let mut lines = vec![ENVIRONMENT_CONTEXT_START.to_string()];

        if let Some(ref cwd) = self.cwd {
            lines.push(format!("  <cwd>{}</cwd>", cwd.display()));
        }

        if let Some(ref approval_policy) = self.approval_policy {
            lines.push(format!("  <approval_policy>{:?}</approval_policy>", approval_policy));
        }

        if let Some(ref sandbox_mode) = self.sandbox_mode {
            lines.push(format!("  <sandbox_mode>{:?}</sandbox_mode>", sandbox_mode));
        }

        // Default to restricted network access for safety
        lines.push("  <network_access>restricted</network_access>".to_string());

        if let Some(ref shell) = self.shell {
            lines.push(format!("  <shell>{:?}</shell>", shell));
        }

        lines.push("</environment_context>".to_string());
        lines.join("\n")
    }

    pub fn serialize_to_xml_string(&self) -> String {
        self.clone().serialize_to_xml()
    }
}

// Legacy compatibility function
pub fn build_environment_context(cwd: &str, approval_policy: &str, sandbox: SandboxPolicy, network: bool) -> String {
    format!(
        "<environment_context>\n  <cwd>{}</cwd>\n  <approval_policy>{}</approval_policy>\n  <sandbox_policy>{:?}</sandbox_policy>\n  <network_access>{}</network_access>\n</environment_context>",
        cwd,
        approval_policy,
        sandbox,
        if network { "enabled" } else { "restricted" }
    )
}

