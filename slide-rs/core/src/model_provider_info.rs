use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderInfo {
    pub name: String,
    pub base_url: String,
    pub api_key_env_var: Option<String>,
    pub supported_models: Vec<String>,
}

impl ModelProviderInfo {
    pub fn new(name: String, base_url: String) -> Self {
        Self {
            name,
            base_url,
            api_key_env_var: None,
            supported_models: Vec::new(),
        }
    }
}

pub fn built_in_model_providers() -> HashMap<String, ModelProviderInfo> {
    let mut providers = HashMap::new();
    
    providers.insert(
        "openai".to_string(),
        ModelProviderInfo {
            name: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key_env_var: Some("OPENAI_API_KEY".to_string()),
            supported_models: vec![
                "gpt-4".to_string(),
                "gpt-4-turbo".to_string(),
                "gpt-3.5-turbo".to_string(),
            ],
        },
    );
    
    providers.insert(
        "anthropic".to_string(),
        ModelProviderInfo {
            name: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key_env_var: Some("ANTHROPIC_API_KEY".to_string()),
            supported_models: vec![
                "claude-3-haiku".to_string(),
                "claude-3-sonnet".to_string(),
                "claude-3-opus".to_string(),
            ],
        },
    );
    
    providers
}

