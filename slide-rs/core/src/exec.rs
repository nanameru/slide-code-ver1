use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::sync::mpsc::Sender;

use crate::approval_manager::{ApprovalManager, ApprovalRequest, ApprovalResponse, AskForApproval};
use crate::config_types::ShellEnvironmentPolicy;
use crate::exec_env::create_env;
use crate::is_safe_command::{explain_safety_concern, is_known_safe_command};
use crate::seatbelt::SandboxPolicy;

const DEFAULT_TIMEOUT_MS: u64 = 10_000;

// Hardcode these since it does not seem worth including the libc crate just
// for these.
const SIGKILL_CODE: i32 = 9;
const TIMEOUT_CODE: i32 = 64;
const EXIT_CODE_SIGNAL_BASE: i32 = 128; // conventional shell: 128 + signal

// I/O buffer sizing
const READ_CHUNK_SIZE: usize = 8192; // bytes per read
const AGGREGATE_BUFFER_INITIAL_CAPACITY: usize = 8 * 1024; // 8 KiB

/// Limit the number of ExecCommandOutputDelta events emitted per exec call.
/// Aggregation still collects full output; only the live event stream is capped.
pub(crate) const MAX_EXEC_OUTPUT_DELTAS_PER_CALL: usize = 10_000;

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

    /// Only available on macOS.
    MacosSeatbelt,

    /// Only available on Linux.
    LinuxSeccomp,
}

#[derive(Clone)]
pub struct StdoutStream {
    pub sub_id: String,
    pub call_id: String,
    pub tx_event: Sender<ExecEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecEvent {
    pub event_type: String,
    pub data: String,
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

/// High-level command execution with sandbox and approval controls
pub async fn process_exec_tool_call(
    params: ExecParams,
    sandbox_type: SandboxType,
    sandbox_policy: &SandboxPolicy,
    _codex_linux_sandbox_exe: &Option<PathBuf>,
    stdout_stream: Option<StdoutStream>,
) -> Result<ExecToolCallOutput> {
    let start = Instant::now();

    let raw_output_result: std::result::Result<RawExecToolCallOutput, anyhow::Error> =
        match sandbox_type {
            SandboxType::None => exec(params, sandbox_policy, stdout_stream.clone()).await,
            SandboxType::MacosSeatbelt => {
                let timeout = params.timeout_duration();
                let ExecParams {
                    command, cwd, env, ..
                } = params;

                // For now, fall back to normal execution
                // TODO: Implement macOS Seatbelt sandbox
                let params = ExecParams {
                    command,
                    cwd,
                    timeout_ms: Some(timeout.as_millis() as u64),
                    env,
                    with_escalated_permissions: None,
                    justification: None,
                };
                exec(params, sandbox_policy, stdout_stream.clone()).await
            }
            SandboxType::LinuxSeccomp => {
                // For now, fall back to normal execution
                // TODO: Implement Linux Seccomp sandbox
                exec(params, sandbox_policy, stdout_stream.clone()).await
            }
        };

    let duration_ms = start.elapsed().as_millis() as u64;

    match raw_output_result {
        Ok(raw_output) => {
            let command_summary = format!(
                "Command: {}",
                raw_output
                    .stdout
                    .lines()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" ")
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

/// Core execution function with safety checks
async fn exec(
    params: ExecParams,
    sandbox_policy: &SandboxPolicy,
    stdout_stream: Option<StdoutStream>,
) -> Result<RawExecToolCallOutput> {
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

    // Prepare environment
    let env = if params.env.is_empty() {
        std::env::vars().collect()
    } else {
        params.env
    };

    // Build command
    let mut cmd = tokio::process::Command::new(&params.command[0]);
    if params.command.len() > 1 {
        cmd.args(&params.command[1..]);
    }

    cmd.current_dir(&params.cwd)
        .envs(&env)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    // Execute with timeout
    let timeout = params.timeout_duration();
    let child = cmd.spawn()?;

    match tokio::time::timeout(timeout, collect_output(child, stdout_stream)).await {
        Ok(result) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            match result {
                Ok((stdout, stderr, exit_status)) => {
                    let exit_code = exit_status_to_code(exit_status);
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
            // Timeout occurred
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

/// Collect output from a running process
async fn collect_output(
    mut child: Child,
    stdout_stream: Option<StdoutStream>,
) -> Result<(String, String, ExitStatus)> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;

    // Collect stdout
    let stdout_task = async {
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::with_capacity(AGGREGATE_BUFFER_INITIAL_CAPACITY);
        let mut temp_buffer = [0u8; READ_CHUNK_SIZE];

        loop {
            match reader.read(&mut temp_buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    buffer.extend_from_slice(&temp_buffer[..n]);

                    // Stream output if requested
                    if let Some(ref stream) = stdout_stream {
                        let chunk = String::from_utf8_lossy(&temp_buffer[..n]);
                        let event = ExecEvent {
                            event_type: "stdout_delta".to_string(),
                            data: chunk.to_string(),
                        };
                        let _ = stream.tx_event.send(event).await;
                    }
                }
                Err(e) => return Err(anyhow::anyhow!("Failed to read stdout: {}", e)),
            }
        }

        String::from_utf8_lossy(&buffer).to_string()
    };

    // Collect stderr
    let stderr_task = async {
        let mut reader = BufReader::new(stderr);
        let mut buffer = Vec::with_capacity(AGGREGATE_BUFFER_INITIAL_CAPACITY);
        let mut temp_buffer = [0u8; READ_CHUNK_SIZE];

        loop {
            match reader.read(&mut temp_buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    buffer.extend_from_slice(&temp_buffer[..n]);
                }
                Err(e) => return Err(anyhow::anyhow!("Failed to read stderr: {}", e)),
            }
        }

        String::from_utf8_lossy(&buffer).to_string()
    };

    // Wait for both output collection and process completion
    let (stdout_result, stderr_result, exit_status) =
        tokio::try_join!(stdout_task, stderr_task, child.wait())?;

    Ok((stdout_result, stderr_result, exit_status))
}

/// Convert ExitStatus to exit code
fn exit_status_to_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(code) = status.code() {
            code
        } else if let Some(signal) = status.signal() {
            EXIT_CODE_SIGNAL_BASE + signal
        } else {
            1 // Default error code
        }
    }
    #[cfg(not(unix))]
    {
        status.code().unwrap_or(1)
    }
}

/// Sandboxed executor for high-level command execution
pub struct SandboxedExecutor {
    approval_manager: ApprovalManager,
    sandbox_policy: SandboxPolicy,
}

impl SandboxedExecutor {
    pub fn new(approval_policy: AskForApproval, sandbox_policy: SandboxPolicy) -> Self {
        Self {
            approval_manager: ApprovalManager::new(approval_policy),
            sandbox_policy,
        }
    }

    /// Execute a command with full sandbox and approval controls
    pub async fn execute(&mut self, params: ExecParams) -> Result<ExecToolCallOutput> {
        // Safety assessment
        if !is_known_safe_command(&params.command) {
            // Request approval for potentially unsafe commands
            let approval_request = ApprovalRequest {
                command: params.command.clone(),
                working_dir: params.cwd.clone(),
                justification: params.justification.clone(),
                risk_level: "medium".to_string(),
            };

            match self
                .approval_manager
                .request_approval(approval_request)
                .await
            {
                ApprovalResponse::Approved => {
                    // Continue with execution
                }
                ApprovalResponse::ApprovedAndTrust => {
                    // Add to trusted commands and continue
                    self.approval_manager
                        .approve_command(params.command.clone());
                }
                ApprovalResponse::Denied => {
                    return Ok(ExecToolCallOutput {
                        stdout: String::new(),
                        stderr: "Command execution denied by user".to_string(),
                        exit_code: 1,
                        duration_ms: 0,
                        timed_out: false,
                        command_summary: "Denied".to_string(),
                    });
                }
                ApprovalResponse::ChangePolicy(new_policy) => {
                    self.approval_manager.set_policy(new_policy);
                    // Re-evaluate with new policy
                    return Box::pin(self.execute(params)).await;
                }
            }
        }

        // Execute with sandbox
        process_exec_tool_call(
            params,
            SandboxType::None, // For now, default to no sandbox
            &self.sandbox_policy,
            &None,
            None,
        )
        .await
    }

    /// Execute a simple command string
    pub async fn execute_simple(
        &mut self,
        command: &str,
        cwd: PathBuf,
    ) -> Result<ExecToolCallOutput> {
        let command_parts = crate::parse_command::parse_command_string(command);
        if command_parts.is_empty() {
            return Err(anyhow::anyhow!("Empty command"));
        }

        let params = ExecParams {
            command: command_parts,
            cwd,
            timeout_ms: Some(DEFAULT_TIMEOUT_MS),
            env: HashMap::new(),
            with_escalated_permissions: None,
            justification: None,
        };

        self.execute(params).await
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

    let result = exec(params, &SandboxPolicy::default(), None).await?;

    Ok(ExecResult {
        status: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
    })
}
