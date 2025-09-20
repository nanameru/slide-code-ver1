// Simplified exec module for basic functionality
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;
use std::time::Instant;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::is_safe_command::{explain_safety_concern, is_known_safe_command};
use crate::seatbelt::SandboxPolicy;

const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const TIMEOUT_CODE: i32 = 64;
const EXIT_CODE_SIGNAL_BASE: i32 = 128;

#[derive(Debug, Clone)]
pub struct ExecParams {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub timeout_ms: Option<u64>,
    pub env: HashMap<String, String>,
    pub with_escalated_permissions: Option<bool>,
    pub justification: Option<String>,
}

impl ExecParams {
    pub fn timeout_duration(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SandboxType {
    None,
    MacosSeatbelt,
    LinuxSeccomp,
}

#[derive(Debug, Clone)]
pub struct ExecToolCallOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub command_summary: String,
}

#[derive(Debug, Clone)]
pub struct RawExecToolCallOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub timed_out: bool,
}

/// Basic command execution without streaming
pub async fn process_exec_tool_call(
    params: ExecParams,
    _sandbox_type: SandboxType,
    _sandbox_policy: &SandboxPolicy,
    _codex_linux_sandbox_exe: &Option<PathBuf>,
    _stdout_stream: Option<()>, // Simplified for now
) -> Result<ExecToolCallOutput> {
    let start = Instant::now();

    let raw_output_result = exec_basic(params).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match raw_output_result {
        Ok(raw_output) => {
            let command_summary = format!(
                "Command executed: exit code {}",
                raw_output.exit_code
            );

            Ok(ExecToolCallOutput {
                stdout: raw_output.stdout,
                stderr: raw_output.stderr,
                exit_code: raw_output.exit_code,
                duration_ms,
                timed_out: raw_output.timed_out,
                command_summary,
            })
        }
        Err(e) => Ok(ExecToolCallOutput {
            stdout: String::new(),
            stderr: format!("Execution failed: {}", e),
            exit_code: 1,
            duration_ms,
            timed_out: false,
            command_summary: "Failed to execute".to_string(),
        }),
    }
}

/// Basic execution function
async fn exec_basic(params: ExecParams) -> Result<RawExecToolCallOutput> {
    let start = Instant::now();

    // Safety check
    if !is_known_safe_command(&params.command) {
        if let Some(concern) = explain_safety_concern(&params.command) {
            return Err(anyhow::anyhow!(
                "Command rejected by safety policy: {}",
                concern
            ));
        }
    }

    let timeout = params.timeout_duration();

    // Prepare environment
    let env = if params.env.is_empty() {
        std::env::vars().collect()
    } else {
        params.env.clone()
    };

    // Build command
    let mut cmd = Command::new(&params.command[0]);
    if params.command.len() > 1 {
        cmd.args(&params.command[1..]);
    }

    cmd.current_dir(&params.cwd)
        .envs(&env)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    // Execute with timeout
    let output_future = cmd.output();

    match tokio::time::timeout(timeout, output_future).await {
        Ok(result) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            match result {
                Ok(output) => {
                    let exit_code = output.status.code().unwrap_or(1);
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                    Ok(RawExecToolCallOutput {
                        stdout,
                        stderr,
                        exit_code,
                        duration_ms,
                        timed_out: false,
                    })
                }
                Err(e) => Err(anyhow::anyhow!("Command execution failed: {}", e)),
            }
        }
        Err(_) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            Ok(RawExecToolCallOutput {
                stdout: String::new(),
                stderr: format!("Command timed out after {}ms", timeout.as_millis()),
                exit_code: TIMEOUT_CODE,
                duration_ms,
                timed_out: true,
            })
        }
    }
}

// Legacy compatibility
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub async fn exec_command(cmd: &str, _sandbox: bool, _network: bool) -> Result<ExecResult> {
    let command_parts = crate::parse_command::parse_command_string(cmd);
    if command_parts.is_empty() {
        return Err(anyhow::anyhow!("Empty command"));
    }

    let params = ExecParams {
        command: command_parts,
        cwd: std::env::current_dir()?,
        timeout_ms: Some(DEFAULT_TIMEOUT_MS),
        env: HashMap::new(),
        with_escalated_permissions: None,
        justification: None,
    };

    let result = exec_basic(params).await?;

    Ok(ExecResult {
        status: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
    })
}