use crate::approval_manager::AskForApproval;
use crate::config_types::{CoreConfig, ShellEnvironmentPolicy};
use crate::seatbelt::SandboxPolicy;
use slide_common::ApprovalMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config file not found: {path}")]
    FileNotFound { path: String },
    #[error("Failed to read config file: {source}")]
    IoError { source: std::io::Error },
    #[error("Failed to parse config file: {source}")]
    ParseError { source: serde_json::Error },
    #[error("Failed to parse TOML config: {source}")]
    TomlParseError { source: toml::de::Error },
}

/// Enhanced configuration with codex-style settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Sandbox policy for command execution
    pub sandbox_policy: SandboxPolicy,
    /// Approval policy for commands and patches
    pub approval_policy: AskForApproval,
    /// Shell environment policy
    pub shell_environment_policy: ShellEnvironmentPolicy,
    /// Whether to include view image tool
    pub include_view_image_tool: bool,
    /// Current working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// User-defined instructions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_instructions: Option<String>,
    /// Maximum bytes for project documentation
    pub project_doc_max_bytes: usize,
    /// Whether to disable response storage
    pub disable_response_storage: bool,
    /// AI model to use
    pub model: String,
    /// Model provider
    pub model_provider: String,
    /// Configuration profiles
    #[serde(default)]
    pub profiles: HashMap<String, ConfigProfile>,
}

/// Configuration profile for different scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigProfile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_policy: Option<SandboxPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<AskForApproval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_environment_policy: Option<ShellEnvironmentPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = HashMap::new();

        // Add default profiles
        profiles.insert("readonly".to_string(), ConfigProfile {
            sandbox_policy: Some(SandboxPolicy::ReadOnly),
            approval_policy: Some(AskForApproval::OnRequest),
            shell_environment_policy: None,
            model: None,
        });

        profiles.insert("full-auto".to_string(), ConfigProfile {
            sandbox_policy: Some(SandboxPolicy::WorkspaceWrite {
                writable_roots: vec![],
                network_access: false,
                exclude_tmpdir_env_var: false,
                exclude_system_tmp: false,
            }),
            approval_policy: Some(AskForApproval::OnFailure),
            shell_environment_policy: None,
            model: None,
        });

        profiles.insert("danger".to_string(), ConfigProfile {
            sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
            approval_policy: Some(AskForApproval::Never),
            shell_environment_policy: None,
            model: None,
        });

        Self {
            sandbox_policy: SandboxPolicy::default(),
            approval_policy: AskForApproval::default(),
            shell_environment_policy: ShellEnvironmentPolicy::default(),
            include_view_image_tool: false,
            cwd: None,
            user_instructions: None,
            project_doc_max_bytes: 32 * 1024,
            disable_response_storage: false,
            model: "gpt-5".to_string(),
            model_provider: "openai".to_string(),
            profiles,
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::IoError { source: e })?;

        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            toml::from_str(&contents)
                .map_err(|e| ConfigError::TomlParseError { source: e })
        } else {
            serde_json::from_str(&contents)
                .map_err(|e| ConfigError::ParseError { source: e })
        }
    }

    /// Load configuration with default fallback
    pub fn load_with_fallback() -> Self {
        // Try to load from various locations
        let config_paths = [
            ".slide/config.json",
            ".slide/config.toml",
            "slide.config.json",
            "slide.config.toml",
        ];

        for path in &config_paths {
            if Path::new(path).exists() {
                match Self::load_from_file(path) {
                    Ok(config) => {
                        tracing::info!("Loaded configuration from {}", path);
                        return config;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load config from {}: {}", path, e);
                    }
                }
            }
        }

        tracing::info!("Using default configuration");
        Self::default()
    }

    /// Apply a profile to this configuration
    pub fn apply_profile(&mut self, profile_name: &str) -> Result<(), ConfigError> {
        if let Some(profile) = self.profiles.get(profile_name).cloned() {
            if let Some(sandbox_policy) = profile.sandbox_policy {
                self.sandbox_policy = sandbox_policy;
            }
            if let Some(approval_policy) = profile.approval_policy {
                self.approval_policy = approval_policy;
            }
            if let Some(shell_env_policy) = profile.shell_environment_policy {
                self.shell_environment_policy = shell_env_policy;
            }
            if let Some(model) = profile.model {
                self.model = model;
            }
            Ok(())
        } else {
            Err(ConfigError::FileNotFound {
                path: format!("profile '{}'", profile_name),
            })
        }
    }

    /// Save configuration to file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let contents = if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            toml::to_string_pretty(self)
                .map_err(|e| ConfigError::ParseError {
                    source: serde_json::Error::custom(e.to_string())
                })?
        } else {
            serde_json::to_string_pretty(self)
                .map_err(|e| ConfigError::ParseError { source: e })?
        };

        std::fs::write(path, contents)
            .map_err(|e| ConfigError::IoError { source: e })
    }

    /// Get current working directory with fallback
    pub fn get_cwd(&self) -> PathBuf {
        self.cwd.clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// Load legacy core config (for compatibility)
pub fn load_core_config() -> CoreConfig {
    CoreConfig::default()
}

/// Load legacy config (for compatibility)
pub fn load_config() -> Config {
    Config::load_with_fallback()
}

/// Configuration manager for handling runtime configuration changes
pub struct ConfigManager {
    config: Config,
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self {
            config: Config::load_with_fallback(),
        }
    }
}

impl ConfigManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        Ok(Self {
            config: Config::load_from_file(path)?,
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn apply_profile(&mut self, profile_name: &str) -> Result<(), ConfigError> {
        self.config.apply_profile(profile_name)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        self.config.save_to_file(path)
    }

    /// Create a sample configuration file
    pub fn create_sample_config<P: AsRef<Path>>(path: P) -> Result<(), ConfigError> {
        let sample_config = Config::default();
        sample_config.save_to_file(path)
    }
}

