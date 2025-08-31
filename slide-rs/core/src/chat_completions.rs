use anyhow::Result;
use slide_chatgpt::{ChatGptClient, SlideRequest};
use slide_common::SlideConfig;

pub async fn generate_slides_from_prompt(prompt: &str) -> Result<String> {
    // Load config to get API key
    let config = SlideConfig::load().await?;
    
    let api_key = config.api_key.ok_or_else(|| {
        anyhow::anyhow!("No API key configured. Please run 'slide login --api-key <your-key>'")
    })?;
    
    let client = ChatGptClient::new(api_key);
    
    let request = SlideRequest {
        prompt: prompt.to_string(),
        num_slides: 5, // Default number
        language: "English".to_string(),
    };
    
    let response = client.generate_slides(request).await?;
    Ok(response.markdown)
}