use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRequest {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub context_lines: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchResult {
    pub success: bool,
    pub message: String,
    pub diff: Option<String>,
    pub applied_hunks: usize,
    pub failed_hunks: usize,
}

#[derive(Debug, Clone)]
pub struct ApplyPatch {
    pub working_directory: PathBuf,
    pub dry_run: bool,
    pub context_lines: usize,
}

impl Default for ApplyPatch {
    fn default() -> Self {
        Self {
            working_directory: std::env::current_dir().unwrap_or_default(),
            dry_run: false,
            context_lines: 3,
        }
    }
}

impl ApplyPatch {
    pub fn new(working_directory: PathBuf) -> Self {
        Self {
            working_directory,
            dry_run: false,
            context_lines: 3,
        }
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_context_lines(mut self, context_lines: usize) -> Self {
        self.context_lines = context_lines;
        self
    }

    pub async fn apply_patch(&self, request: PatchRequest) -> Result<PatchResult> {
        let file_path = self.working_directory.join(&request.file_path);

        // Validate file exists and is readable
        if !file_path.exists() {
            return Ok(PatchResult {
                success: false,
                message: format!("File does not exist: {}", request.file_path),
                diff: None,
                applied_hunks: 0,
                failed_hunks: 1,
            });
        }

        let current_content = fs::read_to_string(&file_path)?;

        let diff_output = self.create_diff_preview(&request.old_content, &request.new_content);

        // Check if the old content matches current file content
        let similarity = self.calculate_similarity(&current_content, &request.old_content);

        if similarity < 0.8 {
            return Ok(PatchResult {
                success: false,
                message: format!(
                    "File content has changed significantly (similarity: {:.1}%). Manual merge required.",
                    similarity * 100.0
                ),
                diff: Some(diff_output),
                applied_hunks: 0,
                failed_hunks: 1,
            });
        }

        // Apply the patch
        let result = if self.dry_run {
            PatchResult {
                success: true,
                message: "Dry run: patch would be applied successfully".to_string(),
                diff: Some(diff_output),
                applied_hunks: 1,
                failed_hunks: 0,
            }
        } else {
            match fs::write(&file_path, &request.new_content) {
                Ok(_) => PatchResult {
                    success: true,
                    message: format!("Successfully applied patch to {}", request.file_path),
                    diff: Some(diff_output),
                    applied_hunks: 1,
                    failed_hunks: 0,
                },
                Err(e) => PatchResult {
                    success: false,
                    message: format!("Failed to write file: {}", e),
                    diff: Some(diff_output),
                    applied_hunks: 0,
                    failed_hunks: 1,
                },
            }
        };

        Ok(result)
    }

    pub async fn apply_multiple_patches(&self, patches: Vec<PatchRequest>) -> Result<Vec<PatchResult>> {
        let mut results = Vec::new();

        for patch in patches {
            let result = self.apply_patch(patch).await?;
            results.push(result);
        }

        Ok(results)
    }

    pub async fn create_backup(&self, file_path: &str) -> Result<String> {
        let source_path = self.working_directory.join(file_path);
        let backup_path = format!("{}.backup", source_path.to_string_lossy());

        fs::copy(&source_path, &backup_path)?;

        Ok(backup_path)
    }

    pub async fn restore_from_backup(&self, file_path: &str) -> Result<()> {
        let target_path = self.working_directory.join(file_path);
        let backup_path = format!("{}.backup", target_path.to_string_lossy());

        if Path::new(&backup_path).exists() {
            fs::copy(&backup_path, &target_path)?;
            fs::remove_file(&backup_path)?;
        }

        Ok(())
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

    fn calculate_similarity(&self, content1: &str, content2: &str) -> f64 {
        if content1 == content2 {
            return 1.0;
        }

        let lines1: Vec<&str> = content1.lines().collect();
        let lines2: Vec<&str> = content2.lines().collect();

        let total_lines = lines1.len().max(lines2.len());
        if total_lines == 0 {
            return 1.0;
        }

        let mut matching_lines = 0;
        let min_lines = lines1.len().min(lines2.len());

        for i in 0..min_lines {
            if lines1[i] == lines2[i] {
                matching_lines += 1;
            }
        }

        matching_lines as f64 / total_lines as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_apply_simple_patch() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let original_content = "line1\nline2\nline3\n";
        fs::write(&file_path, original_content).unwrap();

        let patcher = ApplyPatch::new(temp_dir.path().to_path_buf());

        let patch_request = PatchRequest {
            file_path: "test.txt".to_string(),
            old_content: original_content.to_string(),
            new_content: "line1\nmodified line2\nline3\n".to_string(),
            context_lines: Some(3),
        };

        let result = patcher.apply_patch(patch_request).await.unwrap();

        assert!(result.success);
        assert_eq!(result.applied_hunks, 1);
        assert_eq!(result.failed_hunks, 0);

        let updated_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(updated_content, "line1\nmodified line2\nline3\n");
    }

    #[tokio::test]
    async fn test_dry_run_patch() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let original_content = "line1\nline2\nline3\n";
        fs::write(&file_path, original_content).unwrap();

        let patcher = ApplyPatch::new(temp_dir.path().to_path_buf())
            .with_dry_run(true);

        let patch_request = PatchRequest {
            file_path: "test.txt".to_string(),
            old_content: original_content.to_string(),
            new_content: "line1\nmodified line2\nline3\n".to_string(),
            context_lines: Some(3),
        };

        let result = patcher.apply_patch(patch_request).await.unwrap();

        assert!(result.success);
        assert!(result.message.contains("Dry run"));

        // File should not be modified in dry run
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original_content);
    }
}