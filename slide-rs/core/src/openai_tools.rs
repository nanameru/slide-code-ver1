use crate::approval_manager::AskForApproval;
use crate::seatbelt::SandboxPolicy;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub schema: Option<JsonValue>,
}

/// Generic JSON‑Schema subset needed for our tool definitions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JsonSchema {
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    #[serde(alias = "integer")]
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Array {
        items: Box<JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Object {
        properties: BTreeMap<String, JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<Vec<String>>,
        #[serde(
            rename = "additionalProperties",
            skip_serializing_if = "Option::is_none"
        )]
        additional_properties: Option<bool>,
    },
}

/// Tool definition that matches OpenAI function calling format
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResponsesApiTool {
    pub name: String,
    pub description: String,
    /// TODO: Validation. When strict is set to true, the JSON schema,
    /// `required` and `additional_properties` must be present. All fields in
    /// `properties` must be present in `required`.
    pub strict: bool,
    pub parameters: JsonSchema,
}

/// Freeform tool format for custom tools (GPT-5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreeformTool {
    pub name: String,
    pub description: String,
    pub format: FreeformToolFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreeformToolFormat {
    pub r#type: String,
    pub syntax: String,
    pub definition: String,
}

/// When serialized as JSON, this produces a valid "Tool" in the OpenAI
/// Responses API.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum OpenAiTool {
    #[serde(rename = "function")]
    Function(ResponsesApiTool),
    #[serde(rename = "freeform")]
    Freeform(FreeformTool),
    #[serde(rename = "local_shell")]
    LocalShell {},
}

#[derive(Debug, Clone)]
pub enum ConfigShellToolType {
    DefaultShell,
    ShellWithRequest { sandbox_policy: SandboxPolicy },
    LocalShell,
    StreamableShell,
}

#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub shell_type: ConfigShellToolType,
    pub include_plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub include_view_image_tool: bool,
    pub include_web_search_request: bool,
    pub include_slides_tools: bool,
}

pub struct ToolsConfigParams {
    pub approval_policy: AskForApproval,
    pub sandbox_policy: SandboxPolicy,
    pub include_plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub include_view_image_tool: bool,
    pub include_web_search_request: bool,
    pub use_streamable_shell_tool: bool,
    pub include_slides_tools: bool,
}

impl ToolsConfig {
    pub fn new(params: &ToolsConfigParams) -> Self {
        let shell_type = if params.use_streamable_shell_tool {
            ConfigShellToolType::StreamableShell
        } else if matches!(params.approval_policy, AskForApproval::OnRequest)
            && !params.use_streamable_shell_tool
        {
            ConfigShellToolType::ShellWithRequest {
                sandbox_policy: params.sandbox_policy.clone(),
            }
        } else {
            ConfigShellToolType::DefaultShell
        };

        Self {
            shell_type,
            include_plan_tool: params.include_plan_tool,
            include_apply_patch_tool: params.include_apply_patch_tool,
            include_view_image_tool: params.include_view_image_tool,
            include_web_search_request: params.include_web_search_request,
            include_slides_tools: params.include_slides_tools,
        }
    }
}

/// Create the basic shell tool
fn create_shell_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: Some("The command to execute".to_string()),
        },
    );
    properties.insert(
        "workdir".to_string(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_string()),
        },
    );
    properties.insert(
        "timeout_ms".to_string(),
        JsonSchema::Number {
            description: Some("The timeout for the command in milliseconds".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_string(),
        description: "Runs a shell command and returns its output".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_string()]),
            additional_properties: Some(false),
        },
    })
}

/// Create the sandbox-aware shell tool with approval support
fn create_shell_tool_for_sandbox(sandbox_policy: &SandboxPolicy) -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: Some("The command to execute".to_string()),
        },
    );
    properties.insert(
        "workdir".to_string(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_string()),
        },
    );
    properties.insert(
        "timeout_ms".to_string(),
        JsonSchema::Number {
            description: Some("The timeout for the command in milliseconds".to_string()),
        },
    );

    // Add escalated permissions support for workspace-write mode
    if matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }) {
        properties.insert(
            "with_escalated_permissions".to_string(),
            JsonSchema::Boolean {
                description: Some("Whether to request escalated permissions. Set to true if command needs to be run without sandbox restrictions".to_string()),
            },
        );
        properties.insert(
            "justification".to_string(),
            JsonSchema::String {
                description: Some("Justification for why this command needs to be run".to_string()),
            },
        );
    }

    let required = vec!["command".to_string()];
    if matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }) {
        // justification is required when with_escalated_permissions is used
    }

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_string(),
        description: format!(
            "Runs a shell command with sandbox policy: {}. {}",
            match sandbox_policy {
                SandboxPolicy::ReadOnly => "read-only",
                SandboxPolicy::WorkspaceWrite { .. } => "workspace-write",
                SandboxPolicy::DangerFullAccess => "danger-full-access",
            },
            if matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }) {
                "Use with_escalated_permissions=true for commands that need to access outside workspace."
            } else {
                ""
            }
        ),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(required),
            additional_properties: Some(false),
        },
    })
}

/// Create the plan tool
fn create_plan_tool() -> OpenAiTool {
    let mut plan_item_props = BTreeMap::new();
    plan_item_props.insert("step".to_string(), JsonSchema::String { description: None });
    plan_item_props.insert(
        "status".to_string(),
        JsonSchema::String {
            description: Some("One of: pending, in_progress, completed".to_string()),
        },
    );

    let plan_items_schema = JsonSchema::Array {
        description: Some("The list of steps".to_string()),
        items: Box::new(JsonSchema::Object {
            properties: plan_item_props,
            required: Some(vec!["step".to_string(), "status".to_string()]),
            additional_properties: Some(false),
        }),
    };

    let mut properties = BTreeMap::new();
    properties.insert(
        "explanation".to_string(),
        JsonSchema::String { description: None },
    );
    properties.insert("plan".to_string(), plan_items_schema);

    OpenAiTool::Function(ResponsesApiTool {
        name: "update_plan".to_string(),
        description: "Updates the task plan. Provide an explanation and a list of plan items."
            .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["plan".to_string()]),
            additional_properties: Some(false),
        },
    })
}

/// Create tools based on configuration
pub fn create_tools(config: &ToolsConfig, _mcp_tools: Option<Vec<String>>) -> Vec<OpenAiTool> {
    let mut tools = Vec::new();

    // Add shell tool based on configuration
    match &config.shell_type {
        ConfigShellToolType::DefaultShell => {
            tools.push(create_shell_tool());
        }
        ConfigShellToolType::ShellWithRequest { sandbox_policy } => {
            tools.push(create_shell_tool_for_sandbox(sandbox_policy));
        }
        ConfigShellToolType::LocalShell => {
            // For now, same as default shell
            tools.push(create_shell_tool());
        }
        ConfigShellToolType::StreamableShell => {
            // Add streamable shell tools (simplified)
            tools.push(create_shell_tool());
        }
    }

    if config.include_plan_tool {
        tools.push(create_plan_tool());
    }

    // Add apply_patch tool if enabled
    if config.include_apply_patch_tool {
        tools.push(create_apply_patch_tool());
    }

    // Note: Other tools (view_image, etc.) would be implemented similarly

    tools
}

/// Create the apply_patch tool
fn create_apply_patch_tool() -> OpenAiTool {
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

/// Legacy function for compatibility - returns tool names
pub fn get_openai_tools(cfg: &ToolsConfig, _mcp_tools: Option<Vec<String>>) -> Vec<String> {
    let tools = create_tools(cfg, _mcp_tools);
    tools
        .into_iter()
        .map(|t| match t {
            OpenAiTool::Function(f) => f.name,
            OpenAiTool::Freeform(f) => f.name,
            OpenAiTool::LocalShell {} => "local_shell".to_string(),
        })
        .collect()
}

/// Render a concise instruction block that advertises available tools to the model.
/// This is a lightweight alternative to function/tool calling and mirrors codex style.
pub fn render_tools_instructions(cfg: &ToolsConfig, approval_mode_hint: Option<&str>) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(
        "You can propose using the following tools by writing clear instructions:".to_string(),
    );

    // Shell tool description based on configuration
    match &cfg.shell_type {
        ConfigShellToolType::StreamableShell => {
            lines.push("- exec_command: run a shell command. Always explain why and prefer read-only commands (ls, cat, rg).".to_string());
        }
        ConfigShellToolType::ShellWithRequest { sandbox_policy } => {
            let policy_desc = match sandbox_policy {
                SandboxPolicy::ReadOnly => "read-only sandbox",
                SandboxPolicy::WorkspaceWrite { .. } => {
                    "workspace-write sandbox (use with_escalated_permissions for broader access)"
                }
                SandboxPolicy::DangerFullAccess => "full access",
            };
            lines.push(format!("- shell: run a shell command in {}. Always explain why and prefer read-only commands (ls, cat, rg).", policy_desc));
        }
        _ => {
            lines.push("- shell: run a shell command. Always explain why and prefer read-only commands (ls, cat, rg).".to_string());
        }
    }

    if cfg.include_apply_patch_tool {
        lines.push(
            "- apply_patch: propose a unified diff to edit files. Keep edits minimal and correct."
                .to_string(),
        );
    }
    if cfg.include_slides_tools {
        lines.push(
            "- slides_write: write slide files under slides/ (create/overwrite/append)."
                .to_string(),
        );
        lines.push(
            "- slides_apply_patch: apply a restricted apply_patch affecting only slides/ files."
                .to_string(),
        );
    }
    if cfg.include_plan_tool {
        lines.push("- update_plan: refine your task plan concisely.".to_string());
    }
    if cfg.include_view_image_tool {
        lines.push("- view_image: request to view an image by path.".to_string());
    }
    if cfg.include_web_search_request {
        lines.push(
            "- web_search_request: request a web search when strictly necessary.".to_string(),
        );
    }

    if let Some(mode) = approval_mode_hint {
        lines.push(format!(
            "Approval policy: {mode}. Destructive or ambiguous actions may require user approval."
        ));
    }

    lines.push("When proposing a tool, output a short rationale followed by the exact command or a minimal diff.".to_string());
    lines.join("\n")
}
