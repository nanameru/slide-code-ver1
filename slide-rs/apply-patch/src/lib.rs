use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchAction {
    pub files: Vec<ApplyPatchFileChange>,
    pub description: String,
}

impl ApplyPatchAction {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchFileChange {
    pub path: String,
    pub content: String,
    pub operation: String, // "add", "update", "delete"
}

/// Minimal patch applier supporting Add/Update/Delete blocks in the custom format.
/// This implementation is intentionally simple: Update replaces the entire file
/// with concatenated `+` lines from the first hunk. Context (` `) and removals (`-`)
/// are ignored. Good for generated files and new content.
pub fn apply_patch_to_files(patch: &str, _dry_run: bool) -> Result<()> {
    let mut lines = patch.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let content = collect_plus_lines_until_next_header(&mut lines);
            write_file(path.trim(), &content)?;
        } else if let Some(path) = line.strip_prefix("*** Update File: ") {
            // For simplicity, replace full file with `+` lines across all hunks
            let content = collect_plus_lines_until_next_header(&mut lines);
            write_file(path.trim(), &content)?;
        } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let p = PathBuf::from(path.trim());
            if p.exists() { fs::remove_file(&p).with_context(|| format!("delete: {}", p.display()))?; }
        }
    }
    Ok(())
}

fn collect_plus_lines_until_next_header<'a, I>(lines: &mut std::iter::Peekable<I>) -> String
where I: Iterator<Item = &'a str> {
    let mut out = String::new();
    while let Some(&l) = lines.peek() {
        if l.starts_with("*** ") { break; }
        if l.starts_with("+") {
            out.push_str(l.trim_start_matches('+'));
            out.push('\n');
        }
        lines.next();
    }
    out
}

fn write_file(path: &str, content: &str) -> Result<()> {
    let p = PathBuf::from(path);
    if let Some(parent) = p.parent() { fs::create_dir_all(parent)?; }
    fs::write(&p, content).with_context(|| format!("write: {}", p.display()))
}
