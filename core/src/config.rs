use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub api_key: Option<String>,
    pub model: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub approval_mode: Option<String>,
    pub log_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            model: "gpt-4o-mini".to_string(),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            approval_mode: None,
            log_path: None,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            config.api_key = Some(key);
        }
        
        if let Ok(model) = std::env::var("SLIDE_MODEL") {
            config.model = model;
        }
        
        if let Ok(mode) = std::env::var("SLIDE_APPROVAL_MODE") {
            config.approval_mode = Some(mode);
        }
        
        if let Ok(path) = std::env::var("SLIDE_LOG_PATH") {
            config.log_path = Some(PathBuf::from(path));
        }
        
        config
    }
}