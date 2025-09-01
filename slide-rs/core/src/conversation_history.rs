use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResponseItem {
    pub id: Option<String>,
    pub role: String,
    pub content: Vec<ContentItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentItem {
    Text { text: String },
    FunctionCall { name: String, arguments: String },
    FunctionResult { result: String },
}

pub type ResponseInputItem = ResponseItem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallOutputPayload {
    pub success: bool,
    pub message: String,
    pub output: Option<String>,
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
    // Include messages from user and assistant roles
    matches!(item.role.as_str(), "user" | "assistant" | "system")
}

