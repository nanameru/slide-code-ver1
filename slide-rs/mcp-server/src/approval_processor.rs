//! 承認処理モジュール - パッチ適用とコマンド実行の承認を処理

use std::sync::Arc;
use anyhow::Result;
use mcp_types::RequestId;
use tracing::{info, warn, error};

use slide_core::SlideConversation;
use protocol::{
    ApplyPatchApprovalRequestEvent, ExecApprovalRequestEvent,
    ReviewDecision, Op, ApplyPatchApprovalResponseEvent,
    ExecApprovalResponseEvent
};

/// パッチ適用承認リクエストを処理
pub async fn handle_patch_approval(
    approval_event: ApplyPatchApprovalRequestEvent,
    conversation: Arc<SlideConversation>,
    _request_id: RequestId,
) {
    let ApplyPatchApprovalRequestEvent {
        call_id,
        reason,
        grant_root,
        changes,
    } = approval_event;

    info!("Processing patch approval request: call_id={}, reason={:?}", call_id, reason);

    // 実際の実装では、UIを通じてユーザーに承認を求める
    // ここでは簡単な自動承認ロジックを実装
    let decision = evaluate_patch_approval(&changes, grant_root.unwrap_or(false));

    info!("Patch approval decision: {:?}", decision);

    // 承認結果を送信
    let response = ApplyPatchApprovalResponseEvent {
        call_id,
        decision,
    };

    if let Err(e) = conversation.submit(Op::ApplyPatchApprovalResponse(response)).await {
        error!("Failed to send patch approval response: {}", e);
    }
}

/// コマンド実行承認リクエストを処理
pub async fn handle_exec_approval(
    approval_event: ExecApprovalRequestEvent,
    conversation: Arc<SlideConversation>,
    _request_id: RequestId,
) {
    let ExecApprovalRequestEvent {
        command,
        cwd,
        call_id,
        reason,
    } = approval_event;

    info!("Processing exec approval request: call_id={}, command={:?}, cwd={:?}", 
          call_id, command, cwd);

    // 実際の実装では、UIを通じてユーザーに承認を求める
    // ここでは簡単な自動承認ロジックを実装
    let decision = evaluate_exec_approval(&command, reason.as_deref());

    info!("Exec approval decision: {:?}", decision);

    // 承認結果を送信
    let response = ExecApprovalResponseEvent {
        call_id,
        decision,
    };

    if let Err(e) = conversation.submit(Op::ExecApprovalResponse(response)).await {
        error!("Failed to send exec approval response: {}", e);
    }
}

/// パッチ適用の自動承認ロジック
fn evaluate_patch_approval(
    changes: &[protocol::FileChange],
    grant_root: bool,
) -> ReviewDecision {
    // 基本的な安全性チェック
    if grant_root {
        warn!("Patch requests root privileges - requires manual approval");
        return ReviewDecision::Denied; // ルート権限が必要な場合は手動承認を要求
    }

    // ファイル変更の内容をチェック
    for change in changes {
        match change {
            protocol::FileChange::Add { path, .. } => {
                if is_sensitive_path(path) {
                    warn!("Patch attempts to add sensitive file: {}", path.display());
                    return ReviewDecision::Denied;
                }
            }
            protocol::FileChange::Update { path, .. } => {
                if is_sensitive_path(path) {
                    warn!("Patch attempts to update sensitive file: {}", path.display());
                    return ReviewDecision::Denied;
                }
            }
            protocol::FileChange::Delete { path } => {
                if is_sensitive_path(path) {
                    warn!("Patch attempts to delete sensitive file: {}", path.display());
                    return ReviewDecision::Denied;
                }
            }
        }
    }

    // 基本的な安全性チェックを通過した場合は承認
    info!("Patch approved automatically - {} changes", changes.len());
    ReviewDecision::Approved
}

/// コマンド実行の自動承認ロジック
fn evaluate_exec_approval(command: &[String], reason: Option<&str>) -> ReviewDecision {
    if command.is_empty() {
        return ReviewDecision::Denied;
    }

    let cmd = &command[0];
    
    // 安全なコマンドのホワイトリスト
    let safe_commands = [
        "ls", "cat", "grep", "find", "echo", "pwd", "whoami", "date", 
        "which", "head", "tail", "wc", "sort", "uniq", "file", "stat",
        "git", "cargo", "npm", "node", "python", "python3"
    ];

    if safe_commands.contains(&cmd.as_str()) {
        info!("Exec approved automatically - safe command: {}", cmd);
        return ReviewDecision::Approved;
    }

    // 危険なコマンドのブラックリスト
    let dangerous_commands = [
        "rm", "rmdir", "del", "delete", "format", "fdisk",
        "dd", "mkfs", "mount", "umount", "sudo", "su",
        "chmod", "chown", "kill", "killall", "shutdown", "reboot"
    ];

    if dangerous_commands.contains(&cmd.as_str()) {
        warn!("Exec denied - dangerous command: {}", cmd);
        return ReviewDecision::Denied;
    }

    // その他のコマンドは理由に基づいて判断
    if let Some(reason) = reason {
        if reason.contains("safe") || reason.contains("read-only") {
            info!("Exec approved based on reason: {}", reason);
            return ReviewDecision::Approved;
        }
    }

    // デフォルトは手動承認を要求
    info!("Exec requires manual approval - command: {}", cmd);
    ReviewDecision::Denied // 実際のUIでは AskUser に相当
}

/// パスが機密性の高いファイルかどうかをチェック
fn is_sensitive_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    
    // システムファイルやセキュリティ関連ファイル
    let sensitive_patterns = [
        "/etc/", "/sys/", "/proc/", "/dev/",
        "passwd", "shadow", "hosts", "sudoers",
        ".ssh/", ".env", "config.toml", "Cargo.toml"
    ];

    sensitive_patterns.iter().any(|pattern| path_str.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_evaluate_exec_approval_safe_commands() {
        assert_eq!(
            evaluate_exec_approval(&["ls".to_string()], None),
            ReviewDecision::Approved
        );
        
        assert_eq!(
            evaluate_exec_approval(&["git".to_string(), "status".to_string()], None),
            ReviewDecision::Approved
        );
    }

    #[test]
    fn test_evaluate_exec_approval_dangerous_commands() {
        assert_eq!(
            evaluate_exec_approval(&["rm".to_string(), "-rf".to_string()], None),
            ReviewDecision::Denied
        );
        
        assert_eq!(
            evaluate_exec_approval(&["sudo".to_string(), "rm".to_string()], None),
            ReviewDecision::Denied
        );
    }

    #[test]
    fn test_is_sensitive_path() {
        assert!(is_sensitive_path(&PathBuf::from("/etc/passwd")));
        assert!(is_sensitive_path(&PathBuf::from("~/.ssh/id_rsa")));
        assert!(!is_sensitive_path(&PathBuf::from("src/main.rs")));
        assert!(!is_sensitive_path(&PathBuf::from("README.md")));
    }
}
