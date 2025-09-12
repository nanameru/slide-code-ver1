#[derive(Debug, Clone)]
pub struct OpenAiModelInfo {
    pub model: String,
    pub max_output_tokens: u32,
}

impl OpenAiModelInfo {
    pub fn new(model: String, max_output_tokens: u32) -> Self {
        Self { model, max_output_tokens }
    }
    
    pub fn gpt4o_mini() -> Self {
        Self::new("gpt-4o-mini".to_string(), 16384)
    }
    
    pub fn gpt4o() -> Self {
        Self::new("gpt-4o".to_string(), 4096)
    }
}