use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct PatchApplier;

impl PatchApplier {
    pub fn new() -> Self {
        Self
    }

    pub fn apply_changes(&self, changes: &HashMap<PathBuf, String>) -> Result<()> {
        for (path, content) in changes {
            self.apply_single_change(path, content)?;
        }
        Ok(())
    }

    fn apply_single_change(&self, path: &Path, content: &str) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write the content to the file
        std::fs::write(path, content)?;
        
        println!("Applied patch to: {}", path.display());
        Ok(())
    }

    pub fn validate_changes(&self, changes: &HashMap<PathBuf, String>) -> Result<()> {
        for (path, content) in changes {
            if content.is_empty() {
                return Err(anyhow!("Empty content for path: {}", path.display()));
            }
        }
        Ok(())
    }
}