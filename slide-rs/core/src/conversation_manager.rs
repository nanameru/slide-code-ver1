#[derive(Debug, Clone, Default)]
pub struct ConversationManager {
    conversations: Vec<String>,
}

impl ConversationManager {
    pub fn new() -> Self { Self::default() }
    pub fn start(&mut self, id: String) { self.conversations.push(id); }
}

