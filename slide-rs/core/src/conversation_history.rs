use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// codex-1レベルのResponseItem定義（7種類対応）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseItem {
    Message {
        #[serde(skip_serializing)]
        id: Option<String>,
        role: String,
        content: Vec<ContentItem>,
    },
    Reasoning {
        #[serde(default, skip_serializing)]
        id: String,
        summary: Vec<ReasoningItemReasoningSummary>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ReasoningItemContent>>,
        encrypted_content: Option<String>,
    },
    LocalShellCall {
        /// Set when using the chat completions API.
        #[serde(skip_serializing)]
        id: Option<String>,
        /// Set when using the Responses API.
        call_id: Option<String>,
        status: LocalShellStatus,
        action: LocalShellAction,
    },
    FunctionCall {
        #[serde(skip_serializing)]
        id: Option<String>,
        name: String,
        arguments: String,
        call_id: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: FunctionCallOutputPayload,
    },
    CustomToolCall {
        #[serde(skip_serializing)]
        id: Option<String>,
        call_id: String,
        name: String,
        input: String,
        status: CustomToolStatus,
    },
    CustomToolCallOutput {
        call_id: String,
        output: String,
    },
    WebSearchCall {
        #[serde(skip_serializing)]
        id: Option<String>,
        call_id: Option<String>,
        action: WebSearchAction,
    },
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    InputText { text: String },
    InputImage { image_url: String },
    OutputText { text: String },
    FunctionCall { name: String, arguments: String },
    FunctionResult { result: String },
}

// 推論関連の構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningItemReasoningSummary {
    SummaryText { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningItemContent {
    ReasoningText { text: String },
    Text { text: String },
}

// LocalShellCall関連の構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalShellStatus {
    Completed,
    InProgress,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalShellAction {
    Exec(LocalShellExecAction),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellExecAction {
    pub command: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub working_directory: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub user: Option<String>,
}

// CustomToolCall関連の構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomToolStatus {
    Completed,
    InProgress,
    Incomplete,
}

// WebSearchCall関連の構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSearchAction {
    Search { query: String },
    #[serde(other)]
    Other,
}

// codex-1レベルのResponseInputItem定義
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    CustomToolCallOutput {
        call_id: String,
        output: String,
    },
    McpToolCallOutput {
        call_id: String,
        result: Result<mcp_types::CallToolResult, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallOutputPayload {
    pub success: Option<bool>,
    pub content: String,
}

// codex-1レベルのProcessedResponseItem構造体
#[derive(Debug, Clone)]
pub struct ProcessedResponseItem {
    pub item: ResponseItem,
    pub response: Option<ResponseInputItem>,
}

// TurnRunResult構造体（ターン実行結果）
#[derive(Debug, Clone)]
pub struct TurnRunResult {
    pub processed_items: Vec<ProcessedResponseItem>,
    pub token_usage: Option<protocol::protocol::TokenUsage>,
}

#[derive(Debug, Clone, Default)]
pub struct ConversationHistory {
    items: Vec<ResponseItem>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Returns a clone of the contents in the transcript.
    pub fn contents(&self) -> Vec<ResponseItem> {
        self.items.clone()
    }

    /// Record items in the conversation history
    pub fn record_items<I>(&mut self, items: I)
    where
        I: IntoIterator<Item = ResponseItem>,
    {
        for item in items {
            if is_api_message(&item) {
                self.items.push(item);
            }
        }
    }

    /// Add a single item to the history
    pub fn add_item(&mut self, item: ResponseItem) {
        if is_api_message(&item) {
            self.items.push(item);
        }
    }

    /// Clear all items from the history
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Get the number of items in the history
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the history is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Check if an item should be included in the API conversation
fn is_api_message(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { role, .. } => {
            matches!(role.as_str(), "user" | "assistant" | "system")
        }
        ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::WebSearchCall { .. } => true,
        ResponseItem::Other => false,
    }
}

// MEE-33: 既存のConversationHistoryを拡張
impl ConversationHistory {
    pub fn extend(&mut self, items: Vec<ResponseItem>) {
        self.items.extend(items);
    }
    
    pub fn push(&mut self, item: ResponseItem) {
        self.items.push(item);
    }
    
    /// MEE-30: 履歴を完全に置き換える（auto-compact用）
    pub fn replace(&mut self, new_items: Vec<ResponseItem>) {
        self.items = new_items;
    }
}

// MEE-33: 既存のResponseInputItemを拡張
impl ResponseInputItem {
    pub fn from_text(text: String) -> Self {
        Self::Message {
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text }],
        }
    }
}

impl From<ResponseInputItem> for ResponseItem {
    fn from(item: ResponseInputItem) -> Self {
        match item {
            ResponseInputItem::Message { role, content } => {
                ResponseItem::Message { 
                    id: Some(uuid::Uuid::new_v4().to_string()),
                    role, 
                    content 
                }
            }
            ResponseInputItem::FunctionCallOutput { call_id, output } => {
                ResponseItem::FunctionCallOutput { call_id, output }
            }
            ResponseInputItem::CustomToolCallOutput { call_id, output } => {
                ResponseItem::CustomToolCallOutput { call_id, output }
            }
            ResponseInputItem::McpToolCallOutput { call_id, result } => {
                // 簡略化: McpToolCallOutputをCustomToolCallOutputに変換
                let output = match result {
                    Ok(call_result) => serde_json::to_string(&call_result).unwrap_or_default(),
                    Err(e) => format!("Error: {}", e),
                };
                ResponseItem::CustomToolCallOutput { call_id, output }
            }
        }
    }
}

impl From<Vec<protocol::protocol::InputItem>> for ResponseInputItem {
    fn from(input: Vec<protocol::protocol::InputItem>) -> Self {
        // テキストと画像を両方サポートし、1メッセージに詰める
        let mut contents: Vec<ContentItem> = Vec::new();
        for item in input.into_iter() {
            match item {
                protocol::protocol::InputItem::Text { text } => {
                    if !text.is_empty() {
                        contents.push(ContentItem::InputText { text });
                    }
                }
                protocol::protocol::InputItem::Image { image_url } => {
                    if !image_url.is_empty() {
                        contents.push(ContentItem::InputImage { image_url });
                    }
                }
            }
        }

        if contents.is_empty() {
            // 何も無ければ空テキストで構成
            return Self::Message {
                role: "user".to_string(),
                content: vec![ContentItem::InputText { text: String::new() }],
            };
        }

        Self::Message {
            role: "user".to_string(),
            content: contents,
        }
    }
}

