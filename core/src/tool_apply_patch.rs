use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchTool {
    pub working_directory: PathBuf,
    pub dry_run: bool,
    pub create_backup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchInstruction {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub line_range: Option<(usize, usize)>,
    pub context_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplication {
    pub success: bool,
    pub message: String,
    pub diff_preview: String,
    pub backup_created: Option<String>,
    pub files_modified: Vec<String>,
}

impl Default for ApplyPatchTool {
    fn default() -> Self {
        Self {
            working_directory: std::env::current_dir().unwrap_or_default(),
            dry_run: false,
            create_backup: true,
        }
    }
}

impl ApplyPatchTool {
    pub fn new(working_directory: PathBuf) -> Self {
        Self {
            working_directory,
            dry_run: false,
            create_backup: true,
        }
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_backup(mut self, create_backup: bool) -> Self {
        self.create_backup = create_backup;
        self
    }

    pub async fn apply_patch(&self, instruction: PatchInstruction) -> Result<PatchApplication> {
        let file_path = self.working_directory.join(&instruction.file_path);

        // Validate the file exists
        if !file_path.exists() {
            return Ok(PatchApplication {
                success: false,
                message: format!("File does not exist: {}", instruction.file_path),
                diff_preview: String::new(),
                backup_created: None,
                files_modified: vec![],
            });
        }

        // Read current file content
        let current_content = std::fs::read_to_string(&file_path)?;

        // Validate the old content matches
        if !self.content_matches(&current_content, &instruction.old_content) {
            return Ok(PatchApplication {
                success: false,
                message: "File content has changed and doesn't match expected old content".to_string(),
                diff_preview: self.create_diff_preview(&instruction.old_content, &instruction.new_content),
                backup_created: None,
                files_modified: vec![],
            });
        }

        let diff_preview = self.create_diff_preview(&instruction.old_content, &instruction.new_content);

        // Dry run mode - don't actually modify files
        if self.dry_run {
            return Ok(PatchApplication {
                success: true,
                message: format!("DRY RUN: Would apply patch to {}", instruction.file_path),
                diff_preview,
                backup_created: None,
                files_modified: vec![instruction.file_path],
            });
        }

        // Create backup if requested
        let backup_path = if self.create_backup {
            let backup_file = format!("{}.backup", file_path.display());
            std::fs::copy(&file_path, &backup_file)?;
            Some(backup_file)
        } else {
            None
        };

        // Apply the patch by replacing old content with new content
        let updated_content = if let Some((start_line, end_line)) = instruction.line_range {
            self.apply_line_range_patch(&current_content, &instruction, start_line, end_line)?
        } else {
            current_content.replace(&instruction.old_content, &instruction.new_content)
        };

        // Write the updated content
        std::fs::write(&file_path, updated_content)?;

        Ok(PatchApplication {
            success: true,
            message: format!("Successfully applied patch to {}", instruction.file_path),
            diff_preview,
            backup_created: backup_path,
            files_modified: vec![instruction.file_path],
        })
    }

    pub async fn apply_multiple_patches(&self, instructions: Vec<PatchInstruction>) -> Result<Vec<PatchApplication>> {
        let mut results = Vec::new();
        let mut any_failed = false;

        for instruction in instructions {
            match self.apply_patch(instruction).await {
                Ok(result) => {
                    if !result.success {
                        any_failed = true;
                    }
                    results.push(result);
                },
                Err(e) => {
                    any_failed = true;
                    results.push(PatchApplication {
                        success: false,
                        message: format!("Error applying patch: {}", e),
                        diff_preview: String::new(),
                        backup_created: None,
                        files_modified: vec![],
                    });
                }
            }

            // If any patch fails and we're not in dry-run mode, consider stopping
            if any_failed && !self.dry_run {
                // You might want to implement a rollback mechanism here
                break;
            }
        }

        Ok(results)
    }

    pub async fn rollback_changes(&self, backup_files: Vec<String>) -> Result<Vec<String>> {
        let mut restored_files = Vec::new();

        for backup_file in backup_files {
            if !PathBuf::from(&backup_file).exists() {
                continue;
            }

            // Extract the original file path by removing .backup extension
            let original_file = backup_file.replace(".backup", "");
            let original_path = PathBuf::from(&original_file);

            if original_path.exists() {
                std::fs::copy(&backup_file, &original_path)?;
                std::fs::remove_file(&backup_file)?;
                restored_files.push(original_file);
            }
        }

        Ok(restored_files)
    }

    fn content_matches(&self, current: &str, expected: &str) -> bool {
        // Allow for whitespace differences
        let current_normalized = current.trim();
        let expected_normalized = expected.trim();

        if current_normalized == expected_normalized {
            return true;
        }

        // Check for partial matches (useful when old_content is just a portion)
        current_normalized.contains(expected_normalized)
    }

    fn apply_line_range_patch(&self, content: &str, instruction: &PatchInstruction, start_line: usize, end_line: usize) -> Result<String> {
        let lines: Vec<&str> = content.lines().collect();

        if start_line == 0 || start_line > lines.len() || end_line > lines.len() || start_line > end_line {
            return Err(anyhow!("Invalid line range: {}-{}", start_line, end_line));
        }

        // Convert to 0-based indexing
        let start_idx = start_line - 1;
        let end_idx = end_line; // end_line is exclusive in the range

        let mut result_lines = Vec::new();

        // Add lines before the replacement range
        result_lines.extend_from_slice(&lines[..start_idx]);

        // Add the new content (split into lines)
        for line in instruction.new_content.lines() {
            result_lines.push(line);
        }

        // Add lines after the replacement range
        if end_idx < lines.len() {
            result_lines.extend_from_slice(&lines[end_idx..]);
        }

        Ok(result_lines.join("\n"))
    }

    fn create_diff_preview(&self, old_content: &str, new_content: &str) -> String {
        let old_lines: Vec<&str> = old_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();

        let mut diff = String::new();
        diff.push_str("--- OLD\n");
        diff.push_str("+++ NEW\n");

        let max_lines = old_lines.len().max(new_lines.len());

        for i in 0..max_lines {
            let old_line = old_lines.get(i).unwrap_or(&"");
            let new_line = new_lines.get(i).unwrap_or(&"");

            if old_line != new_line {
                if !old_line.is_empty() {
                    diff.push_str(&format!("-{}\n", old_line));
                }
                if !new_line.is_empty() {
                    diff.push_str(&format!("+{}\n", new_line));
                }
            } else if !old_line.is_empty() {
                diff.push_str(&format!(" {}\n", old_line));
            }
        }

        diff
    }

    pub fn validate_patch_instruction(&self, instruction: &PatchInstruction) -> Result<()> {
        // Basic validation
        if instruction.file_path.is_empty() {
            return Err(anyhow!("File path cannot be empty"));
        }

        if instruction.old_content.is_empty() && instruction.new_content.is_empty() {
            return Err(anyhow!("Both old and new content cannot be empty"));
        }

        // Validate line range if specified
        if let Some((start, end)) = instruction.line_range {
            if start == 0 {
                return Err(anyhow!("Line numbers start at 1, not 0"));
            }
            if start > end {
                return Err(anyhow!("Start line {} cannot be greater than end line {}", start, end));
            }
        }

        // Check if file path is within working directory (security check)
        let file_path = self.working_directory.join(&instruction.file_path);
        let canonical_file = file_path.canonicalize().unwrap_or(file_path);
        let canonical_workspace = self.working_directory.canonicalize()
            .unwrap_or_else(|_| self.working_directory.clone());

        if !canonical_file.starts_with(&canonical_workspace) {
            return Err(anyhow!(
                "File path '{}' is outside the workspace",
                instruction.file_path
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_apply_patch_tool() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ApplyPatchTool::new(temp_dir.path().to_path_buf());

        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        let original_content = "fn main() {\n    println!(\"Hello, World!\");\n}";
        std::fs::write(&test_file, original_content).unwrap();

        // Create patch instruction
        let instruction = PatchInstruction {
            file_path: "test.rs".to_string(),
            old_content: "println!(\"Hello, World!\");".to_string(),
            new_content: "println!(\"Hello, Rust!\");".to_string(),
            line_range: None,
            context_lines: 3,
        };

        // Apply patch
        let result = tool.apply_patch(instruction).await.unwrap();

        assert!(result.success);
        assert_eq!(result.files_modified.len(), 1);

        // Verify the file was modified
        let updated_content = std::fs::read_to_string(&test_file).unwrap();
        assert!(updated_content.contains("Hello, Rust!"));
    }

    #[tokio::test]
    async fn test_dry_run_patch() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ApplyPatchTool::new(temp_dir.path().to_path_buf())
            .with_dry_run(true);

        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        let original_content = "fn main() {\n    println!(\"Hello, World!\");\n}";
        std::fs::write(&test_file, original_content).unwrap();

        // Create patch instruction
        let instruction = PatchInstruction {
            file_path: "test.rs".to_string(),
            old_content: "println!(\"Hello, World!\");".to_string(),
            new_content: "println!(\"Hello, Rust!\");".to_string(),
            line_range: None,
            context_lines: 3,
        };

        // Apply patch in dry-run mode
        let result = tool.apply_patch(instruction).await.unwrap();

        assert!(result.success);
        assert!(result.message.contains("DRY RUN"));

        // Verify the file was NOT modified
        let content_after = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content_after, original_content);
    }

    #[test]
    fn test_validate_patch_instruction() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ApplyPatchTool::new(temp_dir.path().to_path_buf());

        // Valid instruction
        let valid_instruction = PatchInstruction {
            file_path: "test.rs".to_string(),
            old_content: "old".to_string(),
            new_content: "new".to_string(),
            line_range: Some((1, 5)),
            context_lines: 3,
        };

        assert!(tool.validate_patch_instruction(&valid_instruction).is_ok());

        // Invalid instruction - empty file path
        let invalid_instruction = PatchInstruction {
            file_path: String::new(),
            old_content: "old".to_string(),
            new_content: "new".to_string(),
            line_range: None,
            context_lines: 3,
        };

        assert!(tool.validate_patch_instruction(&invalid_instruction).is_err());

        // Invalid instruction - invalid line range
        let invalid_line_range = PatchInstruction {
            file_path: "test.rs".to_string(),
            old_content: "old".to_string(),
            new_content: "new".to_string(),
            line_range: Some((5, 3)), // start > end
            context_lines: 3,
        };

        assert!(tool.validate_patch_instruction(&invalid_line_range).is_err());
    }
}