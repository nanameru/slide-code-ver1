use std::fmt;
use thiserror::Error;

/// Core error types for the SLIDE execution engine
#[derive(Error, Debug)]
pub enum SlideError {
    /// Execution-related errors
    #[error("Execution failed: {0}")]
    Execution(#[from] ExecError),

    /// Sandbox-related errors
    #[error("Sandbox error: {0}")]
    Sandbox(#[from] SandboxError),

    /// Tool-related errors
    #[error("Tool error: {0}")]
    Tool(#[from] ToolError),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Generic errors
    #[error("Error: {0}")]
    Generic(#[from] anyhow::Error),
}

/// Execution-specific errors
#[derive(Error, Debug)]
pub enum ExecError {
    #[error("Command not found: {command}")]
    CommandNotFound { command: String },

    #[error("Command timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("Command failed with exit code {exit_code}: {stderr}")]
    CommandFailed { exit_code: i32, stderr: String },

    #[error("Failed to spawn process: {source}")]
    SpawnFailed {
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to capture output: {reason}")]
    OutputCaptureFailed { reason: String },

    #[error("Command rejected by safety policy: {reason}")]
    SafetyRejection { reason: String },

    #[error("Permission denied: {reason}")]
    PermissionDenied { reason: String },

    #[error("Working directory not accessible: {path}")]
    WorkingDirectoryError { path: String },

    #[error("Environment setup failed: {reason}")]
    EnvironmentError { reason: String },
}

/// Sandbox-specific errors
#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Sandbox policy invalid: {reason}")]
    InvalidPolicy { reason: String },

    #[error("Sandbox setup failed: {reason}")]
    SetupFailed { reason: String },

    #[error("Sandbox enforcement failed: {reason}")]
    EnforcementFailed { reason: String },

    #[error("macOS Seatbelt error: {reason}")]
    SeatbeltError { reason: String },

    #[error("Linux Landlock error: {reason}")]
    LandlockError { reason: String },

    #[error("Linux Seccomp error: {reason}")]
    SeccompError { reason: String },

    #[error("Sandbox tool not available: {tool}")]
    ToolNotAvailable { tool: String },

    #[error("Sandbox violation: {violation}")]
    Violation { violation: String },
}

/// Tool-specific errors
#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool not found: {tool_name}")]
    NotFound { tool_name: String },

    #[error("Invalid tool arguments: {reason}")]
    InvalidArguments { reason: String },

    #[error("Tool execution failed: {tool_name}, reason: {reason}")]
    ExecutionFailed {
        tool_name: String,
        reason: String,
    },

    #[error("Tool timeout: {tool_name} timed out after {timeout_ms}ms")]
    Timeout {
        tool_name: String,
        timeout_ms: u64,
    },

    #[error("Tool output parsing failed: {reason}")]
    OutputParsingFailed { reason: String },

    #[error("Tool initialization failed: {tool_name}, reason: {reason}")]
    InitializationFailed {
        tool_name: String,
        reason: String,
    },

    #[error("Tool configuration invalid: {reason}")]
    InvalidConfiguration { reason: String },

    #[error("File operation failed: {operation} on {path}: {reason}")]
    FileOperationFailed {
        operation: String,
        path: String,
        reason: String,
    },

    #[error("Patch application failed: {reason}")]
    PatchFailed { reason: String },
}

/// Configuration-specific errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing required configuration: {field}")]
    MissingField { field: String },

    #[error("Invalid configuration value: {field} = {value}")]
    InvalidValue { field: String, value: String },

    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },

    #[error("Configuration file invalid: {reason}")]
    InvalidFile { reason: String },

    #[error("Environment variable not set: {var}")]
    EnvironmentVariable { var: String },

    #[error("Path does not exist: {path}")]
    PathNotFound { path: String },

    #[error("Permission denied accessing: {path}")]
    PermissionDenied { path: String },
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, SlideError>;

/// Error reporting utilities
pub struct ErrorReporter;

impl ErrorReporter {
    /// Format error for user display
    pub fn format_user_error(error: &SlideError) -> String {
        match error {
            SlideError::Execution(exec_err) => {
                Self::format_execution_error(exec_err)
            }
            SlideError::Sandbox(sandbox_err) => {
                Self::format_sandbox_error(sandbox_err)
            }
            SlideError::Tool(tool_err) => {
                Self::format_tool_error(tool_err)
            }
            SlideError::Config(config_err) => {
                Self::format_config_error(config_err)
            }
            SlideError::Io(io_err) => {
                format!("File operation failed: {}", io_err)
            }
            SlideError::Json(json_err) => {
                format!("Data format error: {}", json_err)
            }
            SlideError::Generic(anyhow_err) => {
                format!("Unexpected error: {}", anyhow_err)
            }
        }
    }

    fn format_execution_error(error: &ExecError) -> String {
        match error {
            ExecError::CommandNotFound { command } => {
                format!(
                    "❌ Command not found: '{}'\n💡 Make sure the command is installed and in your PATH",
                    command
                )
            }
            ExecError::Timeout { timeout_ms } => {
                format!(
                    "⏱️  Command timed out after {}ms\n💡 Try increasing the timeout or check if the command is hanging",
                    timeout_ms
                )
            }
            ExecError::CommandFailed { exit_code, stderr } => {
                format!(
                    "❌ Command failed with exit code {}\n📝 Error output: {}",
                    exit_code, stderr
                )
            }
            ExecError::SafetyRejection { reason } => {
                format!(
                    "🛡️  Command blocked for safety: {}\n💡 Review the command or adjust safety settings",
                    reason
                )
            }
            _ => format!("❌ Execution error: {}", error),
        }
    }

    fn format_sandbox_error(error: &SandboxError) -> String {
        match error {
            SandboxError::ToolNotAvailable { tool } => {
                format!(
                    "🔒 Sandbox tool not available: {}\n💡 Install the required sandbox tool or disable sandboxing",
                    tool
                )
            }
            SandboxError::Violation { violation } => {
                format!(
                    "🚫 Sandbox violation: {}\n💡 The command tried to access restricted resources",
                    violation
                )
            }
            _ => format!("🔒 Sandbox error: {}", error),
        }
    }

    fn format_tool_error(error: &ToolError) -> String {
        match error {
            ToolError::NotFound { tool_name } => {
                format!(
                    "🔧 Tool not found: {}\n💡 Make sure the tool is properly configured",
                    tool_name
                )
            }
            ToolError::InvalidArguments { reason } => {
                format!(
                    "⚙️  Invalid tool arguments: {}\n💡 Check the tool documentation for correct usage",
                    reason
                )
            }
            ToolError::FileOperationFailed { operation, path, reason } => {
                format!(
                    "📁 File operation failed: {} on {}\n📝 Reason: {}\n💡 Check file permissions and path existence",
                    operation, path, reason
                )
            }
            _ => format!("🔧 Tool error: {}", error),
        }
    }

    fn format_config_error(error: &ConfigError) -> String {
        match error {
            ConfigError::MissingField { field } => {
                format!(
                    "⚙️  Missing configuration: {}\n💡 Add the required configuration field",
                    field
                )
            }
            ConfigError::FileNotFound { path } => {
                format!(
                    "📄 Configuration file not found: {}\n💡 Create the configuration file or check the path",
                    path
                )
            }
            ConfigError::PermissionDenied { path } => {
                format!(
                    "🔐 Permission denied: {}\n💡 Check file permissions or run with appropriate privileges",
                    path
                )
            }
            _ => format!("⚙️  Configuration error: {}", error),
        }
    }

    /// Get error severity level
    pub fn get_severity(error: &SlideError) -> ErrorSeverity {
        match error {
            SlideError::Execution(ExecError::SafetyRejection { .. }) => ErrorSeverity::Warning,
            SlideError::Execution(ExecError::Timeout { .. }) => ErrorSeverity::Warning,
            SlideError::Sandbox(SandboxError::Violation { .. }) => ErrorSeverity::Critical,
            SlideError::Config(ConfigError::MissingField { .. }) => ErrorSeverity::Error,
            SlideError::Io(_) => ErrorSeverity::Error,
            _ => ErrorSeverity::Error,
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Info => write!(f, "ℹ️  INFO"),
            ErrorSeverity::Warning => write!(f, "⚠️  WARNING"),
            ErrorSeverity::Error => write!(f, "❌ ERROR"),
            ErrorSeverity::Critical => write!(f, "🚨 CRITICAL"),
        }
    }
}