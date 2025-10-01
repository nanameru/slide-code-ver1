use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::model_family::ModelFamily;
use crate::plan_tool::PLAN_TOOL;
use crate::tool_apply_patch::ApplyPatchToolType;
use crate::tool_apply_patch::create_apply_patch_freeform_tool;
use crate::tool_apply_patch::create_apply_patch_json_tool;


/// Generic JSON‑Schema subset needed for our tool definitions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum JsonSchema {
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// MCP schema allows "number" | "integer" for Number
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

/// When serialized as JSON, this produces a valid "Tool" in the OpenAI
/// Responses API.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
pub(crate) enum OpenAiTool {
    #[serde(rename = "function")]
    Function(ResponsesApiTool),
    #[serde(rename = "local_shell")]
    LocalShell {},
    // TODO: Understand why we get an error on web_search although the API docs say it's supported.
    // https://platform.openai.com/docs/guides/tools-web-search?api-mode=responses#:~:text=%7B%20type%3A%20%22web_search%22%20%7D%2C
    #[serde(rename = "web_search")]
    WebSearch {},
    #[serde(rename = "custom")]
    Freeform(FreeformTool),
}

#[derive(Debug, Clone)]
pub enum ConfigShellToolType {
    Default,
    Local,
    Streamable,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolsConfig {
    pub shell_type: ConfigShellToolType,
    pub plan_tool: bool,
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,
    pub web_search_request: bool,
    pub include_view_image_tool: bool,
    pub experimental_unified_exec_tool: bool,
}

pub(crate) struct ToolsConfigParams<'a> {
    pub(crate) model_family: &'a ModelFamily,
    pub(crate) include_plan_tool: bool,
    pub(crate) include_apply_patch_tool: bool,
    pub(crate) include_web_search_request: bool,
    pub(crate) use_streamable_shell_tool: bool,
    pub(crate) include_view_image_tool: bool,
    pub(crate) experimental_unified_exec_tool: bool,
    // Compatibility fields for existing code
    pub(crate) include_slides_tools: bool,
    pub(crate) approval_policy: crate::approval_manager::AskForApproval,
    pub(crate) sandbox_policy: crate::seatbelt::SandboxPolicy,
}

impl ToolsConfig {
    pub fn new(params: &ToolsConfigParams) -> Self {
        let ToolsConfigParams {
            model_family,
            include_plan_tool,
            include_apply_patch_tool,
            include_web_search_request,
            use_streamable_shell_tool,
            include_view_image_tool,
            experimental_unified_exec_tool,
            ..
        } = params;
        let shell_type = if *use_streamable_shell_tool {
            ConfigShellToolType::Streamable
        } else if model_family.uses_local_shell_tool {
            ConfigShellToolType::Local
        } else {
            ConfigShellToolType::Default
        };

        let apply_patch_tool_type = match model_family.apply_patch_tool_type {
            Some(ApplyPatchToolType::Freeform) => Some(ApplyPatchToolType::Freeform),
            Some(ApplyPatchToolType::Function) => Some(ApplyPatchToolType::Function),
            None => {
                if *include_apply_patch_tool {
                    Some(ApplyPatchToolType::Freeform)
                } else {
                    None
                }
            }
        };

        Self {
            shell_type,
            plan_tool: *include_plan_tool,
            apply_patch_tool_type,
            web_search_request: *include_web_search_request,
            include_view_image_tool: *include_view_image_tool,
            experimental_unified_exec_tool: *experimental_unified_exec_tool,
        }
    }
}

fn create_unified_exec_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "input".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: Some(
                "When no session_id is provided, treat the array as the command and arguments \
                 to launch. When session_id is set, concatenate the strings (in order) and write \
                 them to the session's stdin."
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "session_id".to_string(),
        JsonSchema::String {
            description: Some(
                "Identifier for an existing interactive session. If omitted, a new command \
                 is spawned."
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "timeout_ms".to_string(),
        JsonSchema::Number {
            description: Some(
                "Maximum time in milliseconds to wait for output after writing the input."
                    .to_string(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "unified_exec".to_string(),
        description:
            "Runs a command in a PTY. Provide a session_id to reuse an existing interactive session.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["input".to_string()]),
            additional_properties: Some(false),
        },
    })
}

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

    properties.insert(
        "with_escalated_permissions".to_string(),
        JsonSchema::Boolean {
            description: Some("Whether to request escalated permissions. Set to true if command needs to be run without sandbox restrictions".to_string()),
        },
    );
    properties.insert(
        "justification".to_string(),
        JsonSchema::String {
            description: Some("Only set if with_escalated_permissions is true. 1-sentence explanation of why we want to run this command.".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_string(),
        description: "Runs a shell command and returns its output.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_string()]),
            additional_properties: Some(false),
        },
    })
}

fn create_view_image_tool() -> OpenAiTool {
    // Support only local filesystem path.
    let mut properties = BTreeMap::new();
    properties.insert(
        "path".to_string(),
        JsonSchema::String {
            description: Some("Local filesystem path to an image file".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "view_image".to_string(),
        description:
            "Attach a local image (by filesystem path) to the conversation context for this turn."
                .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["path".to_string()]),
            additional_properties: Some(false),
        },
    })
}

/// TODO(dylan): deprecate once we get rid of json tool
#[derive(Serialize, Deserialize)]
pub(crate) struct ApplyPatchToolArgs {
    pub(crate) input: String,
}

/// Returns JSON values that are compatible with Function Calling in the
/// Responses API:
/// https://platform.openai.com/docs/guides/function-calling?api-mode=responses
pub fn create_tools_json_for_responses_api(
    tools: &[OpenAiTool],
) -> crate::error::Result<Vec<serde_json::Value>> {
    let mut tools_json = Vec::new();

    for tool in tools {
        let json = serde_json::to_value(tool)?;
        tools_json.push(json);
    }

    Ok(tools_json)
}
/// Returns JSON values that are compatible with Function Calling in the
/// Chat Completions API:
/// https://platform.openai.com/docs/guides/function-calling?api-mode=chat
pub(crate) fn create_tools_json_for_chat_completions_api(
    tools: &[OpenAiTool],
) -> crate::error::Result<Vec<serde_json::Value>> {
    // We start with the JSON for the Responses API and than rewrite it to match
    // the chat completions tool call format.
    let responses_api_tools_json = create_tools_json_for_responses_api(tools)?;
    let tools_json = responses_api_tools_json
        .into_iter()
        .filter_map(|mut tool| {
            if tool.get("type") != Some(&serde_json::Value::String("function".to_string())) {
                return None;
            }

            if let Some(map) = tool.as_object_mut() {
                // Remove "type" field as it is not needed in chat completions.
                map.remove("type");
                Some(json!({
                    "type": "function",
                    "function": map,
                }))
            } else {
                None
            }
        })
        .collect::<Vec<serde_json::Value>>();
    Ok(tools_json)
}

pub(crate) fn mcp_tool_to_openai_tool(
    fully_qualified_name: String,
    tool: mcp_types::Tool,
) -> Result<ResponsesApiTool, serde_json::Error> {
    let mcp_types::Tool {
        description,
        mut input_schema,
        ..
    } = tool;

    // Ensure properties exists (Agents SDK does this as well)
    if input_schema.properties.is_empty() {
        input_schema.properties.insert("__placeholder__".to_string(), serde_json::Map::new());
        input_schema.properties.remove("__placeholder__");
    }

    let mut serialized = serde_json::to_value(input_schema)?;
    sanitize_json_schema(&mut serialized);
    let input_schema = serde_json::from_value::<JsonSchema>(serialized)?;

    Ok(ResponsesApiTool {
        name: fully_qualified_name,
        description: description.unwrap_or_default(),
        strict: false,
        parameters: input_schema,
    })
}

/// Sanitize a JSON Schema (as serde_json::Value) so it can fit our limited
/// JsonSchema enum. This function:
/// - Ensures every schema object has a "type". If missing, infers it from
///   common keywords (properties => object, items => array, enum/const/format => string)
///   and otherwise defaults to "string".
/// - Fills required child fields (e.g. array items, object properties) with
///   permissive defaults when absent.
fn sanitize_json_schema(value: &mut JsonValue) {
    match value {
        JsonValue::Bool(_) => {
            // JSON Schema boolean form: true/false. Coerce to an accept-all string.
            *value = json!({ "type": "string" });
        }
        JsonValue::Array(arr) => {
            for v in arr.iter_mut() {
                sanitize_json_schema(v);
            }
        }
        JsonValue::Object(map) => {
            // First, recursively sanitize known nested schema holders
            if let Some(props) = map.get_mut("properties") {
                if let Some(props_map) = props.as_object_mut() {
                    for (_k, v) in props_map.iter_mut() {
                        sanitize_json_schema(v);
                    }
                }
            }
            if let Some(items) = map.get_mut("items") {
                sanitize_json_schema(items);
            }
            // Some schemas use oneOf/anyOf/allOf - sanitize their entries
            for combiner in ["oneOf", "anyOf", "allOf", "prefixItems"] {
                if let Some(v) = map.get_mut(combiner) {
                    sanitize_json_schema(v);
                }
            }

            // Normalize/ensure type
            let mut ty = map
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // If type is an array (union), pick first supported; else leave to inference
            if ty.is_none() {
                if let Some(JsonValue::Array(types)) = map.get("type") {
                    for t in types {
                        if let Some(tt) = t.as_str() {
                            if matches!(
                                tt,
                                "object" | "array" | "string" | "number" | "integer" | "boolean"
                            ) {
                                ty = Some(tt.to_string());
                                break;
                            }
                        }
                    }
                }
            }

            // Infer type if still missing
            if ty.is_none() {
                if map.contains_key("properties")
                    || map.contains_key("required")
                    || map.contains_key("additionalProperties")
                {
                    ty = Some("object".to_string());
                } else if map.contains_key("items") || map.contains_key("prefixItems") {
                    ty = Some("array".to_string());
                } else if map.contains_key("enum")
                    || map.contains_key("const")
                    || map.contains_key("format")
                {
                    ty = Some("string".to_string());
                } else if map.contains_key("minimum")
                    || map.contains_key("maximum")
                    || map.contains_key("exclusiveMinimum")
                    || map.contains_key("exclusiveMaximum")
                    || map.contains_key("multipleOf")
                {
                    ty = Some("number".to_string());
                }
            }
            // If we still couldn't infer, default to string
            let ty = ty.unwrap_or_else(|| "string".to_string());
            map.insert("type".to_string(), JsonValue::String(ty.to_string()));

            // Ensure object schemas have properties map
            if ty == "object" {
                if !map.contains_key("properties") {
                    map.insert(
                        "properties".to_string(),
                        JsonValue::Object(serde_json::Map::new()),
                    );
                }
                // If additionalProperties is an object schema, sanitize it too.
                // Leave booleans as-is, since JSON Schema allows boolean here.
                if let Some(ap) = map.get_mut("additionalProperties") {
                    let is_bool = matches!(ap, JsonValue::Bool(_));
                    if !is_bool {
                        sanitize_json_schema(ap);
                    }
                }
            }

            // Ensure array schemas have items
            if ty == "array" && !map.contains_key("items") {
                map.insert("items".to_string(), json!({ "type": "string" }));
            }
        }
        _ => {}
    }
}

/// Returns a list of OpenAiTools based on the provided config and MCP tools.
/// Note that the keys of mcp_tools should be fully qualified names. See
/// [`McpConnectionManager`] for more details.
pub(crate) fn get_openai_tools(
    config: &ToolsConfig,
    mcp_tools: Option<HashMap<String, mcp_types::Tool>>,
) -> Vec<OpenAiTool> {
    let mut tools: Vec<OpenAiTool> = Vec::new();

    if config.experimental_unified_exec_tool {
        tools.push(create_unified_exec_tool());
    } else {
        match &config.shell_type {
            ConfigShellToolType::Default => {
                tools.push(create_shell_tool());
            }
            ConfigShellToolType::Local => {
                tools.push(OpenAiTool::LocalShell {});
            }
            ConfigShellToolType::Streamable => {
                tools.push(OpenAiTool::Function(
                    crate::exec_command::create_exec_command_tool_for_responses_api(),
                ));
                tools.push(OpenAiTool::Function(
                    crate::exec_command::create_write_stdin_tool_for_responses_api(),
                ));
            }
        }
    }

    if config.plan_tool {
        tools.push(PLAN_TOOL.clone());
    }

    if let Some(apply_patch_tool_type) = &config.apply_patch_tool_type {
        match apply_patch_tool_type {
            ApplyPatchToolType::Freeform => {
                tools.push(create_apply_patch_freeform_tool());
            }
            ApplyPatchToolType::Function => {
                tools.push(create_apply_patch_json_tool());
            }
        }
    }

    if config.web_search_request {
        tools.push(OpenAiTool::WebSearch {});
    }

    // Include the view_image tool so the agent can attach images to context.
    if config.include_view_image_tool {
        tools.push(create_view_image_tool());
    }
    if let Some(mcp_tools) = mcp_tools {
        // Ensure deterministic ordering to maximize prompt cache hits.
        let mut entries: Vec<(String, mcp_types::Tool)> = mcp_tools.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, tool) in entries.into_iter() {
            match mcp_tool_to_openai_tool(name.clone(), tool.clone()) {
                Ok(converted_tool) => tools.push(OpenAiTool::Function(converted_tool)),
                Err(e) => {
                    tracing::error!("Failed to convert {name:?} MCP tool to OpenAI tool: {e:?}");
                }
            }
        }
    }

    tools
}

// Compatibility function for existing code
pub fn render_tools_instructions(_config: &ToolsConfig, _approval_mode_hint: Option<&str>) -> String {
    // Instruct the model to emit a single-line JSON object to trigger tools.
    // The ToolExecutor looks for a line that starts with '{' and contains a
    // "tool" key, or a <tool_call>{...}</tool_call> block. Any prose around
    // it will prevent detection, so we must be explicit.
    let mut lines: Vec<String> = Vec::new();
    lines.push("To execute a tool, include exactly one of the following as a standalone line with no surrounding text:".to_string());
    lines.push("".to_string());
    lines.push("Examples:".to_string());
    lines.push("- read a file:".to_string());
    lines.push("  {\"tool\":\"shell\",\"command\":[\"cat\",\"/absolute/path/to/file\"]}".to_string());
    lines.push("- list files in a directory:".to_string());
    lines.push("  {\"tool\":\"shell\",\"command\":[\"ls\",\"-la\",\"/absolute/path/to/dir\"]}".to_string());
    lines.push("- search files by name:".to_string());
    lines.push("  {\"tool\":\"shell\",\"command\":[\"find\",\"/path\",\"-name\",\"pattern\"]}".to_string());
    lines.push("- search text in files:".to_string());
    lines.push("  {\"tool\":\"shell\",\"command\":[\"rg\",\"pattern\",\"/path\"]}".to_string());
    lines.push("- apply a patch (edit files):".to_string());
    lines.push("  {\"tool\":\"apply_patch\",\"input\":\"*** Begin Patch\\n*** Update File: path\\n- old\\n+ new\\n*** End Patch\"}".to_string());
    lines.push("".to_string());
    lines.push("Rules:".to_string());
    lines.push("- Output the JSON on its own line. Do not prepend/append any explanation or formatting.".to_string());
    lines.push("- Use only supported tool names: shell, apply_patch.".to_string());
    lines.push("- Prefer absolute paths. Relative paths are resolved against the current working directory.".to_string());
    lines.push("- For shell, use commands like cat, ls, find, rg, grep for file operations.".to_string());
    lines.push("- Alternatively, you may wrap the JSON in <tool_call>{...}</tool_call> on a single line.".to_string());
    lines.join("\n")
}
