use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOperationRequest {
    pub operation: FileOperation,
    pub path: String,
    pub content: Option<String>,
    pub backup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileOperation {
    Read,
    Write,
    Create,
    Delete,
    Move { destination: String },
    Copy { destination: String },
    Search { pattern: String },
    List,
    Backup,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOperationResult {
    pub success: bool,
    pub message: String,
    pub content: Option<String>,
    pub files: Option<Vec<String>>,
    pub metadata: Option<FileMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: String,
    pub is_directory: bool,
    pub permissions: String,
}

pub struct FileOperationManager {
    pub working_directory: PathBuf,
    pub allow_outside_workspace: bool,
    pub max_file_size: usize,
}

impl Default for FileOperationManager {
    fn default() -> Self {
        Self {
            working_directory: std::env::current_dir().unwrap_or_default(),
            allow_outside_workspace: false,
            max_file_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

impl FileOperationManager {
    pub fn new(working_directory: PathBuf) -> Self {
        Self {
            working_directory,
            allow_outside_workspace: false,
            max_file_size: 10 * 1024 * 1024,
        }
    }

    pub fn with_max_file_size(mut self, max_size: usize) -> Self {
        self.max_file_size = max_size;
        self
    }

    pub fn allow_outside_workspace(mut self, allow: bool) -> Self {
        self.allow_outside_workspace = allow;
        self
    }

    fn validate_path(&self, path: &str) -> Result<PathBuf> {
        let full_path = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.working_directory.join(path)
        };

        let canonical_path = full_path.canonicalize().unwrap_or(full_path.clone());

        if !self.allow_outside_workspace {
            let canonical_workspace = self.working_directory.canonicalize()
                .unwrap_or_else(|_| self.working_directory.clone());

            if !canonical_path.starts_with(&canonical_workspace) {
                return Err(anyhow!(
                    "Path '{}' is outside the workspace '{}'. Access denied.",
                    path,
                    canonical_workspace.display()
                ));
            }
        }

        Ok(canonical_path)
    }

    pub async fn execute_operation(&self, request: FileOperationRequest) -> Result<FileOperationResult> {
        let path = self.validate_path(&request.path)?;

        match request.operation {
            FileOperation::Read => self.read_file(&path).await,
            FileOperation::Write => {
                let content = request.content
                    .ok_or_else(|| anyhow!("Content required for write operation"))?;
                self.write_file(&path, &content, request.backup).await
            },
            FileOperation::Create => {
                let content = request.content.unwrap_or_default();
                self.create_file(&path, &content).await
            },
            FileOperation::Delete => self.delete_file(&path).await,
            FileOperation::Move { destination } => {
                let dest_path = self.validate_path(&destination)?;
                self.move_file(&path, &dest_path).await
            },
            FileOperation::Copy { destination } => {
                let dest_path = self.validate_path(&destination)?;
                self.copy_file(&path, &dest_path).await
            },
            FileOperation::Search { pattern } => {
                self.search_files(&path, &pattern).await
            },
            FileOperation::List => self.list_directory(&path).await,
            FileOperation::Backup => self.backup_file(&path).await,
            FileOperation::Restore => self.restore_file(&path).await,
        }
    }

    async fn read_file(&self, path: &Path) -> Result<FileOperationResult> {
        if !path.exists() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("File does not exist: {}", path.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        let metadata = path.metadata()?;
        if metadata.len() > self.max_file_size as u64 {
            return Ok(FileOperationResult {
                success: false,
                message: format!(
                    "File too large: {} bytes (max: {} bytes)",
                    metadata.len(),
                    self.max_file_size
                ),
                content: None,
                files: None,
                metadata: None,
            });
        }

        let content = fs::read_to_string(path)?;
        let file_metadata = self.get_file_metadata(path)?;

        Ok(FileOperationResult {
            success: true,
            message: format!("Successfully read file: {}", path.display()),
            content: Some(content),
            files: None,
            metadata: Some(file_metadata),
        })
    }

    async fn write_file(&self, path: &Path, content: &str, backup: bool) -> Result<FileOperationResult> {
        if backup && path.exists() {
            self.create_backup(path).await?;
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, content)?;
        let file_metadata = self.get_file_metadata(path)?;

        Ok(FileOperationResult {
            success: true,
            message: format!("Successfully wrote file: {}", path.display()),
            content: None,
            files: None,
            metadata: Some(file_metadata),
        })
    }

    async fn create_file(&self, path: &Path, content: &str) -> Result<FileOperationResult> {
        if path.exists() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("File already exists: {}", path.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, content)?;
        let file_metadata = self.get_file_metadata(path)?;

        Ok(FileOperationResult {
            success: true,
            message: format!("Successfully created file: {}", path.display()),
            content: None,
            files: None,
            metadata: Some(file_metadata),
        })
    }

    async fn delete_file(&self, path: &Path) -> Result<FileOperationResult> {
        if !path.exists() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("File does not exist: {}", path.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }

        Ok(FileOperationResult {
            success: true,
            message: format!("Successfully deleted: {}", path.display()),
            content: None,
            files: None,
            metadata: None,
        })
    }

    async fn move_file(&self, source: &Path, destination: &Path) -> Result<FileOperationResult> {
        if !source.exists() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("Source file does not exist: {}", source.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        // Create parent directories for destination if they don't exist
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::rename(source, destination)?;
        let file_metadata = self.get_file_metadata(destination)?;

        Ok(FileOperationResult {
            success: true,
            message: format!("Successfully moved {} to {}", source.display(), destination.display()),
            content: None,
            files: None,
            metadata: Some(file_metadata),
        })
    }

    async fn copy_file(&self, source: &Path, destination: &Path) -> Result<FileOperationResult> {
        if !source.exists() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("Source file does not exist: {}", source.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        // Create parent directories for destination if they don't exist
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        if source.is_dir() {
            self.copy_directory(source, destination).await?;
        } else {
            fs::copy(source, destination)?;
        }

        let file_metadata = self.get_file_metadata(destination)?;

        Ok(FileOperationResult {
            success: true,
            message: format!("Successfully copied {} to {}", source.display(), destination.display()),
            content: None,
            files: None,
            metadata: Some(file_metadata),
        })
    }

    async fn copy_directory(&self, source: &Path, destination: &Path) -> Result<()> {
        fs::create_dir_all(destination)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let source_path = source.join(&file_name);
            let dest_path = destination.join(&file_name);

            if source_path.is_dir() {
                // Use Box::pin to handle recursive async calls
                Box::pin(self.copy_directory(&source_path, &dest_path)).await?;
            } else {
                fs::copy(&source_path, &dest_path)?;
            }
        }

        Ok(())
    }

    async fn search_files(&self, directory: &Path, pattern: &str) -> Result<FileOperationResult> {
        if !directory.exists() || !directory.is_dir() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("Directory does not exist: {}", directory.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        let mut matching_files = Vec::new();
        self.search_recursive(directory, pattern, &mut matching_files)?;

        Ok(FileOperationResult {
            success: true,
            message: format!("Found {} files matching pattern '{}'", matching_files.len(), pattern),
            content: None,
            files: Some(matching_files),
            metadata: None,
        })
    }

    fn search_recursive(&self, directory: &Path, pattern: &str, results: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.search_recursive(&path, pattern, results)?;
            } else if let Some(file_name) = path.file_name() {
                if file_name.to_string_lossy().contains(pattern) {
                    results.push(path.to_string_lossy().to_string());
                }
            }
        }
        Ok(())
    }

    async fn list_directory(&self, path: &Path) -> Result<FileOperationResult> {
        if !path.exists() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("Directory does not exist: {}", path.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        if !path.is_dir() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("Path is not a directory: {}", path.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        let mut files = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            files.push(path.to_string_lossy().to_string());
        }

        files.sort();

        Ok(FileOperationResult {
            success: true,
            message: format!("Listed {} items in directory", files.len()),
            content: None,
            files: Some(files),
            metadata: None,
        })
    }

    async fn backup_file(&self, path: &Path) -> Result<FileOperationResult> {
        if !path.exists() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("File does not exist: {}", path.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        let backup_path = self.create_backup(path).await?;

        Ok(FileOperationResult {
            success: true,
            message: format!("Successfully created backup: {}", backup_path.display()),
            content: None,
            files: None,
            metadata: None,
        })
    }

    async fn restore_file(&self, path: &Path) -> Result<FileOperationResult> {
        let backup_path = PathBuf::from(format!("{}.backup", path.display()));

        if !backup_path.exists() {
            return Ok(FileOperationResult {
                success: false,
                message: format!("Backup file does not exist: {}", backup_path.display()),
                content: None,
                files: None,
                metadata: None,
            });
        }

        fs::copy(&backup_path, path)?;
        fs::remove_file(&backup_path)?;

        Ok(FileOperationResult {
            success: true,
            message: format!("Successfully restored file from backup: {}", path.display()),
            content: None,
            files: None,
            metadata: None,
        })
    }

    async fn create_backup(&self, path: &Path) -> Result<PathBuf> {
        let backup_path = PathBuf::from(format!("{}.backup", path.display()));
        fs::copy(path, &backup_path)?;
        Ok(backup_path)
    }

    fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = path.metadata()?;
        let modified = metadata.modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        Ok(FileMetadata {
            size: metadata.len(),
            modified: format!("{}", modified),
            is_directory: metadata.is_dir(),
            permissions: "755".to_string(), // Simplified for cross-platform compatibility
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let manager = FileOperationManager::new(temp_dir.path().to_path_buf());

        // Test create file
        let create_request = FileOperationRequest {
            operation: FileOperation::Create,
            path: "test.txt".to_string(),
            content: Some("Hello, World!".to_string()),
            backup: false,
        };

        let result = manager.execute_operation(create_request).await.unwrap();
        assert!(result.success);

        // Test read file
        let read_request = FileOperationRequest {
            operation: FileOperation::Read,
            path: "test.txt".to_string(),
            content: None,
            backup: false,
        };

        let result = manager.execute_operation(read_request).await.unwrap();
        assert!(result.success);
        assert_eq!(result.content.unwrap(), "Hello, World!");
    }
}