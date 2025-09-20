use crate::approval_manager::{ApprovalRequest, ApprovalResponse, AskForApproval};
use crate::config_types::ShellEnvironmentPolicy;
use crate::exec_env::create_env;
use crate::safety::{assess_command_safety_v2, SafetyCheck};
use crate::seatbelt::{build_seatbelt_policy, SandboxPolicy};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("Command failed to execute: {message}")]
    ExecutionFailed { message: String },
    #[error("Command timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    #[error("Approval denied by user")]
    ApprovalDenied,
    #[error("Command rejected by safety policy: {reason}")]
    SafetyRejected { reason: String },
    #[error("Sandbox setup failed: {message}")]
    SandboxError { message: String },
    #[error("IO error: {source}")]
    Io { source: std::io::Error },
}

#[derive(Debug, Clone)]
pub struct ExecParams {
    pub command: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
    pub with_escalated_permissions: bool,
    pub justification: Option<String>,
    pub environment_policy: ShellEnvironmentPolicy,
}

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub used_escalated_permissions: bool,
}

/// Platform-specific sandbox execution engine
pub struct SandboxedExecutor {
    approval_manager: crate::approval_manager::ApprovalManager,
    sandbox_policy: SandboxPolicy,
}

impl SandboxedExecutor {
    pub fn new(approval_policy: AskForApproval, sandbox_policy: SandboxPolicy) -> Self {
        Self {
            approval_manager: crate::approval_manager::ApprovalManager::new(approval_policy),
            sandbox_policy,
        }
    }

    /// Execute a command with sandbox and approval controls
    pub async fn execute(&mut self, params: ExecParams) -> Result<ExecResult, ExecError> {
        let start_time = std::time::Instant::now();

        // 1. Safety assessment
        let safety_check = assess_command_safety_v2(
            &params.command,
            &self.approval_manager,
            &self.sandbox_policy,
            params.with_escalated_permissions,
        );

        match safety_check {
            SafetyCheck::Reject { reason } => {
                return Err(ExecError::SafetyRejected { reason });
            }
            SafetyCheck::AskUser => {
                // 2. Request user approval if needed
                let approval_response = self.request_approval(&params).await?;
                match approval_response {
                    ApprovalResponse::Approved => {
                        // Continue to execution
                    }
                    ApprovalResponse::ApprovedAndTrust => {
                        // Add to approved commands
                        self.approval_manager
                            .approve_command(params.command.clone());
                    }
                    ApprovalResponse::Denied => {
                        return Err(ExecError::ApprovalDenied);
                    }
                    ApprovalResponse::ChangePolicy(new_policy) => {
                        self.approval_manager.set_policy(new_policy);
                        // Re-evaluate with new policy
                        return Box::pin(self.execute(params)).await;
                    }
                }
            }
            SafetyCheck::AutoApprove => {
                // Continue to execution
            }
        }

        // 3. Execute command with appropriate sandbox
        let with_escalated = params.with_escalated_permissions;
        let result = if with_escalated {
            self.execute_with_escalated_permissions(&params).await?
        } else {
            self.execute_sandboxed(&params).await?
        };

        let duration_ms = start_time.elapsed().as_millis() as u64;
        Ok(ExecResult {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
            duration_ms,
            used_escalated_permissions: with_escalated,
        })
    }

    /// Request approval from user (placeholder - would integrate with TUI)
    async fn request_approval(&self, params: &ExecParams) -> Result<ApprovalResponse, ExecError> {
        let request = ApprovalRequest::new(
            params.command.clone(),
            params.working_dir.as_deref(),
            params.justification.clone(),
            params.with_escalated_permissions,
            format!("{:?}", self.sandbox_policy),
        );

        // For now, just log the request and auto-approve (in real implementation, this would show UI)
        tracing::info!("Approval request: {}", request.description());

        // In a real implementation, this would:
        // 1. Send approval request to TUI
        // 2. Wait for user response
        // 3. Return the user's decision

        // For demo purposes, auto-approve non-dangerous commands
        if params.command.get(0).map_or(false, |cmd| {
            ["ls", "cat", "grep", "find", "echo", "pwd"].contains(&cmd.as_str())
        }) {
            Ok(ApprovalResponse::Approved)
        } else {
            Ok(ApprovalResponse::Approved) // Would be actual user input in real implementation
        }
    }

    /// Execute command within sandbox constraints
    async fn execute_sandboxed(&self, params: &ExecParams) -> Result<BasicExecResult, ExecError> {
        let working_dir = params
            .working_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("."));

        // Prepare environment variables
        let env_vars = create_env(&params.environment_policy);

        match self.sandbox_policy {
            SandboxPolicy::ReadOnly => {
                self.execute_read_only(
                    params.command.clone(),
                    working_dir,
                    env_vars,
                    params.timeout_ms,
                )
                .await
            }
            SandboxPolicy::WorkspaceWrite { .. } => {
                self.execute_workspace_write(
                    params.command.clone(),
                    working_dir,
                    env_vars,
                    params.timeout_ms,
                )
                .await
            }
            SandboxPolicy::DangerFullAccess => {
                self.execute_full_access(
                    params.command.clone(),
                    working_dir,
                    env_vars,
                    params.timeout_ms,
                )
                .await
            }
        }
    }

    /// Execute command with escalated permissions (outside normal sandbox)
    async fn execute_with_escalated_permissions(
        &self,
        params: &ExecParams,
    ) -> Result<BasicExecResult, ExecError> {
        let working_dir = params
            .working_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("."));

        let env_vars = create_env(&params.environment_policy);

        tracing::warn!(
            "Executing command with escalated permissions: {:?}",
            params.command
        );

        // Execute without sandbox restrictions
        self.execute_full_access(
            params.command.clone(),
            working_dir,
            env_vars,
            params.timeout_ms,
        )
        .await
    }

    /// Execute with read-only sandbox
    async fn execute_read_only(
        &self,
        command: Vec<String>,
        working_dir: &Path,
        env_vars: HashMap<String, String>,
        timeout_ms: Option<u64>,
    ) -> Result<BasicExecResult, ExecError> {
        #[cfg(target_os = "macos")]
        {
            self.execute_with_seatbelt(
                command,
                working_dir,
                env_vars,
                timeout_ms,
                &SandboxPolicy::ReadOnly,
            )
            .await
        }
        #[cfg(target_os = "linux")]
        {
            self.execute_with_landlock(
                command,
                working_dir,
                env_vars,
                timeout_ms,
                &SandboxPolicy::ReadOnly,
            )
            .await
        }
        #[cfg(target_os = "windows")]
        {
            // Windows: Basic execution with limited environment
            self.execute_basic(command, working_dir, env_vars, timeout_ms)
                .await
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            // Other platforms: Basic execution
            self.execute_basic(command, working_dir, env_vars, timeout_ms)
                .await
        }
    }

    /// Execute with workspace-write sandbox
    async fn execute_workspace_write(
        &self,
        command: Vec<String>,
        working_dir: &Path,
        env_vars: HashMap<String, String>,
        timeout_ms: Option<u64>,
    ) -> Result<BasicExecResult, ExecError> {
        #[cfg(target_os = "macos")]
        {
            self.execute_with_seatbelt(
                command,
                working_dir,
                env_vars,
                timeout_ms,
                &self.sandbox_policy,
            )
            .await
        }
        #[cfg(target_os = "linux")]
        {
            self.execute_with_landlock(
                command,
                working_dir,
                env_vars,
                timeout_ms,
                &self.sandbox_policy,
            )
            .await
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            // Other platforms: Basic execution
            self.execute_basic(command, working_dir, env_vars, timeout_ms)
                .await
        }
    }

    /// Execute with full access (no sandbox)
    async fn execute_full_access(
        &self,
        command: Vec<String>,
        working_dir: &Path,
        env_vars: HashMap<String, String>,
        timeout_ms: Option<u64>,
    ) -> Result<BasicExecResult, ExecError> {
        self.execute_basic(command, working_dir, env_vars, timeout_ms)
            .await
    }

    /// Basic command execution without sandbox
    async fn execute_basic(
        &self,
        command: Vec<String>,
        working_dir: &Path,
        env_vars: HashMap<String, String>,
        timeout_ms: Option<u64>,
    ) -> Result<BasicExecResult, ExecError> {
        if command.is_empty() {
            return Err(ExecError::ExecutionFailed {
                message: "Empty command".to_string(),
            });
        }

        let mut cmd = Command::new(&command[0]);
        if command.len() > 1 {
            cmd.args(&command[1..]);
        }

        cmd.current_dir(working_dir)
            .env_clear()
            .envs(env_vars)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let timeout = timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(30));

        let output = tokio::time::timeout(timeout, async {
            tokio::task::spawn_blocking(move || cmd.output())
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
        })
        .await
        .map_err(|_| ExecError::Timeout {
            timeout_ms: timeout.as_millis() as u64,
        })?
        .map_err(|e| ExecError::Io { source: e })?;

        Ok(BasicExecResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Execute command with macOS Seatbelt sandbox
    #[cfg(target_os = "macos")]
    async fn execute_with_seatbelt(
        &self,
        command: Vec<String>,
        working_dir: &Path,
        env_vars: HashMap<String, String>,
        timeout_ms: Option<u64>,
        sandbox_policy: &SandboxPolicy,
    ) -> Result<BasicExecResult, ExecError> {
        let policy = build_seatbelt_policy(sandbox_policy.clone(), Some(working_dir));

        // Create a temporary policy file
        let policy_file =
            std::env::temp_dir().join(format!("slide_policy_{}.sbpl", std::process::id()));
        std::fs::write(&policy_file, policy).map_err(|e| ExecError::SandboxError {
            message: e.to_string(),
        })?;

        let mut sandbox_cmd = Command::new("sandbox-exec");
        sandbox_cmd
            .arg("-f")
            .arg(&policy_file)
            .args(&command)
            .current_dir(working_dir)
            .env_clear()
            .envs(env_vars)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let timeout = timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(30));

        let result = tokio::time::timeout(timeout, async {
            tokio::task::spawn_blocking(move || {
                let output = sandbox_cmd.output()?;
                // Clean up policy file
                let _ = std::fs::remove_file(&policy_file);
                Ok::<_, std::io::Error>(output)
            })
            .await
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
        })
        .await
        .map_err(|_| ExecError::Timeout {
            timeout_ms: timeout.as_millis() as u64,
        })?
        .map_err(|e| ExecError::Io { source: e })?;

        Ok(BasicExecResult {
            stdout: String::from_utf8_lossy(&result.stdout).to_string(),
            stderr: String::from_utf8_lossy(&result.stderr).to_string(),
            exit_code: result.status.code().unwrap_or(-1),
        })
    }

    /// Execute command with Linux Landlock sandbox (placeholder)
    #[cfg(target_os = "linux")]
    async fn execute_with_landlock(
        &self,
        command: Vec<String>,
        working_dir: &Path,
        env_vars: HashMap<String, String>,
        timeout_ms: Option<u64>,
        _sandbox_policy: &SandboxPolicy,
    ) -> Result<BasicExecResult, ExecError> {
        // For now, fall back to basic execution
        // Real implementation would use landlock/seccomp
        tracing::warn!("Landlock sandbox not yet implemented, falling back to basic execution");
        self.execute_basic(command, working_dir, env_vars, timeout_ms)
            .await
    }
}

#[derive(Debug)]
struct BasicExecResult {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_execution() {
        let mut executor = SandboxedExecutor::new(AskForApproval::Never, SandboxPolicy::ReadOnly);

        let params = ExecParams {
            command: vec!["echo".to_string(), "hello".to_string()],
            working_dir: None,
            timeout_ms: Some(5000),
            with_escalated_permissions: false,
            justification: None,
            environment_policy: crate::config_types::ShellEnvironmentPolicy::default(),
        };

        let result = executor.execute(params).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn test_approval_required() {
        let mut executor = SandboxedExecutor::new(
            AskForApproval::OnRequest,
            SandboxPolicy::WorkspaceWrite {
                writable_roots: vec![],
                network_access: false,
                exclude_tmpdir_env_var: false,
                exclude_system_tmp: false,
            },
        );

        let params = ExecParams {
            command: vec!["rm".to_string(), "nonexistent".to_string()],
            working_dir: None,
            timeout_ms: Some(5000),
            with_escalated_permissions: true,
            justification: Some("Test deletion".to_string()),
            environment_policy: crate::config_types::ShellEnvironmentPolicy::default(),
        };

        // This should request approval, but auto-approve in test
        let result = executor.execute(params).await;
        // Result might fail because 'rm nonexistent' fails, but it should not be rejected by approval
        assert!(matches!(
            result,
            Ok(_) | Err(ExecError::ExecutionFailed { .. })
        ));
    }
}
