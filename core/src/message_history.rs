use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct MessageHistory {
    messages: VecDeque<Message>,
    max_messages: usize,
}

impl MessageHistory {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: VecDeque::new(),
            max_messages,
        }
    }

    pub fn add_message(&mut self, role: String, content: String) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let message = Message { role, content, timestamp };
        
        self.messages.push_back(message);
        
        while self.messages.len() > self.max_messages {
            self.messages.pop_front();
        }
    }

    pub fn get_messages(&self) -> Vec<&Message> {
        self.messages.iter().collect()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}