use serde::{Deserialize, Serialize};
use std::collections::{HashSet, HashMap};
use std::path::Path;
use sha2::{Sha256, Digest};

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
    approved_patches: HashSet<String>, // パッチのハッシュを保存
    session_approvals: HashMap<String, bool>, // セッション内承認の記録
}

/// パッチ適用の承認結果
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalResult {
    /// 自動承認
    AutoApproved,
    /// ユーザーによる承認
    UserApproved,
    /// 拒否
    Denied { reason: String },
    /// ユーザーの判断が必要
    RequiresUserInput,
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
            approved_patches: HashSet::new(),
            session_approvals: HashMap::new(),
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

/// パッチ適用の安全性評価結果
#[derive(Debug, Clone, PartialEq)]
enum PatchSafetyAssessment {
    Safe,
    RequiresApproval { reason: String },
    Dangerous { reason: String },
}

/// パッチ承認関連の実装をApprovalManagerに追加
impl ApprovalManager {
    /// パッチ適用の承認を評価
    pub fn evaluate_patch_approval(&mut self, patch_content: &str, session_id: Option<&str>) -> ApprovalResult {
        let patch_hash = self.calculate_patch_hash(patch_content);
        
        // 既に承認済みのパッチかチェック
        if self.approved_patches.contains(&patch_hash) {
            return ApprovalResult::AutoApproved;
        }

        // セッション内承認をチェック
        if let Some(session_id) = session_id {
            let session_key = format!("patch_{}", session_id);
            if let Some(&approved) = self.session_approvals.get(&session_key) {
                if approved {
                    return ApprovalResult::AutoApproved;
                } else {
                    return ApprovalResult::Denied {
                        reason: "Previously denied in this session".to_string(),
                    };
                }
            }
        }

        // パッチの安全性を評価
        match self.assess_patch_safety(patch_content) {
            PatchSafetyAssessment::Safe => {
                self.approved_patches.insert(patch_hash);
                ApprovalResult::AutoApproved
            }
            PatchSafetyAssessment::RequiresApproval { reason: _ } => {
                match self.policy {
                    AskForApproval::Never => {
                        self.approved_patches.insert(patch_hash);
                        ApprovalResult::AutoApproved
                    }
                    _ => ApprovalResult::RequiresUserInput,
                }
            }
            PatchSafetyAssessment::Dangerous { reason } => {
                ApprovalResult::Denied { reason }
            }
        }
    }

    /// パッチのハッシュを計算
    fn calculate_patch_hash(&self, patch_content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(patch_content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// パッチの安全性を評価
    fn assess_patch_safety(&self, patch_content: &str) -> PatchSafetyAssessment {
        // 危険なパターンをチェック
        let dangerous_patterns = [
            "rm -rf", "sudo", "chmod +x", "/etc/", "/sys/", "/proc/",
            "password", "private_key", "secret", "token"
        ];

        for pattern in &dangerous_patterns {
            if patch_content.contains(pattern) {
                return PatchSafetyAssessment::Dangerous {
                    reason: format!("Contains dangerous pattern: {}", pattern),
                };
            }
        }

        // 大きなファイルの変更をチェック
        if patch_content.len() > 100_000 {
            return PatchSafetyAssessment::RequiresApproval {
                reason: "Large patch requires manual review".to_string(),
            };
        }

        // バイナリファイルの検出
        if patch_content.contains("Binary file") || has_binary_content(patch_content) {
            return PatchSafetyAssessment::RequiresApproval {
                reason: "Binary file changes require approval".to_string(),
            };
        }

        PatchSafetyAssessment::Safe
    }

    /// ユーザー承認を記録
    pub fn record_user_approval(&mut self, patch_content: &str, approved: bool, session_id: Option<&str>) {
        let patch_hash = self.calculate_patch_hash(patch_content);
        
        if approved {
            self.approved_patches.insert(patch_hash);
        }

        if let Some(session_id) = session_id {
            let session_key = format!("patch_{}", session_id);
            self.session_approvals.insert(session_key, approved);
        }
    }

    /// コマンド実行の承認を評価（拡張版）
    pub fn evaluate_command_approval(&mut self, command: &[String]) -> ApprovalResult {
        if command.is_empty() {
            return ApprovalResult::Denied {
                reason: "Empty command".to_string(),
            };
        }

        let cmd = &command[0];
        
        // 既に承認されているコマンドかチェック
        if self.approved_commands.contains(command) {
            return ApprovalResult::AutoApproved;
        }

        // 信頼できるコマンドかチェック
        if self.trusted_commands.contains(cmd) {
            self.approved_commands.insert(command.to_vec());
            return ApprovalResult::AutoApproved;
        }

        // 危険なコマンドかチェック
        let dangerous_commands = [
            "rm", "rmdir", "del", "format", "fdisk", "dd", "mkfs",
            "mount", "umount", "sudo", "su", "kill", "killall",
            "shutdown", "reboot", "halt"
        ];

        if dangerous_commands.contains(&cmd.as_str()) {
            return ApprovalResult::Denied {
                reason: format!("Dangerous command: {}", cmd),
            };
        }

        // ポリシーに基づいて決定
        match self.policy {
            AskForApproval::Never => {
                self.approved_commands.insert(command.to_vec());
                ApprovalResult::AutoApproved
            }
            AskForApproval::OnRequest => ApprovalResult::RequiresUserInput,
            AskForApproval::UnlessTrusted => ApprovalResult::RequiresUserInput,
            AskForApproval::OnFailure => {
                self.approved_commands.insert(command.to_vec());
                ApprovalResult::AutoApproved
            }
        }
    }
}

/// バイナリコンテンツの検出ヘルパー関数
fn has_binary_content(content: &str) -> bool {
    // 非ASCII文字の割合をチェック
    let total_chars = content.len();
    if total_chars == 0 {
        return false;
    }

    let non_ascii_count = content.chars().filter(|c| !c.is_ascii()).count();
    let non_ascii_ratio = non_ascii_count as f32 / total_chars as f32;
    
    // 30%以上が非ASCII文字の場合はバイナリと判定
    non_ascii_ratio > 0.3
}
