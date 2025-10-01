// Environment context for AI model
// Reference: codex-1/codex-rs/core/src/environment_context.rs (全329行)
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::codex2::TurnContext;
use crate::approval_manager::AskForApproval;
use crate::seatbelt::SandboxPolicy;
use protocol::config_types::SandboxMode;
use crate::conversation_history::{ResponseItem, ContentItem};
use protocol::protocol::{ENVIRONMENT_CONTEXT_OPEN_TAG, ENVIRONMENT_CONTEXT_CLOSE_TAG};

/// Network access configuration
/// Reference: codex-1/codex-rs/core/src/environment_context.rs lines 16-22
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkAccess {
    Restricted,
    Enabled,
}

impl std::fmt::Display for NetworkAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkAccess::Restricted => write!(f, "restricted"),
            NetworkAccess::Enabled => write!(f, "enabled"),
        }
    }
}

/// Environment context that is sent to the AI model to inform it about
/// the current working environment.
/// Reference: codex-1/codex-rs/core/src/environment_context.rs lines 23-32
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename = "environment_context", rename_all = "snake_case")]
pub(crate) struct EnvironmentContext {
    pub cwd: Option<PathBuf>,
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    pub network_access: Option<NetworkAccess>,
    pub writable_roots: Option<Vec<PathBuf>>,
}

impl EnvironmentContext {
    /// Creates a new EnvironmentContext from TurnContext fields.
    /// Reference: codex-1/codex-rs/core/src/environment_context.rs lines 35-74
    pub fn new(
        cwd: Option<PathBuf>,
        approval_policy: Option<AskForApproval>,
        sandbox_policy: Option<&SandboxPolicy>,
    ) -> Self {
        Self {
            cwd,
            approval_policy,
            sandbox_mode: match sandbox_policy {
                Some(SandboxPolicy::DangerFullAccess) => Some(SandboxMode::DangerFullAccess),
                Some(SandboxPolicy::ReadOnly) => Some(SandboxMode::ReadOnly),
                Some(SandboxPolicy::WorkspaceWrite { .. }) => Some(SandboxMode::WorkspaceWrite),
                None => None,
            },
            network_access: match sandbox_policy {
                Some(SandboxPolicy::DangerFullAccess) => Some(NetworkAccess::Enabled),
                Some(SandboxPolicy::ReadOnly) => Some(NetworkAccess::Restricted),
                Some(SandboxPolicy::WorkspaceWrite { network_access, .. }) => {
                    if *network_access {
                        Some(NetworkAccess::Enabled)
                    } else {
                        Some(NetworkAccess::Restricted)
                    }
                }
                None => None,
            },
            writable_roots: match sandbox_policy {
                Some(SandboxPolicy::WorkspaceWrite { writable_roots, .. }) => {
                    if writable_roots.is_empty() {
                        None
                    } else {
                        // Extract root paths from WritableRoot structs
                        // writable_roots is Vec<WritableRoot> where WritableRoot { root: PathBuf, ... }
                        Some(writable_roots.iter().map(|wr| wr.root.clone()).collect())
                    }
                }
                _ => None,
            },
        }
    }

    /// Compares two environment contexts. Useful when comparing turn to turn
    /// to detect if environment has changed and needs to be recorded.
    /// Reference: codex-1/codex-rs/core/src/environment_context.rs lines 79-95
    pub fn equals_except_shell(&self, other: &EnvironmentContext) -> bool {
        let EnvironmentContext {
            cwd,
            approval_policy,
            sandbox_mode,
            network_access,
            writable_roots,
        } = other;

        self.cwd == *cwd
            && self.approval_policy == *approval_policy
            && self.sandbox_mode == *sandbox_mode
            && self.network_access == *network_access
            && self.writable_roots == *writable_roots
    }
}

/// Convert from TurnContext to EnvironmentContext
/// Reference: codex-1/codex-rs/core/src/environment_context.rs lines 98-107
impl From<&TurnContext> for EnvironmentContext {
    fn from(turn_context: &TurnContext) -> Self {
        Self::new(
            Some(turn_context.cwd.clone()),
            Some(turn_context.approval_policy.clone()),
            Some(&turn_context.sandbox_policy),
        )
    }
}

impl EnvironmentContext {
    /// Serializes the environment context to XML format for the AI model.
    /// Reference: codex-1/codex-rs/core/src/environment_context.rs lines 110-160
    ///
    /// Output format:
    /// ```xml
    /// <environment_context>
    ///   <cwd>/Users/kimura/project</cwd>
    ///   <approval_policy>on-request</approval_policy>
    ///   <sandbox_mode>workspace-write</sandbox_mode>
    ///   <network_access>restricted</network_access>
    ///   <writable_roots>
    ///     <root>/Users/kimura/project</root>
    ///     <root>/tmp</root>
    ///   </writable_roots>
    /// </environment_context>
    /// ```
    pub fn serialize_to_xml(self) -> String {
        let mut lines = vec![ENVIRONMENT_CONTEXT_OPEN_TAG.to_string()];

        if let Some(cwd) = self.cwd {
            lines.push(format!("  <cwd>{}</cwd>", cwd.to_string_lossy()));
        }

        if let Some(approval_policy) = self.approval_policy {
            let policy_str = match approval_policy {
                AskForApproval::Never => "never",
                AskForApproval::OnRequest => "on-request",
                AskForApproval::OnFailure => "on-failure",
            };
            lines.push(format!("  <approval_policy>{}</approval_policy>", policy_str));
        }

        if let Some(sandbox_mode) = self.sandbox_mode {
            lines.push(format!("  <sandbox_mode>{sandbox_mode}</sandbox_mode>"));
        }

        if let Some(network_access) = self.network_access {
            lines.push(format!(
                "  <network_access>{network_access}</network_access>"
            ));
        }

        if let Some(writable_roots) = self.writable_roots {
            lines.push("  <writable_roots>".to_string());
            for writable_root in writable_roots {
                lines.push(format!(
                    "    <root>{}</root>",
                    writable_root.to_string_lossy()
                ));
            }
            lines.push("  </writable_roots>".to_string());
        }

        lines.push(ENVIRONMENT_CONTEXT_CLOSE_TAG.to_string());
        lines.join("\n")
    }
}

/// Convert EnvironmentContext to ResponseItem for conversation history
/// Reference: codex-1/codex-rs/core/src/environment_context.rs lines 163-172
impl From<EnvironmentContext> for ResponseItem {
    fn from(ec: EnvironmentContext) -> Self {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: ec.serialize_to_xml(),
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use protocol::protocol::WritableRoot;

    fn workspace_write_policy(roots: Vec<&str>, network_access: bool) -> SandboxPolicy {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: roots
                .into_iter()
                .map(|r| WritableRoot {
                    root: PathBuf::from(r),
                    read_only_subpaths: vec![],
                })
                .collect(),
            network_access,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    }

    #[test]
    fn serialize_workspace_write_environment_context() {
        let context = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(&workspace_write_policy(vec!["/repo", "/tmp"], false)),
        );

        let expected = r#"<environment_context>
  <cwd>/repo</cwd>
  <approval_policy>on-request</approval_policy>
  <sandbox_mode>workspace-write</sandbox_mode>
  <network_access>restricted</network_access>
  <writable_roots>
    <root>/repo</root>
    <root>/tmp</root>
  </writable_roots>
</environment_context>"#;

        assert_eq!(context.serialize_to_xml(), expected);
    }

    #[test]
    fn serialize_read_only_environment_context() {
        let context = EnvironmentContext::new(
            None,
            Some(AskForApproval::Never),
            Some(&SandboxPolicy::ReadOnly),
        );

        let expected = r#"<environment_context>
  <approval_policy>never</approval_policy>
  <sandbox_mode>read-only</sandbox_mode>
  <network_access>restricted</network_access>
</environment_context>"#;

        assert_eq!(context.serialize_to_xml(), expected);
    }

    #[test]
    fn serialize_full_access_environment_context() {
        let context = EnvironmentContext::new(
            None,
            Some(AskForApproval::OnFailure),
            Some(&SandboxPolicy::DangerFullAccess),
        );

        let expected = r#"<environment_context>
  <approval_policy>on-failure</approval_policy>
  <sandbox_mode>danger-full-access</sandbox_mode>
  <network_access>enabled</network_access>
</environment_context>"#;

        assert_eq!(context.serialize_to_xml(), expected);
    }

    #[test]
    fn equals_except_shell_compares_approval_policy() {
        let context1 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(&workspace_write_policy(vec!["/repo"], false)),
        );
        let context2 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::Never),
            Some(&workspace_write_policy(vec!["/repo"], true)),
        );
        assert!(!context1.equals_except_shell(&context2));
    }

    #[test]
    fn equals_except_shell_compares_sandbox_policy() {
        let context1 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(&SandboxPolicy::new_read_only_policy()),
        );
        let context2 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(&SandboxPolicy::new_workspace_write_policy()),
        );

        assert!(!context1.equals_except_shell(&context2));
    }

    #[test]
    fn equals_except_shell_compares_workspace_write_policy() {
        let context1 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(&workspace_write_policy(vec!["/repo", "/tmp", "/var"], false)),
        );
        let context2 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(&workspace_write_policy(vec!["/repo", "/tmp"], true)),
        );

        assert!(!context1.equals_except_shell(&context2));
    }
}
