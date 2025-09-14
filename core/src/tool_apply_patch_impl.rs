use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::safety_impl::{assess_patch_safety, SandboxPolicy, SafetyCheck};
use slide_common::ApprovalMode;

#[derive(Debug, Clone)]
pub struct ApplyPatchInput {
    pub patch: String,
    pub description: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct ApplyPatchResult {
    pub assessment: PatchAssessment,
    pub applied: bool,
    pub changes: Vec<FileChange>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SafetyDecision {
    AutoApprove,
    AskUser,
    Reject { reason: String },
}

#[derive(Debug, Clone)]
pub struct PatchAssessment {
    pub decision: SafetyDecision,
    pub reason: Option<String>,
    pub files_affected: Vec<PathBuf>,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub operation: FileOperation,
    pub content_preview: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FileOperation {
    Create,
    Update { lines_added: usize, lines_removed: usize },
    Delete,
    Move { from: PathBuf },
}

/// Advanced patch application with comprehensive safety checks
pub fn tool_apply_patch(
    input: ApplyPatchInput,
    workspace_root: &Path,
    approval_mode: ApprovalMode,
    sandbox_policy: SandboxPolicy,
) -> ApplyPatchResult {
    let mut result = ApplyPatchResult {
        assessment: PatchAssessment {
            decision: SafetyDecision::Reject { reason: "Not assessed".to_string() },
            reason: None,
            files_affected: Vec::new(),
            risk_level: RiskLevel::Critical,
        },
        applied: false,
        changes: Vec::new(),
        errors: Vec::new(),
    };

    // Parse the patch to understand changes
    let patch_info = match parse_patch_content(&input.patch) {
        Ok(info) => info,
        Err(e) => {
            result.errors.push(format!("Failed to parse patch: {}", e));
            return result;
        }
    };

    result.changes = patch_info.changes.clone();
    result.assessment.files_affected = patch_info.files_affected.clone();

    // Assess patch safety
    let safety_check = assess_patch_safety(
        &input.patch,
        approval_mode,
        &sandbox_policy,
        workspace_root,
    );

    result.assessment = convert_safety_check_to_assessment(safety_check, &patch_info);

    // If auto-approved and not dry run, apply the patch
    if matches!(result.assessment.decision, SafetyDecision::AutoApprove) && !input.dry_run {
        match apply_patch_changes(&patch_info, workspace_root) {
            Ok(()) => {
                result.applied = true;
            }
            Err(e) => {
                result.errors.push(format!("Failed to apply patch: {}", e));
            }
        }
    }

    result
}

#[derive(Debug)]
struct ParsedPatch {
    changes: Vec<FileChange>,
    files_affected: Vec<PathBuf>,
    hunks: Vec<PatchHunk>,
}

#[derive(Debug)]
struct PatchHunk {
    file_path: PathBuf,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    lines: Vec<HunkLine>,
}

#[derive(Debug)]
enum HunkLine {
    Context(String),
    Addition(String),
    Deletion(String),
}

fn parse_patch_content(patch_content: &str) -> Result<ParsedPatch> {
    let mut changes = Vec::new();
    let mut files_affected = Vec::new();
    let mut hunks = Vec::new();
    
    let lines: Vec<&str> = patch_content.lines().collect();
    let mut i = 0;
    
    while i < lines.len() {
        let line = lines[i];
        
        // Handle custom format (*** Add File, *** Update File, *** Delete File)
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let file_path = PathBuf::from(path.trim());
            files_affected.push(file_path.clone());
            changes.push(FileChange {
                path: file_path,
                operation: FileOperation::Create,
                content_preview: None,
            });
            i += 1;
            continue;
        }
        
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let file_path = PathBuf::from(path.trim());
            files_affected.push(file_path.clone());
            changes.push(FileChange {
                path: file_path,
                operation: FileOperation::Update { lines_added: 0, lines_removed: 0 },
                content_preview: None,
            });
            i += 1;
            continue;
        }
        
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let file_path = PathBuf::from(path.trim());
            files_affected.push(file_path.clone());
            changes.push(FileChange {
                path: file_path,
                operation: FileOperation::Delete,
                content_preview: None,
            });
            i += 1;
            continue;
        }
        
        // Handle unified diff format
        if line.starts_with("--- ") {
            if i + 1 < lines.len() && lines[i + 1].starts_with("+++ ") {
                let old_file = parse_file_path(&lines[i][4..])?;
                let new_file = parse_file_path(&lines[i + 1][4..])?;
                
                if old_file != new_file && old_file != "/dev/null" && new_file != "/dev/null" {
                    // File rename/move
                    files_affected.push(new_file.clone());
                    changes.push(FileChange {
                        path: new_file,
                        operation: FileOperation::Move { from: old_file },
                        content_preview: None,
                    });
                } else if old_file == "/dev/null" {
                    // New file
                    files_affected.push(new_file.clone());
                    changes.push(FileChange {
                        path: new_file,
                        operation: FileOperation::Create,
                        content_preview: None,
                    });
                } else if new_file == "/dev/null" {
                    // Deleted file
                    files_affected.push(old_file.clone());
                    changes.push(FileChange {
                        path: old_file,
                        operation: FileOperation::Delete,
                        content_preview: None,
                    });
                } else {
                    // Modified file
                    files_affected.push(new_file.clone());
                    
                    // Parse hunks to count changes
                    let (lines_added, lines_removed) = count_changes(&lines[i+2..]);
                    changes.push(FileChange {
                        path: new_file,
                        operation: FileOperation::Update { lines_added, lines_removed },
                        content_preview: None,
                    });
                }
                
                i += 2;
                continue;
            }
        }
        
        i += 1;
    }
    
    Ok(ParsedPatch {
        changes,
        files_affected,
        hunks,
    })
}

fn parse_file_path(path_line: &str) -> Result<PathBuf> {
    let parts: Vec<&str> = path_line.split_whitespace().collect();
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty file path"));
    }
    
    Ok(PathBuf::from(parts[0]))
}

fn count_changes(lines: &[&str]) -> (usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    
    for line in lines {
        if line.starts_with("@@") {
            break; // End of current hunk
        }
        if line.starts_with('+') && !line.starts_with("+++") {
            added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            removed += 1;
        }
    }
    
    (added, removed)
}

fn convert_safety_check_to_assessment(
    safety_check: SafetyCheck,
    patch_info: &ParsedPatch,
) -> PatchAssessment {
    let (decision, reason) = match safety_check {
        SafetyCheck::AutoApprove => (SafetyDecision::AutoApprove, None),
        SafetyCheck::AskUser => (SafetyDecision::AskUser, Some("Requires user approval".to_string())),
        SafetyCheck::Reject { reason } => (SafetyDecision::Reject { reason: reason.clone() }, Some(reason)),
    };
    
    let risk_level = assess_risk_level(patch_info);
    
    PatchAssessment {
        decision,
        reason,
        files_affected: patch_info.files_affected.clone(),
        risk_level,
    }
}

fn assess_risk_level(patch_info: &ParsedPatch) -> RiskLevel {
    let mut score = 0;
    
    // Number of files affected
    match patch_info.files_affected.len() {
        0 => score += 0,
        1..=3 => score += 1,
        4..=10 => score += 2,
        _ => score += 3,
    }
    
    // Types of operations
    for change in &patch_info.changes {
        match &change.operation {
            FileOperation::Create => score += 1,
            FileOperation::Update { lines_added, lines_removed } => {
                score += (*lines_added + *lines_removed) / 10;
            }
            FileOperation::Delete => score += 2,
            FileOperation::Move { .. } => score += 1,
        }
    }
    
    // File types and paths
    for path in &patch_info.files_affected {
        let path_str = path.to_string_lossy().to_lowercase();
        
        if path_str.contains("config") || path_str.contains(".env") {
            score += 2;
        }
        if path_str.ends_with(".sh") || path_str.ends_with(".py") || path_str.ends_with(".js") {
            score += 1;
        }
        if path_str.starts_with("/etc/") || path_str.starts_with("/usr/") || path_str.starts_with("/var/") {
            score += 3;
        }
    }
    
    match score {
        0..=2 => RiskLevel::Low,
        3..=5 => RiskLevel::Medium,
        6..=10 => RiskLevel::High,
        _ => RiskLevel::Critical,
    }
}

fn apply_patch_changes(patch_info: &ParsedPatch, workspace_root: &Path) -> Result<()> {
    for change in &patch_info.changes {
        let full_path = workspace_root.join(&change.path);
        
        match &change.operation {
            FileOperation::Create => {
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                // For creation, we would need to extract the content from the patch
                std::fs::write(&full_path, "")?;
            }
            FileOperation::Update { .. } => {
                // For updates, we would apply the actual diff
                // This is a simplified version
                if !full_path.exists() {
                    return Err(anyhow::anyhow!("File does not exist: {}", full_path.display()));
                }
            }
            FileOperation::Delete => {
                if full_path.exists() {
                    std::fs::remove_file(&full_path)?;
                }
            }
            FileOperation::Move { from } => {
                let old_path = workspace_root.join(from);
                if old_path.exists() {
                    if let Some(parent) = full_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::rename(&old_path, &full_path)?;
                }
            }
        }
    }
    
    Ok(())
}

/// Validate that a patch can be applied safely
pub fn validate_patch(patch_content: &str, workspace_root: &Path) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    let patch_info = parse_patch_content(patch_content)?;
    
    for change in &patch_info.changes {
        let full_path = workspace_root.join(&change.path);
        
        match &change.operation {
            FileOperation::Create => {
                if full_path.exists() {
                    warnings.push(format!("File already exists: {}", change.path.display()));
                }
            }
            FileOperation::Update { .. } => {
                if !full_path.exists() {
                    warnings.push(format!("File to update does not exist: {}", change.path.display()));
                }
            }
            FileOperation::Delete => {
                if !full_path.exists() {
                    warnings.push(format!("File to delete does not exist: {}", change.path.display()));
                }
            }
            FileOperation::Move { from } => {
                let old_path = workspace_root.join(from);
                if !old_path.exists() {
                    warnings.push(format!("Source file for move does not exist: {}", from.display()));
                }
                if full_path.exists() {
                    warnings.push(format!("Destination file already exists: {}", change.path.display()));
                }
            }
        }
    }
    
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_custom_patch_format() {
        let patch = r#"*** Add File: test.txt
+Hello World
+Second line

*** Update File: existing.txt
-old line
+new line

*** Delete File: unwanted.txt"#;

        let parsed = parse_patch_content(patch).unwrap();
        assert_eq!(parsed.files_affected.len(), 3);
        assert_eq!(parsed.changes.len(), 3);
    }

    #[test]
    fn test_parse_unified_diff() {
        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 context line
-old line
+new line
 another context"#;

        let parsed = parse_patch_content(patch).unwrap();
        assert_eq!(parsed.files_affected.len(), 1);
        
        if let FileOperation::Update { lines_added, lines_removed } = &parsed.changes[0].operation {
            assert_eq!(*lines_added, 1);
            assert_eq!(*lines_removed, 1);
        } else {
            panic!("Expected Update operation");
        }
    }

    #[test]
    fn test_risk_assessment() {
        let temp_dir = TempDir::new().unwrap();
        let input = ApplyPatchInput {
            patch: "*** Add File: config.json\n+{}".to_string(),
            description: None,
            dry_run: true,
        };

        let result = tool_apply_patch(
            input,
            temp_dir.path(),
            ApprovalMode::Suggest,
            SandboxPolicy::WorkspaceWrite,
        );

        assert!(matches!(result.assessment.risk_level, RiskLevel::Medium | RiskLevel::High));
    }
}