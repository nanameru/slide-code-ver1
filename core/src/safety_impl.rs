use slide_common::ApprovalMode;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum SafetyCheck {
    AutoApprove,
    AskUser,
    Reject { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxPolicy {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        SandboxPolicy::ReadOnly
    }
}

/// Assess the safety of a patch based on content and policies
pub fn assess_patch_safety(
    patch_content: &str,
    approval_mode: ApprovalMode,
    sandbox_policy: &SandboxPolicy,
    cwd: &Path,
) -> SafetyCheck {
    if patch_content.trim().is_empty() {
        return SafetyCheck::Reject {
            reason: "empty patch".to_string(),
        };
    }

    // Check for dangerous patterns
    let dangerous_patterns = [
        "rm -rf",
        "sudo",
        "chmod +x",
        "curl | sh",
        "wget | sh",
        "eval",
        "$(",
        "`",
        ">/dev/null",
        "2>&1",
    ];

    for pattern in &dangerous_patterns {
        if patch_content.contains(pattern) {
            return SafetyCheck::Reject {
                reason: format!("contains dangerous pattern: {}", pattern),
            };
        }
    }

    // Check if patch modifies files outside workspace
    if let Err(reason) = validate_patch_file_paths(patch_content, cwd) {
        return SafetyCheck::Reject { reason };
    }

    match approval_mode {
        ApprovalMode::FullAuto => SafetyCheck::AutoApprove,
        ApprovalMode::AutoEdit => {
            if is_simple_text_edit(patch_content) {
                SafetyCheck::AutoApprove
            } else {
                SafetyCheck::AskUser
            }
        }
        ApprovalMode::Suggest => SafetyCheck::AskUser,
    }
}

/// Assess the safety of a command based on content and policies
pub fn assess_command_safety(
    command: &[String],
    approval_policy: ApprovalMode,
    sandbox_policy: &SandboxPolicy,
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

    // Check for dangerous commands
    if is_dangerous_command(command) {
        return SafetyCheck::Reject {
            reason: format!("dangerous command: {}", command.join(" ")),
        };
    }

    // Check if command is known safe
    if is_known_safe_command(command) {
        match approval_policy {
            ApprovalMode::FullAuto => SafetyCheck::AutoApprove,
            _ => match sandbox_policy {
                SandboxPolicy::ReadOnly => SafetyCheck::AutoApprove,
                _ => SafetyCheck::AskUser,
            },
        }
    } else {
        SafetyCheck::AskUser
    }
}

fn is_known_safe_command(command: &[String]) -> bool {
    if command.is_empty() {
        return false;
    }
    
    let cmd = &command[0];
    let safe_commands = [
        "ls", "cat", "grep", "find", "echo", "pwd", "whoami", "date", "which",
        "head", "tail", "wc", "sort", "uniq", "cut", "awk", "sed",
        "git", "cargo", "npm", "node", "python", "python3",
        "mkdir", "touch", "cp", "mv",
    ];
    
    safe_commands.contains(&cmd.as_str())
}

fn is_dangerous_command(command: &[String]) -> bool {
    if command.is_empty() {
        return true;
    }
    
    let cmd = &command[0];
    let dangerous_commands = [
        "rm", "rmdir", "dd", "mkfs", "fdisk", "parted",
        "sudo", "su", "chmod", "chown", "chgrp",
        "systemctl", "service", "init", "shutdown", "reboot",
        "iptables", "ufw", "firewall-cmd",
        "curl", "wget", "nc", "netcat", "ssh", "scp", "rsync",
    ];
    
    // Check base command
    if dangerous_commands.contains(&cmd.as_str()) {
        return true;
    }
    
    // Check for dangerous flags
    let full_command = command.join(" ");
    let dangerous_patterns = [
        "rm -rf", "rm -r", "chmod +x", "| sh", "| bash",
        "> /dev/", ">/dev/", "2>&1", "&& rm", "; rm",
    ];
    
    for pattern in &dangerous_patterns {
        if full_command.contains(pattern) {
            return true;
        }
    }
    
    false
}

fn is_simple_text_edit(patch_content: &str) -> bool {
    // Simple heuristic: check if patch only contains text files
    let lines: Vec<&str> = patch_content.lines().collect();
    
    for line in lines {
        if line.starts_with("+++") || line.starts_with("---") {
            if let Some(filename) = line.split_whitespace().nth(1) {
                if is_binary_file(filename) {
                    return false;
                }
            }
        }
    }
    
    true
}

fn is_binary_file(filename: &str) -> bool {
    let binary_extensions = [
        ".exe", ".bin", ".so", ".dll", ".dylib", ".a", ".o",
        ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".ico",
        ".mp4", ".avi", ".mov", ".wmv", ".flv",
        ".mp3", ".wav", ".ogg", ".flac",
        ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z",
        ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ];
    
    for ext in &binary_extensions {
        if filename.ends_with(ext) {
            return true;
        }
    }
    
    false
}

fn validate_patch_file_paths(patch_content: &str, cwd: &Path) -> Result<(), String> {
    let lines: Vec<&str> = patch_content.lines().collect();
    
    for line in lines {
        if line.starts_with("+++") || line.starts_with("---") {
            if let Some(filename) = line.split_whitespace().nth(1) {
                // Skip /dev/null entries
                if filename == "/dev/null" {
                    continue;
                }
                
                let path = Path::new(filename);
                
                // Reject absolute paths outside workspace
                if path.is_absolute() && !path.starts_with(cwd) {
                    return Err(format!("file outside workspace: {}", filename));
                }
                
                // Reject paths with dangerous components
                for component in path.components() {
                    match component {
                        std::path::Component::ParentDir => {
                            return Err(format!("contains parent directory reference: {}", filename));
                        }
                        std::path::Component::Normal(name) => {
                            if name.to_string_lossy().starts_with('.') && name != "." {
                                // Allow .gitignore, .env etc., but be cautious
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_safe_commands() {
        assert!(is_known_safe_command(&["ls".to_string()]));
        assert!(is_known_safe_command(&["git".to_string(), "status".to_string()]));
        assert!(!is_known_safe_command(&["rm".to_string()]));
    }

    #[test]
    fn test_dangerous_commands() {
        assert!(is_dangerous_command(&["rm".to_string(), "-rf".to_string()]));
        assert!(is_dangerous_command(&["sudo".to_string()]));
        assert!(!is_dangerous_command(&["ls".to_string()]));
    }

    #[test]
    fn test_patch_safety() {
        let safe_patch = "--- a/test.txt\n+++ b/test.txt\n@@ -1,1 +1,1 @@\n-old line\n+new line\n";
        let cwd = PathBuf::from("/workspace");
        
        let result = assess_patch_safety(
            safe_patch,
            ApprovalMode::AutoEdit,
            &SandboxPolicy::WorkspaceWrite,
            &cwd,
        );
        
        matches!(result, SafetyCheck::AutoApprove);
    }
}