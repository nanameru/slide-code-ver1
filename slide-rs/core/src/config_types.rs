use crate::seatbelt::SandboxPolicy;
use serde::{Deserialize, Serialize};
use slide_common::ApprovalMode;

#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub sandbox_policy: SandboxPolicy,
    pub approval_mode: ApprovalMode,
    pub include_view_image_tool: bool,
    pub shell_environment_policy: ShellEnvironmentPolicy,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            sandbox_policy: SandboxPolicy::default(),
            approval_mode: ApprovalMode::default(),
            include_view_image_tool: false,
            shell_environment_policy: ShellEnvironmentPolicy::default(),
        }
    }
}

/// Controls which environment variables are passed to shell commands
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShellEnvironmentPolicy {
    /// Strategy for inheriting environment variables from the parent process
    pub inherit: ShellEnvironmentPolicyInherit,
    /// If true, ignore the default security excludes (KEY*, SECRET*, TOKEN*)
    pub ignore_default_excludes: bool,
    /// Additional patterns to explicitly exclude
    pub exclude: Vec<EnvironmentVariablePattern>,
    /// Additional patterns to explicitly include (overrides excludes)
    pub include: Vec<EnvironmentVariablePattern>,
    /// Additional variables to set directly
    pub set: std::collections::HashMap<String, String>,
}

impl Default for ShellEnvironmentPolicy {
    fn default() -> Self {
        Self {
            inherit: ShellEnvironmentPolicyInherit::Core,
            ignore_default_excludes: false,
            exclude: Vec::new(),
            include: Vec::new(),
            set: std::collections::HashMap::new(),
        }
    }
}

/// Strategy for inheriting environment variables
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ShellEnvironmentPolicyInherit {
    /// Inherit all environment variables from parent
    All,
    /// Inherit no environment variables
    None,
    /// Inherit only core system variables (HOME, PATH, etc.)
    Core,
}

/// Pattern for matching environment variable names
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvironmentVariablePattern {
    pattern: String,
    case_sensitive: bool,
}

impl EnvironmentVariablePattern {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            case_sensitive: true,
        }
    }

    pub fn new_case_insensitive(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            case_sensitive: false,
        }
    }

    pub fn matches(&self, name: &str) -> bool {
        if self.case_sensitive {
            glob_match(&self.pattern, name)
        } else {
            glob_match(&self.pattern.to_lowercase(), &name.to_lowercase())
        }
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.starts_with('*') && pattern.ends_with('*') {
        let middle = &pattern[1..pattern.len() - 1];
        return text.contains(middle);
    }

    if pattern.starts_with('*') {
        let suffix = &pattern[1..];
        return text.ends_with(suffix);
    }

    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        return text.starts_with(prefix);
    }

    pattern == text
}
