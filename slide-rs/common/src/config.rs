use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Slide configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideConfig {
    pub api_key: Option<String>,
    pub model: String,
    pub approval_mode: String,
    pub output_dir: PathBuf,
}

impl Default for SlideConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            model: "gpt-5".to_string(),
            approval_mode: "suggest".to_string(),
            output_dir: PathBuf::from("slides"),
        }
    }
}

impl SlideConfig {
    /// Get config file path
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?
            .join("slide");
        
        std::fs::create_dir_all(&config_dir)?;
        Ok(config_dir.join("config.json"))
    }

    /// Load configuration from file
    pub async fn load() -> Result<Self> {
        let path = Self::config_path()?;
        
        if path.exists() {
            let content = tokio::fs::read_to_string(&path).await?;
            let config: SlideConfig = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Save configuration to file
    pub async fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }
}