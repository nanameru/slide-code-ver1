use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::openai_tools::JsonSchema;

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

/// Freeform tool format for custom tools (GPT-5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreeformTool {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) format: FreeformToolFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreeformToolFormat {
    pub(crate) r#type: String,
    pub(crate) syntax: String,
    pub(crate) definition: String,
}

/// Tool definition that matches OpenAI function calling format
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResponsesApiTool {
    pub(crate) name: String,
    pub(crate) description: String,
    /// TODO: Validation. When strict is set to true, the JSON schema,
    /// `required` and `additional_properties` must be present. All fields in
    /// `properties` must be present in `required`.
    pub(crate) strict: bool,
    pub(crate) parameters: JsonSchema,
}

/// When serialized as JSON, this produces a valid "Tool" in the OpenAI
/// Responses API.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
pub(crate) enum OpenAiTool {
    #[serde(rename = "function")]
    Function(ResponsesApiTool),
    #[serde(rename = "freeform")]
    Freeform(FreeformTool),
    #[serde(rename = "local_shell")]
    LocalShell {},
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
            definition: r#"start: begin_patch hunk+ end_patch
begin_patch: "*** Begin Patch" LF
end_patch: "*** End Patch" LF?

hunk: add_hunk | delete_hunk | update_hunk
add_hunk: "*** Add File: " filename LF add_line+
delete_hunk: "*** Delete File: " filename LF
update_hunk: "*** Update File: " filename LF change_move? change?

filename: /(.+)/
add_line: "+" /(.+)/ LF -> line

change_move: "*** Move to: " filename LF
change: (change_context | change_line)+ eof_line?
change_context: ("@@" | "@@ " /(.+)/) LF
change_line: ("+" | "-" | " ") /(.+)/ LF
eof_line: "*** End of File" LF

%import common.LF
"#
            .to_string(),
        },
    })
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
*** Delete File: <path> - delete a file (no more lines after this header).
*** Update File: <path> - edit an existing file. This supports:
  1. Context lines (starting with a space).
  2. Additions (starting with +).
  3. Deletions (starting with -).
  4. Optional context markers (@@ ... @@).
  5. Optional end‑of‑file marker (*** End of File).

Examples:

*** Begin Patch
*** Add File: hello.py
+print("Hello, world!")
+print("This is a new file")
*** End Patch

*** Begin Patch
*** Update File: main.py
 def main():
-    print("Old message")
+    print("New message")
     return 0
*** End Patch

*** Begin Patch
*** Delete File: obsolete.py
*** End Patch

*** Begin Patch
*** Update File: config.json
@@ Adding new configuration @@
 {
   "version": "1.0",
+  "debug": true,
   "name": "myapp"
 }
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

/// Parse a patch string and extract file operations
pub fn parse_patch(patch_content: &str) -> Result<Vec<FileOperation>, String> {
    let lines: Vec<&str> = patch_content.lines().collect();
    let mut operations = Vec::new();
    let mut i = 0;

    // Find "*** Begin Patch"
    while i < lines.len() && !lines[i].starts_with("*** Begin Patch") {
        i += 1;
    }

    if i >= lines.len() {
        return Err("Patch must start with '*** Begin Patch'".to_string());
    }

    i += 1; // Skip "*** Begin Patch"

    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with("*** End Patch") {
            break;
        }

        if line.starts_with("*** Add File: ") {
            let path = line.strip_prefix("*** Add File: ").unwrap().to_string();
            i += 1;

            let mut content = Vec::new();
            while i < lines.len() {
                let line = lines[i];
                if line.starts_with("*** ") {
                    break;
                }
                if line.starts_with("+") {
                    content.push(line.strip_prefix("+").unwrap_or(line).to_string());
                }
                i += 1;
            }

            operations.push(FileOperation::Add {
                path,
                content: content.join("\n"),
            });
            continue;
        }

        if line.starts_with("*** Delete File: ") {
            let path = line.strip_prefix("*** Delete File: ").unwrap().to_string();
            operations.push(FileOperation::Delete { path });
            i += 1;
            continue;
        }

        if line.starts_with("*** Update File: ") {
            let path = line.strip_prefix("*** Update File: ").unwrap().to_string();
            i += 1;

            let mut changes = Vec::new();
            while i < lines.len() {
                let line = lines[i];
                if line.starts_with("*** ") {
                    break;
                }

                if line.starts_with(" ") {
                    changes.push(ChangeOperation::Context {
                        line: line.strip_prefix(" ").unwrap_or(line).to_string(),
                    });
                } else if line.starts_with("+") {
                    changes.push(ChangeOperation::Add {
                        line: line.strip_prefix("+").unwrap_or(line).to_string(),
                    });
                } else if line.starts_with("-") {
                    changes.push(ChangeOperation::Remove {
                        line: line.strip_prefix("-").unwrap_or(line).to_string(),
                    });
                } else if line.starts_with("@@") {
                    changes.push(ChangeOperation::Context {
                        line: format!("# {}", line),
                    });
                }
                i += 1;
            }

            operations.push(FileOperation::Update { path, changes });
            continue;
        }

        i += 1;
    }

    Ok(operations)
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileOperation {
    Add { path: String, content: String },
    Delete { path: String },
    Update { path: String, changes: Vec<ChangeOperation> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeOperation {
    Context { line: String },
    Add { line: String },
    Remove { line: String },
}

/// Apply a file operation to the filesystem
pub fn apply_file_operation(operation: &FileOperation) -> Result<String, String> {
    match operation {
        FileOperation::Add { path, content } => {
            std::fs::write(path, content)
                .map_err(|e| format!("Failed to create file {}: {}", path, e))?;
            Ok(format!("Created file: {}", path))
        }
        FileOperation::Delete { path } => {
            std::fs::remove_file(path)
                .map_err(|e| format!("Failed to delete file {}: {}", path, e))?;
            Ok(format!("Deleted file: {}", path))
        }
        FileOperation::Update { path, changes } => {
            let existing_content = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read file {}: {}", path, e))?;

            let updated_content = apply_changes(&existing_content, changes)?;

            std::fs::write(path, updated_content)
                .map_err(|e| format!("Failed to update file {}: {}", path, e))?;

            Ok(format!("Updated file: {}", path))
        }
    }
}

/// Apply changes to file content
fn apply_changes(content: &str, changes: &[ChangeOperation]) -> Result<String, String> {
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut result = Vec::new();
    let mut change_idx = 0;
    let mut line_idx = 0;

    while change_idx < changes.len() || line_idx < lines.len() {
        if change_idx >= changes.len() {
            // Add remaining original lines
            result.extend_from_slice(&lines[line_idx..]);
            break;
        }

        match &changes[change_idx] {
            ChangeOperation::Context { line } => {
                // Skip context lines (they're just for reference)
                if line_idx < lines.len() && lines[line_idx].trim() == line.trim() {
                    result.push(lines[line_idx].clone());
                    line_idx += 1;
                }
                change_idx += 1;
            }
            ChangeOperation::Add { line } => {
                result.push(line.clone());
                change_idx += 1;
            }
            ChangeOperation::Remove { line: _ } => {
                // Skip the line in the original file
                if line_idx < lines.len() {
                    line_idx += 1;
                }
                change_idx += 1;
            }
        }
    }

    Ok(result.join("\n"))
}

// Legacy compatibility structures
#[derive(Debug, Clone)]
pub struct ApplyPatchInput {
    pub patch: String,
}

#[derive(Debug, Clone)]
pub struct ApplyPatchResult {
    pub applied: bool,
    pub message: String,
}

pub fn tool_apply_patch(input: ApplyPatchInput, _workspace_write: bool) -> ApplyPatchResult {
    match parse_patch(&input.patch) {
        Ok(operations) => {
            let mut results = Vec::new();
            let mut all_applied = true;

            for operation in operations {
                match apply_file_operation(&operation) {
                    Ok(message) => results.push(message),
                    Err(error) => {
                        all_applied = false;
                        results.push(format!("Error: {}", error));
                    }
                }
            }

            ApplyPatchResult {
                applied: all_applied,
                message: results.join("\n"),
            }
        }
        Err(error) => ApplyPatchResult {
            applied: false,
            message: format!("Failed to parse patch: {}", error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_add_file_patch() {
        let patch = r#"*** Begin Patch
*** Add File: test.txt
+Hello World
+This is a test file
*** End Patch"#;

        let operations = parse_patch(patch).unwrap();
        assert_eq!(operations.len(), 1);

        match &operations[0] {
            FileOperation::Add { path, content } => {
                assert_eq!(path, "test.txt");
                assert_eq!(content, "Hello World\nThis is a test file");
            }
            _ => panic!("Expected Add operation"),
        }
    }

    #[test]
    fn test_parse_update_file_patch() {
        let patch = r#"*** Begin Patch
*** Update File: main.rs
 fn main() {
-    println!("Hello");
+    println!("Hello, World!");
 }
*** End Patch"#;

        let operations = parse_patch(patch).unwrap();
        assert_eq!(operations.len(), 1);

        match &operations[0] {
            FileOperation::Update { path, changes } => {
                assert_eq!(path, "main.rs");
                assert_eq!(changes.len(), 4);
            }
            _ => panic!("Expected Update operation"),
        }
    }

    #[test]
    fn test_parse_delete_file_patch() {
        let patch = r#"*** Begin Patch
*** Delete File: obsolete.txt
*** End Patch"#;

        let operations = parse_patch(patch).unwrap();
        assert_eq!(operations.len(), 1);

        match &operations[0] {
            FileOperation::Delete { path } => {
                assert_eq!(path, "obsolete.txt");
            }
            _ => panic!("Expected Delete operation"),
        }
    }

    #[test]
    fn test_apply_changes() {
        let original = "line1\nline2\nline3";
        let changes = vec![
            ChangeOperation::Context { line: "line1".to_string() },
            ChangeOperation::Remove { line: "line2".to_string() },
            ChangeOperation::Add { line: "new_line2".to_string() },
            ChangeOperation::Context { line: "line3".to_string() },
        ];

        let result = apply_changes(original, &changes).unwrap();
        assert_eq!(result, "line1\nnew_line2\nline3");
    }

    #[test]
    fn test_create_tools() {
        let freeform_tool = create_apply_patch_freeform_tool();
        match freeform_tool {
            OpenAiTool::Freeform(tool) => {
                assert_eq!(tool.name, "apply_patch");
                assert!(tool.description.contains("edit files"));
            }
            _ => panic!("Expected Freeform tool"),
        }

        let json_tool = create_apply_patch_json_tool();
        match json_tool {
            OpenAiTool::Function(tool) => {
                assert_eq!(tool.name, "apply_patch");
                assert!(tool.description.contains("edit files"));
            }
            _ => panic!("Expected Function tool"),
        }
    }
}
