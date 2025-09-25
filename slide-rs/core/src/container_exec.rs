//! Container execution management for shell commands.
//!
//! This module provides functionality to execute shell commands with proper
//! parameter parsing and result formatting, mirroring codex-1's implementation.

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::Result;

use crate::conversation_history::{FunctionCallOutputPayload, ResponseInputItem};
use crate::approval_manager::AskForApproval;
use crate::seatbelt::SandboxPolicy;
use crate::config_types::ShellEnvironmentPolicy as ConfigShellEnvironmentPolicy;

// Forward declarations for types that will be defined in codex.rs
pub struct TurnContext {
    pub cwd: PathBuf,
    pub approval_policy: AskForApproval,
    pub shell_environment_policy: ConfigShellEnvironmentPolicy,
}

pub struct Session;

pub struct TurnDiffTracker;

#[derive(Debug, Clone)]
pub struct ShellEnvironmentPolicy {
    pub use_profile: bool,
}

/// Parameters for executing shell commands, based on codex-1's ShellToolCallParams
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecCommandParams {
    /// The shell command to execute
    pub cmd: String,
    
    /// Maximum time in milliseconds to wait for output
    #[serde(default = "default_yield_time")]
    pub yield_time_ms: u64,
    
    /// Maximum number of tokens to output
    #[serde(default = "max_output_tokens")]
    pub max_output_tokens: u64,
    
    /// The shell to use (defaults to /bin/bash)
    #[serde(default = "default_shell")]
    pub shell: String,
    
    /// Whether to run as login shell
    #[serde(default = "default_login")]
    pub login: bool,
    
    /// Working directory for command execution
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    
    /// Timeout in milliseconds
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    
    /// Whether to request escalated permissions
    #[serde(default)]
    pub with_escalated_permissions: Option<bool>,
    
    /// Justification for the command
    #[serde(default)]
    pub justification: Option<String>,
}

fn default_yield_time() -> u64 {
    10_000
}

fn max_output_tokens() -> u64 {
    10_000
}

fn default_login() -> bool {
    true
}

fn default_shell() -> String {
    "/bin/bash".to_string()
}

/// Execution parameters used internally
#[derive(Debug, Clone)]
pub struct ExecParams {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub timeout_ms: Option<u64>,
    pub env: HashMap<String, String>,
    pub with_escalated_permissions: Option<bool>,
    pub justification: Option<String>,
}

/// Parse container.exec arguments from JSON string
pub fn parse_container_exec_arguments(
    arguments: String,
    turn_context: &TurnContext,
    call_id: &str,
) -> Result<ExecParams, Box<ResponseInputItem>> {
    // Parse command arguments
    match serde_json::from_str::<ExecCommandParams>(&arguments) {
        Ok(shell_tool_call_params) => Ok(to_exec_params(shell_tool_call_params, turn_context)),
        Err(e) => {
            // Allow model to re-sample
            let output = ResponseInputItem::FunctionCallOutput {
                call_id: call_id.to_string(),
                output: FunctionCallOutputPayload {
                    content: format!("failed to parse function arguments: {e}"),
                    success: Some(false),
                },
            };
            Err(Box::new(output))
        }
    }
}

/// Convert ExecCommandParams to ExecParams
pub fn to_exec_params(
    params: ExecCommandParams,
    turn_context: &TurnContext,
) -> ExecParams {
    // Parse command using shlex
    let command = match shlex::split(&params.cmd) {
        Some(cmd) => cmd,
        None => vec![params.cmd.clone()],
    };
    
    let cwd = params.cwd.unwrap_or_else(|| turn_context.cwd.clone());
    
    ExecParams {
        command,
        cwd,
        timeout_ms: params.timeout_ms,
        env: params.env,
        with_escalated_permissions: params.with_escalated_permissions,
        justification: params.justification,
    }
}

/// Handle container.exec with parameters
pub async fn handle_container_exec_with_params(
    params: ExecParams,
    sess: &Session,
    turn_context: &TurnContext,
    _turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    call_id: String,
) -> ResponseInputItem {
    // Check escalated permissions policy
    if params.with_escalated_permissions.unwrap_or(false)
        && !matches!(turn_context.approval_policy, AskForApproval::OnRequest)
    {
        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: format!(
                    "approval policy is {policy:?}; reject command — you should not ask for escalated permissions if the approval policy is {policy:?}",
                    policy = turn_context.approval_policy
                ),
                success: Some(false),
            },
        };
    }

    // For now, use simplified execution via ToolExecutor
    // TODO: Implement full container execution with PTY support
    let start = std::time::Instant::now();
    
    // Create tool executor
    let mut tool_executor = crate::tool_executor::ToolExecutor::new(
        AskForApproval::Never,
        SandboxPolicy::DangerFullAccess,
        params.cwd.clone(),
        turn_context.shell_environment_policy.clone(),
    );
    
    // Execute the command
    let command_str = params.command.join(" ");
    let result = tool_executor.execute_function_call("shell", &command_str).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    
    // Send tool execution events
    let begin_event = protocol::protocol::Event {
        id: sub_id.clone(),
        msg: protocol::protocol::EventMsg::ToolExecutionBegin(
            protocol::protocol::ToolExecutionBeginEvent {
                call_id: call_id.clone(),
                tool_name: "container.exec".to_string(),
                arguments: command_str.clone(),
            }
        ),
    };
    // sess.send_event(begin_event).await; // Commented out for now
    
    let success = result.is_ok();
    let result_text = match &result {
        Ok(output) => output.clone(),
        Err(e) => e.to_string(),
    };
    let end_event = protocol::protocol::Event {
        id: sub_id,
        msg: protocol::protocol::EventMsg::ToolExecutionEnd(
            protocol::protocol::ToolExecutionEndEvent {
                call_id: call_id.clone(),
                tool_name: "container.exec".to_string(),
                success,
                duration_ms,
                result: result_text,
            }
        ),
    };
    // sess.send_event(end_event).await; // Commented out for now
    
    // Return result
    match result {
        Ok(output) => ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: output,
                success: Some(true),
            },
        },
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: format!("Command execution failed: {}", e),
                success: Some(false),
            },
        },
    }
}
