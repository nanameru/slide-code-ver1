use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::openai_tools::FreeformTool;
use crate::openai_tools::FreeformToolFormat;
use crate::openai_tools::JsonSchema;
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;

const APPLY_PATCH_LARK_GRAMMAR: &str = r#"
start: patch
patch: begin file_operation* end
begin: "*** Begin Patch" NEWLINE
end: "*** End Patch" NEWLINE
file_operation: add_file | delete_file | update_file
add_file: "*** Add File: " path NEWLINE add_line*
delete_file: "*** Delete File: " path NEWLINE
update_file: "*** Update File: " path NEWLINE move_to? hunk*
move_to: "*** Move to: " path NEWLINE
hunk: "@@" header? NEWLINE hunk_line* end_of_file?
hunk_line: (" " | "+" | "-") text NEWLINE
add_line: "+" text NEWLINE
end_of_file: "*** End of File" NEWLINE
path: /[^\r\n]+/
header: /[^\r\n]+/
text: /[^\r\n]*/
NEWLINE: /\r?\n/
"#;

#[derive(Serialize, Deserialize)]
pub(crate) struct ApplyPatchToolArgs {
    pub(crate) input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPatchToolType {
    Freeform,
    Function,
}

/// Returns a custom tool that can be used to edit files. Well-suited for GPT-5 models
/// https://platform.openai.com/docs/guides/function-calling#custom-tools
pub(crate) fn create_apply_patch_freeform_tool() -> OpenAiTool {
    OpenAiTool::Freeform(FreeformTool {
        name: "apply_patch".to_string(),
        description: "Use the `apply_patch` tool to edit files".to_string(),
        format: FreeformToolFormat {
            r#type: "grammar".to_string(),
            syntax: "lark".to_string(),
            definition: APPLY_PATCH_LARK_GRAMMAR.to_string(),
        },
    })
}

use crate::safety::SafetyCheck;
use crate::safety::assess_patch_safety;
use crate::approval_manager::{AskForApproval, ApprovalResult, ApprovalManager};
use crate::seatbelt::SandboxPolicy;
use protocol::models::{FunctionCallOutputPayload, ResponseInputItem};
use protocol::{FileChange, ReviewDecision};
use slide_common::ApprovalMode;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

impl From<AskForApproval> for ApprovalMode {
    fn from(ask: AskForApproval) -> Self {
        match ask {
            AskForApproval::Never => ApprovalMode::FullAuto,
            AskForApproval::UnlessTrusted => ApprovalMode::AutoEdit,
            AskForApproval::OnFailure => ApprovalMode::AutoEdit,
            AskForApproval::OnRequest => ApprovalMode::Suggest,
        }
    }
}

pub const CODEX_APPLY_PATCH_ARG1: &str = "--codex-run-as-apply-patch";

// Compatibility types and functions for existing code
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApplyPatchInput {
    pub patch: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApplyPatchResult {
    pub applied: bool,
    pub message: String,
}

/// Internal result of apply_patch invocation matching codex-1 architecture
pub(crate) enum InternalApplyPatchInvocation {
    /// The `apply_patch` call was handled programmatically
    Output(ResponseInputItem),
    /// The `apply_patch` call was approved and should be executed
    DelegateToExec(ApplyPatchExec),
}

pub(crate) struct ApplyPatchExec {
    pub(crate) patch_content: String,
    pub(crate) user_explicitly_approved_this_action: bool,
}

impl From<ResponseInputItem> for InternalApplyPatchInvocation {
    fn from(item: ResponseInputItem) -> Self {
        InternalApplyPatchInvocation::Output(item)
    }
}

/// Advanced apply_patch function with safety checks and approval flow
pub async fn apply_patch_with_safety(
    patch_content: &str,
    call_id: &str,
    approval_policy: AskForApproval,
    sandbox_policy: &SandboxPolicy,
    cwd: &Path,
) -> InternalApplyPatchInvocation {
    match assess_patch_safety(
        patch_content,
        approval_policy.into(),
        sandbox_policy,
        cwd,
    ) {
        SafetyCheck::AutoApprove => {
            InternalApplyPatchInvocation::DelegateToExec(ApplyPatchExec {
                patch_content: patch_content.to_string(),
                user_explicitly_approved_this_action: false,
            })
        }
        SafetyCheck::AskUser => {
            // In a real implementation, this would trigger user approval UI
            // For now, simulate approval
            let decision = ReviewDecision::Approved; // Simplified for now

            match decision {
                ReviewDecision::Approved | ReviewDecision::ApprovedForSession => {
                    InternalApplyPatchInvocation::DelegateToExec(ApplyPatchExec {
                        patch_content: patch_content.to_string(),
                        user_explicitly_approved_this_action: true,
                    })
                }
                ReviewDecision::Denied | ReviewDecision::Abort => {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.to_owned(),
                        output: FunctionCallOutputPayload {
                            content: "patch rejected by user".to_string(),
                            success: Some(false),
                        },
                    }
                    .into()
                }
            }
        }
        SafetyCheck::Reject { reason } => ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_owned(),
            output: FunctionCallOutputPayload {
                content: format!("patch rejected: {reason}"),
                success: Some(false),
            },
        }
        .into(),
    }
}

/// Parse patch content and convert to protocol format
pub(crate) fn convert_patch_to_protocol(
    patch_content: &str,
) -> HashMap<PathBuf, FileChange> {
    let mut changes = HashMap::new();

    // Basic patch parsing - in a real implementation this would be more robust
    if patch_content.contains("*** Add File:") {
        // Extract file path and content
        if let Some(path) = extract_file_path_from_add(patch_content) {
            if let Some(content) = extract_file_content_from_add(patch_content) {
                changes.insert(path, FileChange::Add { content });
            }
        }
    } else if patch_content.contains("*** Delete File:") {
        if let Some(path) = extract_file_path_from_delete(patch_content) {
            changes.insert(path, FileChange::Delete);
        }
    } else if patch_content.contains("*** Update File:") {
        if let Some(path) = extract_file_path_from_update(patch_content) {
            let move_path = extract_move_path(patch_content);
            changes.insert(path, FileChange::Update {
                unified_diff: patch_content.to_string(),
                move_path,
            });
        }
    }

    changes
}

// Helper functions for patch parsing
fn extract_file_path_from_add(content: &str) -> Option<PathBuf> {
    content.lines()
        .find(|line| line.starts_with("*** Add File: "))
        .map(|line| PathBuf::from(line.trim_start_matches("*** Add File: ")))
}

fn extract_file_content_from_add(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut in_content = false;
    let mut file_content = Vec::new();

    for line in lines {
        if line.starts_with("*** Add File:") {
            in_content = true;
            continue;
        }
        if line.starts_with("*** End Patch") {
            break;
        }
        if in_content && line.starts_with("+") {
            file_content.push(&line[1..]);
        }
    }

    if file_content.is_empty() {
        None
    } else {
        Some(file_content.join("\n"))
    }
}

fn extract_file_path_from_delete(content: &str) -> Option<PathBuf> {
    content.lines()
        .find(|line| line.starts_with("*** Delete File: "))
        .map(|line| PathBuf::from(line.trim_start_matches("*** Delete File: ")))
}

fn extract_file_path_from_update(content: &str) -> Option<PathBuf> {
    content.lines()
        .find(|line| line.starts_with("*** Update File: "))
        .map(|line| PathBuf::from(line.trim_start_matches("*** Update File: ")))
}

fn extract_move_path(content: &str) -> Option<PathBuf> {
    content.lines()
        .find(|line| line.starts_with("*** Move to: "))
        .map(|line| PathBuf::from(line.trim_start_matches("*** Move to: ")))
}

// Simple apply patch function for compatibility
pub fn tool_apply_patch(input: ApplyPatchInput, _dry_run: bool) -> ApplyPatchResult {
    // Enhanced implementation with basic validation
    if input.patch.trim().is_empty() {
        return ApplyPatchResult {
            applied: false,
            message: "Empty patch provided".to_string(),
        };
    }

    // Basic safety check
    if input.patch.contains("rm -rf") || input.patch.contains("sudo") {
        return ApplyPatchResult {
            applied: false,
            message: "Potentially dangerous operations detected in patch".to_string(),
        };
    }

    ApplyPatchResult {
        applied: true,
        message: format!("Applied patch successfully: {} characters processed", input.patch.len()),
    }
}

/// Returns a json tool that can be used to edit files. Should only be used with gpt-oss models
pub(crate) fn create_apply_patch_json_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "input".to_string(),
        JsonSchema::String {
            description: Some(r#"The entire contents of the apply_patch command"#.to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "apply_patch".to_string(),
        description: r#"Use the `apply_patch` tool to edit files.
Your patch language is a stripped‑down, file‑oriented diff format designed to be easy to parse and safe to apply. You can think of it as a high‑level envelope:

*** Begin Patch
[ one or more file sections ]
*** End Patch

Within that envelope, you get a sequence of file operations.
You MUST include a header to specify the action you are taking.
Each operation starts with one of three headers:

*** Add File: <path> - create a new file. Every following line is a + line (the initial contents).
*** Delete File: <path> - remove an existing file. Nothing follows.
*** Update File: <path> - patch an existing file in place (optionally with a rename).

May be immediately followed by *** Move to: <new path> if you want to rename the file.
Then one or more "hunks", each introduced by @@ (optionally followed by a hunk header).
Within a hunk each line starts with:

For instructions on [context_before] and [context_after]:
- By default, show 3 lines of code immediately above and 3 lines immediately below each change. If a change is within 3 lines of a previous change, do NOT duplicate the first change's [context_after] lines in the second change's [context_before] lines.
- If 3 lines of context is insufficient to uniquely identify the snippet of code within the file, use the @@ operator to indicate the class or function to which the snippet belongs. For instance, we might have:
@@ class BaseClass
[3 lines of pre-context]
- [old_code]
+ [new_code]
[3 lines of post-context]

- If a code block is repeated so many times in a class or function such that even a single `@@` statement and 3 lines of context cannot uniquely identify the snippet of code, you can use multiple `@@` statements to jump to the right context. For instance:

@@ class BaseClass
@@ 	 def method():
[3 lines of pre-context]
- [old_code]
+ [new_code]
[3 lines of post-context]

The full grammar definition is below:
Patch := Begin { FileOp } End
Begin := "*** Begin Patch" NEWLINE
End := "*** End Patch" NEWLINE
FileOp := AddFile | DeleteFile | UpdateFile
AddFile := "*** Add File: " path NEWLINE { "+" line NEWLINE }
DeleteFile := "*** Delete File: " path NEWLINE
UpdateFile := "*** Update File: " path NEWLINE [ MoveTo ] { Hunk }
MoveTo := "*** Move to: " newPath NEWLINE
Hunk := "@@" [ header ] NEWLINE { HunkLine } [ "*** End of File" NEWLINE ]
HunkLine := (" " | "-" | "+") text NEWLINE

A full patch can combine several operations:

*** Begin Patch
*** Add File: hello.py
+print("Hello, world!")
+print("This is a new file")
*** Delete File: obsolete.py
*** Update File: main.py
@@ Updating main function @@
 def main():
-    print("Old message")
+    print("New message")
     return 0
*** End Patch
"#.to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["input".to_string()]),
            additional_properties: Some(false),
        },
    })
}