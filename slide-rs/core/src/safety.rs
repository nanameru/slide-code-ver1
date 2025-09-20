use crate::approval_manager::ApprovalManager;
use crate::seatbelt::SandboxPolicy;
use slide_common::ApprovalMode;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum SafetyCheck {
    AutoApprove,
    AskUser,
    Reject { reason: String },
}

pub fn assess_patch_safety(
    patch_content: &str,
    policy: ApprovalMode,
    _sandbox_policy: &SandboxPolicy,
    _cwd: &Path,
) -> SafetyCheck {
    if patch_content.trim().is_empty() {
        return SafetyCheck::Reject {
            reason: "empty patch".to_string(),
        };
    }

    // Basic safety checks for patches
    if patch_content.contains("rm -rf") || patch_content.contains("sudo") {
        return SafetyCheck::AskUser;
    }

    match policy {
        ApprovalMode::FullAuto => SafetyCheck::AutoApprove,
        ApprovalMode::AutoEdit => SafetyCheck::AskUser,
        ApprovalMode::Suggest => SafetyCheck::AskUser,
    }
}

pub fn assess_command_safety(
    command: &[String],
    approval_policy: ApprovalMode,
    _sandbox_policy: &SandboxPolicy,
    approved: &HashSet<Vec<String>>,
    _with_escalated_permissions: bool,
) -> SafetyCheck {
    if command.is_empty() {
        return SafetyCheck::Reject {
            reason: "empty command".to_string(),
        };
    }

    // Check if command is pre-approved
    if approved.contains(command) {
        return SafetyCheck::AutoApprove;
    }

    // Basic command safety
    if is_known_safe_command(command) {
        match approval_policy {
            ApprovalMode::FullAuto => SafetyCheck::AutoApprove,
            _ => SafetyCheck::AskUser,
        }
    } else {
        SafetyCheck::AskUser
    }
}

/// New improved command safety assessment with full codex-style approval system
pub fn assess_command_safety_v2(
    command: &[String],
    approval_manager: &ApprovalManager,
    sandbox_policy: &SandboxPolicy,
    with_escalated_permissions: bool,
) -> SafetyCheck {
    if command.is_empty() {
        return SafetyCheck::Reject {
            reason: "empty command".to_string(),
        };
    }

    // Check for dangerous commands regardless of approval policy
    if is_dangerous_command(command) && with_escalated_permissions {
        match sandbox_policy {
            SandboxPolicy::DangerFullAccess => {
                // Even in danger mode, ask for confirmation on destructive commands
                SafetyCheck::AskUser
            }
            _ => SafetyCheck::AskUser,
        }
    } else if approval_manager.needs_approval(command, with_escalated_permissions) {
        SafetyCheck::AskUser
    } else {
        SafetyCheck::AutoApprove
    }
}

fn is_dangerous_command(command: &[String]) -> bool {
    if command.is_empty() {
        return false;
    }

    let cmd = &command[0];
    let dangerous_commands = [
        "rm",
        "rmdir",
        "mv",
        "cp",
        "dd",
        "mkfs",
        "fdisk",
        "sudo",
        "su",
        "chmod",
        "chown",
        "kill",
        "killall",
        "halt",
        "reboot",
        "shutdown",
        "systemctl",
        "service",
        "mount",
        "umount",
        "format",
        "del",
        "erase",
    ];

    if dangerous_commands.contains(&cmd.as_str()) {
        return true;
    }

    // Check for dangerous patterns in the full command
    let full_command = command.join(" ");
    full_command.contains("rm -rf")
        || full_command.contains("--force")
        || full_command.contains("--recursive")
        || full_command.contains(">/dev/")
        || full_command.contains("2>/dev/null") && full_command.contains("rm")
}

fn is_known_safe_command(command: &[String]) -> bool {
    if command.is_empty() {
        return false;
    }

    let cmd = &command[0];
    matches!(
        cmd.as_str(),
        "ls" | "cat" | "grep" | "find" | "echo" | "pwd" | "whoami" | "date" | "which"
    )
}

// Legacy compatibility - define our own SafetyDecision for now
#[derive(Debug, Clone, PartialEq)]
pub enum SafetyDecision {
    AutoApprove,
    AskUser,
}

pub fn decide_command_safety(command: &str, network_allowed: bool) -> SafetyDecision {
    let cmd = command.trim();
    let safe = is_known_safe_command_str(cmd) && !cmd.contains(" rm ");
    if safe && !network_allowed {
        SafetyDecision::AutoApprove
    } else if safe {
        SafetyDecision::AskUser
    } else {
        SafetyDecision::AskUser
    }
}

fn is_known_safe_command_str(command: &str) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    let cmd = parts[0];
    matches!(
        cmd,
        "ls" | "cat" | "grep" | "find" | "echo" | "pwd" | "whoami" | "date" | "which"
    )
}
