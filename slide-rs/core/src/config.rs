use crate::config_types::CoreConfig;
use crate::seatbelt::SandboxPolicy;
use slide_common::ApprovalMode;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub sandbox_policy: SandboxPolicy,
    pub approval_policy: ApprovalMode,
    pub include_view_image_tool: bool,
    pub cwd: PathBuf,
    pub user_instructions: Option<String>,
    pub project_doc_max_bytes: usize,
    pub disable_response_storage: bool,
    pub model: String,
    pub model_provider: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sandbox_policy: SandboxPolicy::WorkspaceWrite,
            approval_policy: ApprovalMode::default(),
            include_view_image_tool: false,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            user_instructions: None,
            project_doc_max_bytes: 32 * 1024,
            disable_response_storage: false,
            model: "gpt-5".to_string(),
            model_provider: "openai".to_string(),
        }
    }
}

pub fn load_core_config() -> CoreConfig {
    CoreConfig::default()
}

pub fn load_config() -> Config {
    Config::default()
}

