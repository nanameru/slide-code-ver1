use std::collections::HashMap;

use base64::Engine;
use mcp_types::CallToolResult;
use serde::ser::Serializer;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;

use crate::protocol::InputItem;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseInputItem {
    Message {
        role: String,
        content: Vec<ContentItem>,
    },
    FunctionCallOutput {
        call_id: String,
        output: FunctionCallOutputPayload,
    },
    McpToolCallOutput {
        call_id: String,
        result: Result<CallToolResult, String>,
    },
    CustomToolCallOutput {
        call_id: String,
        output: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    InputText { text: String },
    InputImage { image_url: String },
    OutputText { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseItem {
    Message {
        id: Option<String>,
        role: String,
        content: Vec<ContentItem>,
    },
    Reasoning {
        id: String,
        summary: Vec<ReasoningItemReasoningSummary>,
        #[serde(default, skip_serializing_if = "should_serialize_reasoning_content")]
        content: Option<Vec<ReasoningItemContent>>,
        encrypted_content: Option<String>,
    },
    LocalShellCall {
        /// Set when using the chat completions API.
        id: Option<String>,
        /// Set when using the Responses API.
        call_id: Option<String>,
        status: LocalShellStatus,
        action: LocalShellAction,
    },
    FunctionCall {
        id: Option<String>,
        name: String,
        // The Responses API returns the function call arguments as a *string* that contains
        // JSON, not as an already‑parsed object. We keep it as a raw string here and let
        // Session::handle_function_call parse it into a Value. This exactly matches the
        // Chat Completions + Responses API behavior.
        arguments: String,
        call_id: String,
    },
    // NOTE: The input schema for `function_call_output` objects that clients send to the
    // OpenAI /v1/responses endpoint is NOT the same shape as the objects the server returns on the
    // SSE stream. When *sending* we must wrap the string output inside an object that includes a
    // required `success` boolean. The upstream TypeScript CLI does this implicitly. To ensure we
    // serialize exactly the expected shape we introduce a dedicated payload struct and flatten it
    // here.
    FunctionCallOutput {
        call_id: String,
        output: FunctionCallOutputPayload,
    },
    CustomToolCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,

        call_id: String,
        name: String,
        input: String,
    },
    CustomToolCallOutput {
        call_id: String,
        output: String,
    },
    // Emitted by the Responses API when the agent triggers a web search.
    // Example payload (from SSE `response.output_item.done`):
    // {
    //   "id":"ws_...",
    //   "type":"web_search_call",
    //   "status":"completed",
    //   "action": {"type":"search","query":"weather: San Francisco, CA"}
    // }
    WebSearchCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        action: WebSearchAction,
    },

    #[serde(other)]
    Other,
}

fn should_serialize_reasoning_content(content: &Option<Vec<ReasoningItemContent>>) -> bool {
    match content {
        Some(content) => !content
            .iter()
            .any(|c| matches!(c, ReasoningItemContent::ReasoningText { .. })),
        None => false,
    }
}

impl From<ResponseInputItem> for ResponseItem {
    fn from(item: ResponseInputItem) -> Self {
        match item {
            ResponseInputItem::Message { role, content } => Self::Message {
                role,
                content,
                id: None,
            },
            ResponseInputItem::FunctionCallOutput { call_id, output } => {
                Self::FunctionCallOutput { call_id, output }
            }
            ResponseInputItem::McpToolCallOutput { call_id, result } => Self::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    success: Some(result.is_ok()),
                    content: result.map_or_else(
                        |tool_call_err| format!("err: {tool_call_err:?}"),
                        |result| {
                            serde_json::to_string(&result)
                                .unwrap_or_else(|e| format!("JSON serialization error: {e}"))
                        },
                    ),
                },
            },
            ResponseInputItem::CustomToolCallOutput { call_id, output } => {
                Self::CustomToolCallOutput { call_id, output }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalShellStatus {
    Completed,
    InProgress,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalShellAction {
    Exec(LocalShellExecAction),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalShellExecAction {
    pub command: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub working_directory: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSearchAction {
    Search {
        query: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningItemReasoningSummary {
    SummaryText { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningItemContent {
    ReasoningText { text: String },
    Text { text: String },
}

impl From<Vec<InputItem>> for ResponseInputItem {
    fn from(items: Vec<InputItem>) -> Self {
        Self::Message {
            role: "user".to_string(),
            content: items
                .into_iter()
                .filter_map(|c| match c {
                    InputItem::Text { text } => Some(ContentItem::InputText { text }),
                    InputItem::Image { image_url } => Some(ContentItem::InputImage { image_url }),
                    InputItem::LocalImage { path } => match std::fs::read(&path) {
                        Ok(bytes) => {
                            let mime = mime_guess::from_path(&path)
                                .first()
                                .map(|m| m.essence_str().to_owned())
                                .unwrap_or_else(|| "application/octet-stream".to_string());
                            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                            Some(ContentItem::InputImage {
                                image_url: format!("data:{mime};base64,{encoded}"),
                            })
                        }
                        Err(err) => {
                            tracing::warn!(
                                "Skipping image {} – could not read file: {}",
                                path.display(),
                                err
                            );
                            None
                        }
                    },
                })
                .collect::<Vec<ContentItem>>(),
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ShellToolCallParams {
    pub command: Vec<String>,
    pub workdir: Option<String>,
    #[serde(alias = "timeout")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_escalated_permissions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCallOutputPayload {
    pub content: String,
    pub success: Option<bool>,
}

impl Serialize for FunctionCallOutputPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.content)
    }
}

impl<'de> Deserialize<'de> for FunctionCallOutputPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(FunctionCallOutputPayload {
            content: s,
            success: None,
        })
    }
}

impl std::fmt::Display for FunctionCallOutputPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.content)
    }
}
impl std::ops::Deref for FunctionCallOutputPayload {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serializes_success_as_plain_string() {
        let item = ResponseInputItem::FunctionCallOutput {
            call_id: "call1".into(),
            output: FunctionCallOutputPayload {
                content: "ok".into(),
                success: None,
            },
        };
        let json = serde_json::to_string(&item).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v.get("output").unwrap().as_str().unwrap(), "ok");
    }
    #[test]
    fn serializes_failure_as_string() {
        let item = ResponseInputItem::FunctionCallOutput {
            call_id: "call1".into(),
            output: FunctionCallOutputPayload {
                content: "bad".into(),
                success: Some(false),
            },
        };
        let json = serde_json::to_string(&item).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v.get("output").unwrap().as_str().unwrap(), "bad");
    }
    #[test]
    fn deserialize_shell_tool_call_params() {
        let json = r#"{ "command": ["ls", "-l"], "workdir": "/tmp", "timeout": 1000 }"#;
        let params: ShellToolCallParams = serde_json::from_str(json).unwrap();
        assert_eq!(
            ShellToolCallParams {
                command: vec!["ls".to_string(), "-l".to_string()],
                workdir: Some("/tmp".to_string()),
                timeout_ms: Some(1000),
                with_escalated_permissions: None,
                justification: None
            },
            params
        );
    }
}
