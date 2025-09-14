use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    pub args: Vec<String>,
    pub working_directory: Option<String>,
    pub environment: HashMap<String, String>,
    pub timeout_seconds: Option<u64>,
    pub capture_output: bool,
    pub stream_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub execution_time_ms: u128,
    pub success: bool,
    pub timed_out: bool,
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingExecResult {
    pub process_id: u32,
    pub output_channel: String, // stdout or stderr
    pub line: String,
    pub line_number: usize,
}

pub struct ExecutionManager {
    pub working_directory: PathBuf,
    pub default_timeout: Duration,
    pub sandbox_mode: bool,
    pub allowed_commands: Vec<String>,
}

impl Default for ExecutionManager {
    fn default() -> Self {
        Self {
            working_directory: std::env::current_dir().unwrap_or_default(),
            default_timeout: Duration::from_secs(300), // 5 minutes
            sandbox_mode: true,
            allowed_commands: vec![
                "cargo".to_string(),
                "rustc".to_string(),
                "npm".to_string(),
                "node".to_string(),
                "python".to_string(),
                "python3".to_string(),
                "pip".to_string(),
                "git".to_string(),
                "ls".to_string(),
                "cat".to_string(),
                "echo".to_string(),
                "mkdir".to_string(),
                "cp".to_string(),
                "mv".to_string(),
                "grep".to_string(),
                "find".to_string(),
                "test".to_string(),
                "make".to_string(),
            ],
        }
    }
}

impl ExecutionManager {
    pub fn new(working_directory: PathBuf) -> Self {
        Self {
            working_directory,
            ..Default::default()
        }
    }

    pub fn with_sandbox_mode(mut self, sandbox: bool) -> Self {
        self.sandbox_mode = sandbox;
        self
    }

    pub fn with_allowed_commands(mut self, commands: Vec<String>) -> Self {
        self.allowed_commands = commands;
        self
    }

    pub fn with_default_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    pub async fn execute(&self, request: ExecRequest) -> Result<ExecResult> {
        let start_time = std::time::Instant::now();

        // Security checks in sandbox mode
        if self.sandbox_mode && !self.is_command_allowed(&request.command) {
            return Ok(ExecResult {
                exit_code: Some(-1),
                stdout: String::new(),
                stderr: format!("Command '{}' is not allowed in sandbox mode", request.command),
                execution_time_ms: 0,
                success: false,
                timed_out: false,
                process_id: None,
            });
        }

        let timeout_duration = request.timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or(self.default_timeout);

        let exec_result = timeout(timeout_duration, self.execute_internal(request)).await;

        let execution_time = start_time.elapsed();

        match exec_result {
            Ok(result) => {
                let mut final_result = result?;
                final_result.execution_time_ms = execution_time.as_millis();
                Ok(final_result)
            },
            Err(_) => Ok(ExecResult {
                exit_code: None,
                stdout: String::new(),
                stderr: "Execution timed out".to_string(),
                execution_time_ms: execution_time.as_millis(),
                success: false,
                timed_out: true,
                process_id: None,
            }),
        }
    }

    pub async fn execute_streaming(&self, request: ExecRequest) -> Result<mpsc::Receiver<StreamingExecResult>> {
        if self.sandbox_mode && !self.is_command_allowed(&request.command) {
            return Err(anyhow!("Command '{}' is not allowed in sandbox mode", request.command));
        }

        let (tx, rx) = mpsc::channel(100);
        let working_dir = request.working_directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.working_directory.clone());

        let mut cmd = Command::new(&request.command);
        cmd.args(&request.args);
        cmd.current_dir(working_dir);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Set environment variables
        for (key, value) in &request.environment {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn()?;
        let process_id = child.id().unwrap_or(0);

        // Spawn tasks to handle stdout and stderr streaming
        if let Some(stdout) = child.stdout.take() {
            let tx_stdout = tx.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                let mut line_number = 1;

                while let Ok(Some(line)) = lines.next_line().await {
                    let result = StreamingExecResult {
                        process_id,
                        output_channel: "stdout".to_string(),
                        line,
                        line_number,
                    };

                    if tx_stdout.send(result).await.is_err() {
                        break; // Receiver dropped
                    }
                    line_number += 1;
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let tx_stderr = tx.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                let mut line_number = 1;

                while let Ok(Some(line)) = lines.next_line().await {
                    let result = StreamingExecResult {
                        process_id,
                        output_channel: "stderr".to_string(),
                        line,
                        line_number,
                    };

                    if tx_stderr.send(result).await.is_err() {
                        break; // Receiver dropped
                    }
                    line_number += 1;
                }
            });
        }

        // Spawn task to wait for process completion
        tokio::spawn(async move {
            let _ = child.wait().await;
            // Channel will be closed when all senders are dropped
        });

        Ok(rx)
    }

    async fn execute_internal(&self, request: ExecRequest) -> Result<ExecResult> {
        let working_dir = request.working_directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.working_directory.clone());

        let mut cmd = Command::new(&request.command);
        cmd.args(&request.args);
        cmd.current_dir(working_dir);

        // Configure stdio based on capture_output setting
        if request.capture_output {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit());
            cmd.stderr(Stdio::inherit());
        }

        // Set environment variables
        for (key, value) in &request.environment {
            cmd.env(key, value);
        }

        let child = cmd.spawn()?;
        let process_id = child.id();

        let output = child.wait_with_output().await?;

        let stdout = if request.capture_output {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            String::new()
        };

        let stderr = if request.capture_output {
            String::from_utf8_lossy(&output.stderr).to_string()
        } else {
            String::new()
        };

        Ok(ExecResult {
            exit_code: output.status.code(),
            stdout,
            stderr,
            execution_time_ms: 0, // Will be set by caller
            success: output.status.success(),
            timed_out: false,
            process_id,
        })
    }

    fn is_command_allowed(&self, command: &str) -> bool {
        if !self.sandbox_mode {
            return true;
        }

        // Extract just the command name (remove path)
        let command_name = std::path::Path::new(command)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(command);

        self.allowed_commands.iter().any(|allowed| {
            allowed == command_name || command_name.starts_with(allowed)
        })
    }

    pub fn validate_execution_request(&self, request: &ExecRequest) -> Result<()> {
        if request.command.is_empty() {
            return Err(anyhow!("Command cannot be empty"));
        }

        // Check for dangerous command patterns
        let dangerous_patterns = [
            "rm -rf", "del /s", "format", "mkfs", "dd if=",
            "shutdown", "reboot", "halt", "poweroff",
            "; rm ", "&& rm ", "| rm ", "$(rm", "`rm",
            "curl", "wget", "nc ", "netcat",
            "ssh ", "scp ", "sftp ",
        ];

        let full_command = format!("{} {}", request.command, request.args.join(" "));

        for pattern in &dangerous_patterns {
            if full_command.contains(pattern) {
                return Err(anyhow!(
                    "Command contains dangerous pattern '{}': {}",
                    pattern,
                    full_command
                ));
            }
        }

        // Validate working directory is safe
        if let Some(ref wd) = request.working_directory {
            let wd_path = PathBuf::from(wd);
            if wd_path.is_absolute() {
                let canonical_wd = wd_path.canonicalize().unwrap_or(wd_path);
                let canonical_workspace = self.working_directory.canonicalize()
                    .unwrap_or_else(|_| self.working_directory.clone());

                if self.sandbox_mode && !canonical_wd.starts_with(&canonical_workspace) {
                    return Err(anyhow!(
                        "Working directory '{}' is outside the workspace",
                        wd
                    ));
                }
            }
        }

        Ok(())
    }

    pub async fn get_system_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();

        // Operating system
        info.insert("os".to_string(), std::env::consts::OS.to_string());
        info.insert("arch".to_string(), std::env::consts::ARCH.to_string());

        // Current working directory
        if let Ok(cwd) = std::env::current_dir() {
            info.insert("cwd".to_string(), cwd.to_string_lossy().to_string());
        }

        // Environment variables (safe ones only)
        let safe_env_vars = ["PATH", "HOME", "USER", "SHELL"];
        for var in &safe_env_vars {
            if let Ok(value) = std::env::var(var) {
                info.insert(format!("env_{}", var.to_lowercase()), value);
            }
        }

        // Try to get some system commands availability
        let test_commands = ["cargo", "node", "python", "git"];
        for cmd in &test_commands {
            let available = Command::new("which")
                .arg(cmd)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await
                .map(|status| status.success())
                .unwrap_or(false);

            info.insert(format!("cmd_{}", cmd), available.to_string());
        }

        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_simple_command_execution() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExecutionManager::new(temp_dir.path().to_path_buf());

        let request = ExecRequest {
            command: "echo".to_string(),
            args: vec!["hello world".to_string()],
            working_directory: None,
            environment: HashMap::new(),
            timeout_seconds: Some(10),
            capture_output: true,
            stream_output: false,
        };

        let result = manager.execute(request).await.unwrap();

        assert!(result.success);
        assert!(result.stdout.contains("hello world"));
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn test_sandbox_command_blocking() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExecutionManager::new(temp_dir.path().to_path_buf())
            .with_sandbox_mode(true);

        let request = ExecRequest {
            command: "dangerous_command".to_string(),
            args: vec![],
            working_directory: None,
            environment: HashMap::new(),
            timeout_seconds: Some(10),
            capture_output: true,
            stream_output: false,
        };

        let result = manager.execute(request).await.unwrap();

        assert!(!result.success);
        assert!(result.stderr.contains("not allowed in sandbox mode"));
    }

    #[test]
    fn test_validation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExecutionManager::new(temp_dir.path().to_path_buf());

        // Valid request
        let valid_request = ExecRequest {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            working_directory: None,
            environment: HashMap::new(),
            timeout_seconds: Some(10),
            capture_output: true,
            stream_output: false,
        };

        assert!(manager.validate_execution_request(&valid_request).is_ok());

        // Invalid request - empty command
        let invalid_request = ExecRequest {
            command: String::new(),
            args: vec![],
            working_directory: None,
            environment: HashMap::new(),
            timeout_seconds: Some(10),
            capture_output: true,
            stream_output: false,
        };

        assert!(manager.validate_execution_request(&invalid_request).is_err());
    }
}