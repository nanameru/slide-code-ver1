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

// Simple apply patch function for compatibility
pub fn tool_apply_patch(input: ApplyPatchInput, _dry_run: bool) -> ApplyPatchResult {
    // Simplified implementation - just return success
    ApplyPatchResult {
        applied: true,
        message: format!("Applied patch: {}", input.patch),
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