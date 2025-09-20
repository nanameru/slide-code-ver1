mod parser;
mod seek_sequence;
mod standalone_executable;

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::str::Utf8Error;

use anyhow::Context;
use anyhow::Result;
pub use parser::parse_patch;
pub use parser::Hunk;
pub use parser::ParseError;
use parser::ParseError::*;
use parser::UpdateFileChunk;
use similar::TextDiff;
use thiserror::Error;
use tree_sitter::LanguageError;
use tree_sitter::Parser;
use tree_sitter_bash::LANGUAGE as BASH;

pub use standalone_executable::main;

/// Detailed instructions for how to use the `apply_patch` tool.
pub const APPLY_PATCH_TOOL_INSTRUCTIONS: &str = include_str!("../apply_patch_tool_instructions.md");

const APPLY_PATCH_COMMANDS: [&str; 2] = ["apply_patch", "applypatch"];

#[derive(Debug, Error, PartialEq)]
pub enum ApplyPatchError {
    #[error(transparent)]
    ParseError(#[from] ParseError),
    #[error(transparent)]
    IoError(#[from] IoError),
    /// Error that occurs while computing replacements when applying patch chunks
    #[error("{0}")]
    ComputeReplacements(String),
}

impl From<std::io::Error> for ApplyPatchError {
    fn from(err: std::io::Error) -> Self {
        ApplyPatchError::IoError(IoError {
            context: "I/O error".to_string(),
            source: err,
        })
    }
}

impl From<&std::io::Error> for ApplyPatchError {
    fn from(err: &std::io::Error) -> Self {
        ApplyPatchError::IoError(IoError {
            context: "I/O error".to_string(),
            source: std::io::Error::new(err.kind(), err.to_string()),
        })
    }
}

#[derive(Debug, Error)]
#[error("{context}: {source}")]
pub struct IoError {
    context: String,
    #[source]
    source: std::io::Error,
}

impl PartialEq for IoError {
    fn eq(&self, other: &Self) -> bool {
        self.context == other.context && self.source.to_string() == other.source.to_string()
    }
}

#[derive(Debug, PartialEq)]
pub enum MaybeApplyPatch {
    Body(ApplyPatchArgs),
    ShellParseError(ExtractHeredocError),
    PatchParseError(ParseError),
    NotApplyPatch,
}

/// Both the raw PATCH argument to `apply_patch` as well as the PATCH argument
/// parsed into hunks.
#[derive(Debug, PartialEq)]
pub struct ApplyPatchArgs {
    pub patch: String,
    pub hunks: Vec<Hunk>,
}

pub fn maybe_parse_apply_patch(argv: &[String]) -> MaybeApplyPatch {
    match argv {
        [cmd, body] if APPLY_PATCH_COMMANDS.contains(&cmd.as_str()) => match parse_patch(body) {
            Ok(source) => MaybeApplyPatch::Body(source),
            Err(e) => MaybeApplyPatch::PatchParseError(e),
        },
        [bash, flag, script]
            if bash == "bash"
                && flag == "-lc"
                && APPLY_PATCH_COMMANDS
                    .iter()
                    .any(|cmd| script.trim_start().starts_with(cmd)) =>
        {
            match extract_heredoc_body_from_apply_patch_command(script) {
                Ok(body) => match parse_patch(&body) {
                    Ok(source) => MaybeApplyPatch::Body(source),
                    Err(e) => MaybeApplyPatch::PatchParseError(e),
                },
                Err(e) => MaybeApplyPatch::ShellParseError(e),
            }
        }
        _ => MaybeApplyPatch::NotApplyPatch,
    }
}

#[derive(Debug, PartialEq)]
pub enum ApplyPatchFileChange {
    Add {
        content: String,
    },
    Delete,
    Update {
        unified_diff: String,
        move_path: Option<PathBuf>,
        /// new_content that will result after the unified_diff is applied.
        new_content: String,
    },
}

#[derive(Debug, PartialEq)]
pub enum MaybeApplyPatchVerified {
    /// `argv` corresponded to an `apply_patch` invocation, and these are the
    /// resulting proposed file changes.
    Body(ApplyPatchAction),
    /// `argv` could not be parsed to determine whether it corresponds to an
    /// `apply_patch` invocation.
    ShellParseError(ExtractHeredocError),
    /// `argv` corresponded to an `apply_patch` invocation, but it could not
    /// be fulfilled due to the specified error.
    CorrectnessError(ApplyPatchError),
    /// `argv` decidedly did not correspond to an `apply_patch` invocation.
    NotApplyPatch,
}

/// ApplyPatchAction is the result of parsing an `apply_patch` command. By
/// construction, all paths should be absolute paths.
#[derive(Debug, PartialEq)]
pub struct ApplyPatchAction {
    changes: HashMap<PathBuf, ApplyPatchFileChange>,

    /// The raw patch argument that can be used with `apply_patch` as an exec
    /// call. i.e., if the original arg was parsed in "lenient" mode with a
    /// heredoc, this should be the value without the heredoc wrapper.
    pub patch: String,

    /// The working directory that was used to resolve relative paths in the patch.
    pub cwd: PathBuf,
}

impl ApplyPatchAction {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Returns the changes that would be made by applying the patch.
    pub fn changes(&self) -> &HashMap<PathBuf, ApplyPatchFileChange> {
        &self.changes
    }
}

/// cwd must be an absolute path so that we can resolve relative paths in the
/// patch.
pub fn maybe_parse_apply_patch_verified(argv: &[String], cwd: &Path) -> MaybeApplyPatchVerified {
    match maybe_parse_apply_patch(argv) {
        MaybeApplyPatch::Body(ApplyPatchArgs { patch, hunks }) => {
            let mut changes = HashMap::new();
            for hunk in hunks {
                let path = hunk.resolve_path(cwd);
                match hunk {
                    Hunk::AddFile { contents, .. } => {
                        changes.insert(path, ApplyPatchFileChange::Add { content: contents });
                    }
                    Hunk::DeleteFile { .. } => {
                        changes.insert(path, ApplyPatchFileChange::Delete);
                    }
                    Hunk::UpdateFile {
                        move_path, chunks, ..
                    } => {
                        let ApplyPatchFileUpdate {
                            unified_diff,
                            content: contents,
                        } = match unified_diff_from_chunks(&path, &chunks) {
                            Ok(diff) => diff,
                            Err(e) => {
                                return MaybeApplyPatchVerified::CorrectnessError(e);
                            }
                        };
                        changes.insert(
                            path,
                            ApplyPatchFileChange::Update {
                                unified_diff,
                                move_path: move_path.map(|p| cwd.join(p)),
                                new_content: contents,
                            },
                        );
                    }
                }
            }
            MaybeApplyPatchVerified::Body(ApplyPatchAction {
                changes,
                patch,
                cwd: cwd.to_path_buf(),
            })
        }
        MaybeApplyPatch::ShellParseError(e) => MaybeApplyPatchVerified::ShellParseError(e),
        MaybeApplyPatch::PatchParseError(e) => MaybeApplyPatchVerified::CorrectnessError(e.into()),
        MaybeApplyPatch::NotApplyPatch => MaybeApplyPatchVerified::NotApplyPatch,
    }
}

/// Attempts to extract a heredoc_body object from a string bash command.
fn extract_heredoc_body_from_apply_patch_command(
    src: &str,
) -> std::result::Result<String, ExtractHeredocError> {
    if !APPLY_PATCH_COMMANDS
        .iter()
        .any(|cmd| src.trim_start().starts_with(cmd))
    {
        return Err(ExtractHeredocError::CommandDidNotStartWithApplyPatch);
    }

    let lang = BASH.into();
    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .map_err(ExtractHeredocError::FailedToLoadBashGrammar)?;
    let tree = parser
        .parse(src, None)
        .ok_or(ExtractHeredocError::FailedToParsePatchIntoAst)?;

    let bytes = src.as_bytes();
    let mut c = tree.root_node().walk();

    loop {
        let node = c.node();
        if node.kind() == "heredoc_body" {
            let text = node
                .utf8_text(bytes)
                .map_err(ExtractHeredocError::HeredocNotUtf8)?;
            return Ok(text.trim_end_matches('\n').to_owned());
        }

        if c.goto_first_child() {
            continue;
        }
        while !c.goto_next_sibling() {
            if !c.goto_parent() {
                return Err(ExtractHeredocError::FailedToFindHeredocBody);
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ExtractHeredocError {
    CommandDidNotStartWithApplyPatch,
    FailedToLoadBashGrammar(LanguageError),
    HeredocNotUtf8(Utf8Error),
    FailedToParsePatchIntoAst,
    FailedToFindHeredocBody,
}

/// Applies the patch and prints the result to stdout/stderr.
pub fn apply_patch(
    patch: &str,
    stdout: &mut impl std::io::Write,
    stderr: &mut impl std::io::Write,
) -> Result<(), ApplyPatchError> {
    let hunks = match parse_patch(patch) {
        Ok(source) => source.hunks,
        Err(e) => {
            match &e {
                InvalidPatchError(message) => {
                    writeln!(stderr, "Invalid patch: {message}").map_err(ApplyPatchError::from)?;
                }
                InvalidHunkError {
                    message,
                    line_number,
                } => {
                    writeln!(
                        stderr,
                        "Invalid patch hunk on line {line_number}: {message}"
                    )
                    .map_err(ApplyPatchError::from)?;
                }
            }
            return Err(ApplyPatchError::ParseError(e));
        }
    };

    apply_hunks(&hunks, stdout, stderr)?;

    Ok(())
}

/// Applies hunks and continues to update stdout/stderr
pub fn apply_hunks(
    hunks: &[Hunk],
    stdout: &mut impl std::io::Write,
    stderr: &mut impl std::io::Write,
) -> Result<(), ApplyPatchError> {
    // (existing paths check omitted here; kept from original for parity)
    match apply_hunks_to_files(hunks) {
        Ok(affected) => {
            print_summary(&affected, stdout).map_err(ApplyPatchError::from)?;
            Ok(())
        }
        Err(err) => {
            let msg = err.to_string();
            writeln!(stderr, "{msg}").map_err(ApplyPatchError::from)?;
            if let Some(io) = err.downcast_ref::<std::io::Error>() {
                Err(ApplyPatchError::from(io))
            } else {
                Err(ApplyPatchError::IoError(IoError {
                    context: msg,
                    source: std::io::Error::other(err),
                }))
            }
        }
    }
}

pub struct AffectedPaths {
    pub added: Vec<PathBuf>,
    pub modified: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
}

fn apply_hunks_to_files(hunks: &[Hunk]) -> anyhow::Result<AffectedPaths> {
    if hunks.is_empty() {
        anyhow::bail!("No files were modified.");
    }

    let mut added: Vec<PathBuf> = Vec::new();
    let mut modified: Vec<PathBuf> = Vec::new();
    let mut deleted: Vec<PathBuf> = Vec::new();
    for hunk in hunks {
        match hunk {
            Hunk::AddFile { path, contents } => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent).with_context(|| {
                            format!("Failed to create parent directories for {}", path.display())
                        })?;
                    }
                }
                std::fs::write(path, contents)
                    .with_context(|| format!("Failed to write file {}", path.display()))?;
                added.push(path.clone());
            }
            Hunk::DeleteFile { path } => {
                std::fs::remove_file(path)
                    .with_context(|| format!("Failed to delete file {}", path.display()))?;
                deleted.push(path.clone());
            }
            Hunk::UpdateFile {
                path,
                move_path,
                chunks,
            } => {
                let AppliedPatch { new_contents, .. } =
                    derive_new_contents_from_chunks(path, chunks)?;
                if let Some(dest) = move_path {
                    if let Some(parent) = dest.parent() {
                        if !parent.as_os_str().is_empty() {
                            std::fs::create_dir_all(parent).with_context(|| {
                                format!(
                                    "Failed to create parent directories for {}",
                                    dest.display()
                                )
                            })?;
                        }
                    }
                    std::fs::write(dest, new_contents)
                        .with_context(|| format!("Failed to write file {}", dest.display()))?;
                    std::fs::remove_file(path)
                        .with_context(|| format!("Failed to remove original {}", path.display()))?;
                    modified.push(dest.clone());
                } else {
                    std::fs::write(path, new_contents)
                        .with_context(|| format!("Failed to write file {}", path.display()))?;
                    modified.push(path.clone());
                }
            }
        }
    }
    Ok(AffectedPaths {
        added,
        modified,
        deleted,
    })
}

struct AppliedPatch {
    original_contents: String,
    new_contents: String,
}

fn derive_new_contents_from_chunks(
    path: &Path,
    chunks: &[UpdateFileChunk],
) -> std::result::Result<AppliedPatch, ApplyPatchError> {
    let original_contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            return Err(ApplyPatchError::IoError(IoError {
                context: format!("Failed to read file to update {}", path.display()),
                source: err,
            }));
        }
    };

    let mut original_lines: Vec<String> = original_contents
        .split('\n')
        .map(|s| s.to_string())
        .collect();
    if original_lines.last().is_some_and(|s| s.is_empty()) {
        original_lines.pop();
    }

    let replacements = compute_replacements(&original_lines, path, chunks)?;
    let new_lines = apply_replacements(original_lines, &replacements);
    let mut new_lines = new_lines;
    if !new_lines.last().is_some_and(|s| s.is_empty()) {
        new_lines.push(String::new());
    }
    let new_contents = new_lines.join("\n");
    Ok(AppliedPatch {
        original_contents,
        new_contents,
    })
}

fn compute_replacements(
    original_lines: &[String],
    path: &Path,
    chunks: &[UpdateFileChunk],
) -> std::result::Result<Vec<(usize, usize, Vec<String>)>, ApplyPatchError> {
    let mut replacements: Vec<(usize, usize, Vec<String>)> = Vec::new();
    let mut line_index: usize = 0;

    for chunk in chunks {
        if let Some(ctx_line) = &chunk.change_context {
            if let Some(idx) = seek_sequence::seek_sequence(
                original_lines,
                std::slice::from_ref(ctx_line),
                line_index,
                false,
            ) {
                line_index = idx + 1;
            } else {
                return Err(ApplyPatchError::ComputeReplacements(format!(
                    "Failed to find context '{}' in {}",
                    ctx_line,
                    path.display()
                )));
            }
        }

        if chunk.old_lines.is_empty() {
            let insertion_idx = if original_lines.last().is_some_and(|s| s.is_empty()) {
                original_lines.len() - 1
            } else {
                original_lines.len()
            };
            replacements.push((insertion_idx, 0, chunk.new_lines.clone()));
            continue;
        }

        let mut pattern: &[String] = &chunk.old_lines;
        let mut found =
            seek_sequence::seek_sequence(original_lines, pattern, line_index, chunk.is_end_of_file);
        let mut new_slice: &[String] = &chunk.new_lines;

        if found.is_none() && pattern.last().is_some_and(|s| s.is_empty()) {
            pattern = &pattern[..pattern.len() - 1];
            if new_slice.last().is_some_and(|s| s.is_empty()) {
                new_slice = &new_slice[..new_slice.len() - 1];
            }
            found = seek_sequence::seek_sequence(
                original_lines,
                pattern,
                line_index,
                chunk.is_end_of_file,
            );
        }

        if let Some(start_idx) = found {
            replacements.push((start_idx, pattern.len(), new_slice.to_vec()));
            line_index = start_idx + pattern.len();
        } else {
            return Err(ApplyPatchError::ComputeReplacements(format!(
                "Failed to find expected lines {:?} in {}",
                chunk.old_lines,
                path.display()
            )));
        }
    }

    Ok(replacements)
}

fn apply_replacements(
    mut lines: Vec<String>,
    replacements: &[(usize, usize, Vec<String>)],
) -> Vec<String> {
    for (start_idx, old_len, new_segment) in replacements.iter().rev() {
        let start_idx = *start_idx;
        let old_len = *old_len;
        for _ in 0..old_len {
            if start_idx < lines.len() {
                lines.remove(start_idx);
            }
        }
        for (offset, new_line) in new_segment.iter().enumerate() {
            lines.insert(start_idx + offset, new_line.clone());
        }
    }
    lines
}

#[derive(Debug, Eq, PartialEq)]
pub struct ApplyPatchFileUpdate {
    unified_diff: String,
    content: String,
}

pub fn unified_diff_from_chunks(
    path: &Path,
    chunks: &[UpdateFileChunk],
) -> std::result::Result<ApplyPatchFileUpdate, ApplyPatchError> {
    unified_diff_from_chunks_with_context(path, chunks, 1)
}

pub fn unified_diff_from_chunks_with_context(
    path: &Path,
    chunks: &[UpdateFileChunk],
    context: usize,
) -> std::result::Result<ApplyPatchFileUpdate, ApplyPatchError> {
    let AppliedPatch {
        original_contents,
        new_contents,
    } = derive_new_contents_from_chunks(path, chunks)?;
    let text_diff = TextDiff::from_lines(&original_contents, &new_contents);
    let unified_diff = text_diff.unified_diff().context_radius(context).to_string();
    Ok(ApplyPatchFileUpdate {
        unified_diff,
        content: new_contents,
    })
}

pub fn print_summary(
    affected: &AffectedPaths,
    out: &mut impl std::io::Write,
) -> std::io::Result<()> {
    writeln!(out, "Success. Updated the following files:")?;
    for path in &affected.added {
        writeln!(out, "A {}", path.display())?;
    }
    for path in &affected.modified {
        writeln!(out, "M {}", path.display())?;
    }
    for path in &affected.deleted {
        writeln!(out, "D {}", path.display())?;
    }
    Ok(())
}

// removed legacy minimal applier; full implementation above
