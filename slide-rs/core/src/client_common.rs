use crate::conversation_history::ResponseItem;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseEvent {
    Message(ResponseItem),
    FunctionCall { name: String, arguments: String },
    FunctionResult { result: String },
    Complete,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct Prompt {
    /// Conversation context input items.
    pub input: Vec<ResponseItem>,
    /// Whether to store response on server side (disable_response_storage = !store).
    pub store: bool,
    /// Tools available to the model, including additional tools sourced from
    /// external MCP servers.
    pub tools: Vec<OpenAiTool>,
    /// Optional override for the built-in BASE_INSTRUCTIONS.
    pub base_instructions_override: Option<String>,
}

impl Prompt {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_input(mut self, input: Vec<ResponseItem>) -> Self {
        self.input = input;
        self
    }

    pub fn with_store(mut self, store: bool) -> Self {
        self.store = store;
        self
    }

    pub fn with_tools(mut self, tools: Vec<OpenAiTool>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_base_instructions_override(mut self, instructions: Option<String>) -> Self {
        self.base_instructions_override = instructions;
        self
    }

    pub fn get_full_instructions(&self) -> String {
        const BASE_INSTRUCTIONS: &str = "You are Claude, an AI assistant created by Anthropic.";
        
        self.base_instructions_override
            .as_deref()
            .unwrap_or(BASE_INSTRUCTIONS)
            .to_string()
    }

    pub fn render(&self) -> String {
        let instructions = self.get_full_instructions();
        let messages: Vec<String> = self.input
            .iter()
            .map(|item| format!("{}: {:?}", item.role, item.content))
            .collect();
        
        format!("{}\n\nConversation:\n{}", instructions, messages.join("\n"))
    }
}

