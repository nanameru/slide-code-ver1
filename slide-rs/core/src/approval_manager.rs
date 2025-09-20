use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Approval policy for AI commands and tool usage
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AskForApproval {
    /// Ask for approval unless the command is explicitly trusted
    UnlessTrusted,
    /// Ask for approval only when a command fails and needs escalation
    OnFailure,
    /// Ask for approval on every request that requires elevated permissions
    OnRequest,
    /// Never ask for approval (auto-approve everything)
    Never,
}

impl Default for AskForApproval {
    fn default() -> Self {
        AskForApproval::OnRequest
    }
}

/// Manages the approval workflow for commands and operations
#[derive(Debug, Clone)]
pub struct ApprovalManager {
    policy: AskForApproval,
    approved_commands: HashSet<Vec<String>>,
    trusted_commands: HashSet<String>,
}

impl Default for ApprovalManager {
    fn default() -> Self {
        let mut trusted_commands = HashSet::new();

        // Default trusted commands (safe read-only operations)
        for cmd in &[
            "ls", "cat", "grep", "find", "echo", "pwd", "whoami", "date", "which", "head", "tail",
            "wc", "sort", "uniq", "file", "stat",
        ] {
            trusted_commands.insert(cmd.to_string());
        }

        Self {
            policy: AskForApproval::default(),
            approved_commands: HashSet::new(),
            trusted_commands,
        }
    }
}

impl ApprovalManager {
    pub fn new(policy: AskForApproval) -> Self {
        Self {
            policy,
            ..Default::default()
        }
    }

    /// Check if a command needs user approval
    pub fn needs_approval(&self, command: &[String], with_escalated_permissions: bool) -> bool {
        if command.is_empty() {
            return true;
        }

        match self.policy {
            AskForApproval::Never => false,
            AskForApproval::UnlessTrusted => {
                !self.is_trusted_command(&command[0]) && !self.is_pre_approved(command)
            }
            AskForApproval::OnFailure => {
                // Only ask for approval if escalated permissions are explicitly requested
                with_escalated_permissions
            }
            AskForApproval::OnRequest => {
                // Ask for approval for any non-trusted command or escalated permissions
                with_escalated_permissions || !self.is_trusted_command(&command[0])
            }
        }
    }

    /// Check if a command is in the trusted list
    pub fn is_trusted_command(&self, command: &str) -> bool {
        self.trusted_commands.contains(command)
    }

    /// Check if a command was previously approved
    pub fn is_pre_approved(&self, command: &[String]) -> bool {
        self.approved_commands.contains(command)
    }

    /// Add a command to the approved list
    pub fn approve_command(&mut self, command: Vec<String>) {
        self.approved_commands.insert(command);
    }

    /// Add a command to the trusted list
    pub fn trust_command(&mut self, command: String) {
        self.trusted_commands.insert(command);
    }

    /// Remove a command from the approved list
    pub fn revoke_approval(&mut self, command: &[String]) {
        self.approved_commands.remove(command);
    }

    /// Get the current approval policy
    pub fn policy(&self) -> &AskForApproval {
        &self.policy
    }

    /// Set a new approval policy
    pub fn set_policy(&mut self, policy: AskForApproval) {
        self.policy = policy;
    }

    /// Clear all approved commands
    pub fn clear_approvals(&mut self) {
        self.approved_commands.clear();
    }

    /// Get count of approved commands
    pub fn approved_count(&self) -> usize {
        self.approved_commands.len()
    }

    /// Get count of trusted commands
    pub fn trusted_count(&self) -> usize {
        self.trusted_commands.len()
    }
}

/// Request for user approval
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub command: Vec<String>,
    pub working_dir: Option<String>,
    pub justification: Option<String>,
    pub with_escalated_permissions: bool,
    pub sandbox_policy: String,
}

impl ApprovalRequest {
    pub fn new(
        command: Vec<String>,
        working_dir: Option<&Path>,
        justification: Option<String>,
        with_escalated_permissions: bool,
        sandbox_policy: String,
    ) -> Self {
        Self {
            command,
            working_dir: working_dir.map(|p| p.display().to_string()),
            justification,
            with_escalated_permissions,
            sandbox_policy,
        }
    }

    /// Generate a human-readable description of the request
    pub fn description(&self) -> String {
        let cmd_str = self.command.join(" ");
        let mut desc = format!("Command: {}", cmd_str);

        if let Some(ref wd) = self.working_dir {
            desc.push_str(&format!("\nWorking directory: {}", wd));
        }

        if self.with_escalated_permissions {
            desc.push_str("\n⚠️  Requires escalated permissions");
        }

        desc.push_str(&format!("\nSandbox policy: {}", self.sandbox_policy));

        if let Some(ref justification) = self.justification {
            desc.push_str(&format!("\nJustification: {}", justification));
        }

        desc
    }
}

/// Response to an approval request
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalResponse {
    /// User approved the request
    Approved,
    /// User denied the request
    Denied,
    /// User approved and wants to trust this command going forward
    ApprovedAndTrust,
    /// User wants to modify the approval policy
    ChangePolicy(AskForApproval),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_trusted_commands() {
        let manager = ApprovalManager::default();
        assert!(manager.is_trusted_command("ls"));
        assert!(manager.is_trusted_command("cat"));
        assert!(!manager.is_trusted_command("rm"));
        assert!(!manager.is_trusted_command("sudo"));
    }

    #[test]
    fn test_approval_policies() {
        let mut manager = ApprovalManager::new(AskForApproval::Never);
        assert!(!manager.needs_approval(&["rm".to_string(), "-rf".to_string()], false));

        manager.set_policy(AskForApproval::UnlessTrusted);
        assert!(manager.needs_approval(&["rm".to_string(), "-rf".to_string()], false));
        assert!(!manager.needs_approval(&["ls".to_string()], false));

        manager.set_policy(AskForApproval::OnRequest);
        assert!(manager.needs_approval(&["rm".to_string(), "-rf".to_string()], false));
        assert!(manager.needs_approval(&["ls".to_string()], true)); // escalated permissions
    }

    #[test]
    fn test_command_approval() {
        let mut manager = ApprovalManager::default();
        let command = vec!["rm".to_string(), "file.txt".to_string()];

        assert!(!manager.is_pre_approved(&command));
        manager.approve_command(command.clone());
        assert!(manager.is_pre_approved(&command));

        manager.revoke_approval(&command);
        assert!(!manager.is_pre_approved(&command));
    }

    #[test]
    fn test_approval_request_description() {
        let request = ApprovalRequest::new(
            vec!["git".to_string(), "push".to_string()],
            Some(Path::new("/repo")),
            Some("Push changes to remote".to_string()),
            true,
            "workspace-write".to_string(),
        );

        let desc = request.description();
        assert!(desc.contains("git push"));
        assert!(desc.contains("/repo"));
        assert!(desc.contains("escalated permissions"));
        assert!(desc.contains("Push changes to remote"));
    }
}
