use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::bash_parser::{is_safe_bash_command, extract_commands};
use crate::safety_impl::{SandboxPolicy, assess_command_safety, SafetyCheck};
use crate::seatbelt::{SandboxConfig, validate_sandbox_tools};
use slide_common::ApprovalMode;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub struct ExecConfig {
    pub working_dir: Option<PathBuf>,
    pub timeout_secs: u64,
    pub sandbox: bool,
    pub network: bool,
    pub approval_mode: ApprovalMode,
    pub sandbox_policy: SandboxPolicy,
    pub environment: HashMap<String, String>,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            working_dir: None,
            timeout_secs: 30,
            sandbox: true,
            network: false,
            approval_mode: ApprovalMode::Suggest,
            sandbox_policy: SandboxPolicy::ReadOnly,
            environment: HashMap::new(),
        }
    }
}

/// Execute a command with comprehensive safety checks and sandboxing
pub async fn exec_command_safe(cmd: &str, config: ExecConfig) -> Result<ExecResult> {
    tracing::info!("Executing command: {}", cmd);
    
    // Parse and validate the command
    if !is_safe_bash_command(cmd) {
        bail!("Command failed safety analysis: {}", cmd);
    }

    let commands = extract_commands(cmd);
    if commands.is_empty() {
        bail!("No valid commands found in: {}", cmd);
    }

    // Safety assessment for each command
    let mut approved_commands = HashSet::new();
    for command in &commands {
        match assess_command_safety(
            command,
            config.approval_mode.clone(),
            &config.sandbox_policy,
            &approved_commands,
            false,
        ) {
            SafetyCheck::AutoApprove => {
                approved_commands.insert(command.clone());
            }
            SafetyCheck::AskUser => {
                // In a real implementation, this would prompt the user
                tracing::warn!("Command requires approval: {:?}", command);
                bail!("Command requires user approval: {}", command.join(" "));
            }
            SafetyCheck::Reject { reason } => {
                bail!("Command rejected: {} ({})", command.join(" "), reason);
            }
        }
    }

    // Prepare the execution environment
    let mut final_cmd = prepare_command(cmd, &config).await?;
    
    // Execute with timeout
    let start_time = std::time::Instant::now();
    let output = timeout(
        Duration::from_secs(config.timeout_secs),
        final_cmd.output()
    )
    .await
    .context("Command timed out")?
    .context("Failed to execute command")?;

    let duration = start_time.elapsed();
    
    let result = ExecResult {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        duration,
    };

    tracing::info!(
        "Command completed: status={}, duration={:?}",
        result.status,
        result.duration
    );

    Ok(result)
}

async fn prepare_command(cmd: &str, config: &ExecConfig) -> Result<Command> {
    let mut command;

    if config.sandbox {
        command = prepare_sandboxed_command(cmd, config).await?;
    } else {
        // Direct execution
        command = Command::new("sh");
        command.arg("-c").arg(cmd);
    }

    // Set working directory
    if let Some(ref cwd) = config.working_dir {
        command.current_dir(cwd);
    }

    // Set environment variables
    for (key, value) in &config.environment {
        command.env(key, value);
    }

    // Configure stdio
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::null());

    Ok(command)
}

async fn prepare_sandboxed_command(cmd: &str, config: &ExecConfig) -> Result<Command> {
    let sandbox_config = SandboxConfig::new(config.sandbox_policy)
        .with_workspace(config.working_dir.clone().unwrap_or_else(|| PathBuf::from(".")))
        .with_network_access(config.network);

    if let Some(sandbox_args) = sandbox_config.build_command() {
        // Validate sandbox tools are available
        if let Err(e) = validate_sandbox_tools() {
            tracing::warn!("Sandbox tools not available: {}, falling back to direct execution", e);
            let mut command = Command::new("sh");
            command.arg("-c").arg(cmd);
            return Ok(command);
        }

        let mut command = Command::new(&sandbox_args[0]);
        
        // Add sandbox arguments
        for arg in &sandbox_args[1..] {
            command.arg(arg);
        }
        
        // Add the actual command
        command.arg("--");
        command.arg("sh");
        command.arg("-c");
        command.arg(cmd);

        Ok(command)
    } else {
        // No sandboxing available for this platform/policy
        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);
        Ok(command)
    }
}

/// Execute a single command with streaming output
pub async fn exec_command_streaming(
    cmd: &str,
    config: ExecConfig,
) -> Result<tokio::process::Child> {
    tracing::info!("Starting streaming execution: {}", cmd);
    
    // Safety checks
    if !is_safe_bash_command(cmd) {
        bail!("Command failed safety analysis: {}", cmd);
    }

    let mut final_cmd = prepare_command(cmd, &config).await?;
    
    // Start the process
    let child = final_cmd
        .spawn()
        .context("Failed to spawn command")?;

    Ok(child)
}

/// Execute multiple commands in sequence
pub async fn exec_commands_sequence(
    commands: &[&str],
    config: ExecConfig,
) -> Result<Vec<ExecResult>> {
    let mut results = Vec::new();
    
    for cmd in commands {
        let result = exec_command_safe(cmd, config.clone()).await?;
        
        // Stop on failure unless configured otherwise
        if result.status != 0 {
            tracing::warn!("Command failed with status {}: {}", result.status, cmd);
            // In production, might want to make this configurable
        }
        
        results.push(result);
    }
    
    Ok(results)
}

/// Check if a command would be allowed to execute
pub fn check_command_permission(cmd: &str, config: &ExecConfig) -> Result<bool> {
    if !is_safe_bash_command(cmd) {
        return Ok(false);
    }

    let commands = extract_commands(cmd);
    let approved_commands = HashSet::new();
    
    for command in commands {
        match assess_command_safety(
            &command,
            config.approval_mode.clone(),
            &config.sandbox_policy,
            &approved_commands,
            false,
        ) {
            SafetyCheck::AutoApprove => continue,
            SafetyCheck::AskUser => return Ok(false), // Would need approval
            SafetyCheck::Reject { .. } => return Ok(false),
        }
    }
    
    Ok(true)
}

/// Create a temporary workspace for command execution
pub async fn create_temp_workspace() -> Result<tempfile::TempDir> {
    let temp_dir = tempfile::TempDir::new()
        .context("Failed to create temporary workspace")?;
    
    tracing::info!("Created temporary workspace: {}", temp_dir.path().display());
    Ok(temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_simple_command() {
        let config = ExecConfig::default();
        let result = exec_command_safe("echo 'hello world'", config).await.unwrap();
        
        assert_eq!(result.status, 0);
        assert!(result.stdout.contains("hello world"));
    }

    #[tokio::test]
    async fn test_dangerous_command_rejection() {
        let config = ExecConfig::default();
        let result = exec_command_safe("rm -rf /", config).await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_command_permission_check() {
        let config = ExecConfig::default();
        
        assert!(check_command_permission("ls -la", &config).unwrap());
        assert!(!check_command_permission("sudo rm -rf /", &config).unwrap());
    }

    #[tokio::test]
    async fn test_temp_workspace() {
        let workspace = create_temp_workspace().await.unwrap();
        assert!(workspace.path().exists());
    }
}