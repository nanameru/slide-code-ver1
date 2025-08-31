use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SlideRequest {
    pub prompt: String,
    pub num_slides: usize,
    pub language: String,
}

#[derive(Debug, Deserialize)]
pub struct SlideResponse {
    pub markdown: String,
}

pub struct ChatGptClient {
    #[allow(dead_code)]
    api_key: String,
}

impl ChatGptClient {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
    
    pub async fn generate_slides(&self, request: SlideRequest) -> Result<SlideResponse> {
        // TODO: Implement actual OpenAI API call
        // For now, return a mock response
        let mock_markdown = format!(
            r#"# {}

## Slide 1: Introduction
- Point A
- Point B

## Slide 2: Content
- Content point 1
- Content point 2

"#,
            request.prompt
        );
        
        Ok(SlideResponse {
            markdown: mock_markdown,
        })
    }
}