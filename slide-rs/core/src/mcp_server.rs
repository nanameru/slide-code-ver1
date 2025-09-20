use crate::approval_manager::{ApprovalManager, AskForApproval};
use crate::exec::{process_exec_tool_call, ExecParams, SandboxType, StdoutStream, Event, EventMsg};
use crate::protocol::{EventDispatcher, SessionManager};
use crate::seatbelt::SandboxPolicy;
use crate::tool_executor::ToolExecutor;
use anyhow::{Context, Result};
use async_channel::{Receiver, Sender};
use mcp_types::{
    CallToolRequest, CallToolResult, ContentBlock, RequestId, TextContent, ToolInfo,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

/// MCP Server for handling tool execution requests
pub struct MCPToolServer {
    tool_executor: Arc<Mutex<ToolExecutor>>,
    session_manager: Arc<Mutex<SessionManager>>,
    event_dispatcher: EventDispatcher,
    running_requests: Arc<RwLock<HashMap<RequestId, Uuid>>>,
}

impl MCPToolServer {
    pub fn new(
        approval_policy: AskForApproval,
        sandbox_policy: SandboxPolicy,
        cwd: PathBuf,
    ) -> Self {
        let (event_dispatcher, _event_receiver) = EventDispatcher::new();
        let session_manager = SessionManager::new().with_dispatcher(event_dispatcher.clone());

        let tool_executor = ToolExecutor::new(
            approval_policy,
            sandbox_policy.clone(),
            cwd,
            Default::default(),
        );

        Self {
            tool_executor: Arc::new(Mutex::new(tool_executor)),
            session_manager: Arc::new(Mutex::new(session_manager)),
            event_dispatcher,
            running_requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// List available tools
    pub async fn list_tools(&self) -> Vec<ToolInfo> {
        vec![
            ToolInfo {
                name: "shell".to_string(),
                description: Some("Execute shell commands with optional sandbox restrictions".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Command and arguments to execute"
                        },
                        "working_dir": {
                            "type": "string",
                            "description": "Working directory for command execution"
                        },
                        "timeout_ms": {
                            "type": "number",
                            "description": "Timeout in milliseconds"
                        },
                        "with_escalated_permissions": {
                            "type": "boolean",
                            "description": "Request escalated permissions"
                        },
                        "justification": {
                            "type": "string",
                            "description": "Justification for command execution"
                        }
                    },
                    "required": ["command"]
                }),
            },
            ToolInfo {
                name: "read_file".to_string(),
                description: Some("Read the contents of a file".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read"
                        }
                    },
                    "required": ["path"]
                }),
            },
            ToolInfo {
                name: "write_file".to_string(),
                description: Some("Write content to a file".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolInfo {
                name: "apply_patch".to_string(),
                description: Some("Apply a unified diff patch to files".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "Unified diff patch content"
                        }
                    },
                    "required": ["input"]
                }),
            },
            ToolInfo {
                name: "list_files".to_string(),
                description: Some("List files in a directory".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path to list (optional, defaults to current directory)"
                        }
                    }
                }),
            },
            ToolInfo {
                name: "search_files".to_string(),
                description: Some("Search for files by name pattern".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query pattern"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory path to search in (optional)"
                        }
                    },
                    "required": ["query"]
                }),
            },
        ]
    }

    /// Handle a tool call request
    pub async fn handle_tool_call(
        &self,
        request_id: RequestId,
        tool_name: &str,
        arguments: &Value,
    ) -> Result<CallToolResult> {
        let call_id = Uuid::new_v4().to_string();
        let sub_id = format!("mcp-{}", Uuid::new_v4());

        // Track the request
        {
            let mut running_requests = self.running_requests.write().await;
            running_requests.insert(request_id.clone(), Uuid::new_v4());
        }

        // Create stdout stream for real-time output
        let stdout_stream = Some(StdoutStream {
            sub_id: sub_id.clone(),
            call_id: call_id.clone(),
            tx_event: self.event_dispatcher.get_event_sender(),
        });

        match tool_name {
            "shell" => self.handle_shell_tool(call_id, arguments, stdout_stream).await,
            "read_file" => self.handle_read_file_tool(arguments).await,
            "write_file" => self.handle_write_file_tool(arguments).await,
            "apply_patch" => self.handle_apply_patch_tool(arguments).await,
            "list_files" => self.handle_list_files_tool(arguments).await,
            "search_files" => self.handle_search_files_tool(arguments).await,
            _ => Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: format!("Unknown tool: {}", tool_name),
                })],
                is_error: Some(true),
            }),
        }
    }

    async fn handle_shell_tool(
        &self,
        call_id: String,
        arguments: &Value,
        stdout_stream: Option<StdoutStream>,
    ) -> Result<CallToolResult> {
        let command = arguments["command"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid command array"))?
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();

        if command.is_empty() {
            return Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: "Empty command provided".to_string(),
                })],
                is_error: Some(true),
            });
        }

        let working_dir = arguments["working_dir"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let timeout_ms = arguments["timeout_ms"].as_u64();
        let with_escalated_permissions = arguments["with_escalated_permissions"]
            .as_bool()
            .unwrap_or(false);
        let justification = arguments["justification"].as_str().map(String::from);

        let params = ExecParams {
            command: command.clone(),
            cwd: working_dir,
            timeout_ms,
            env: HashMap::new(),
            with_escalated_permissions: Some(with_escalated_permissions),
            justification,
        };

        // Start session tracking
        {
            let mut session_manager = self.session_manager.lock().await;
            session_manager.start_session(call_id.clone(), call_id.clone(), command);
        }

        // Execute the command
        match process_exec_tool_call(
            params,
            SandboxType::None, // Could be configurable
            &SandboxPolicy::default(),
            &None,
            stdout_stream,
        )
        .await
        {
            Ok(result) => {
                // Complete session
                {
                    let mut session_manager = self.session_manager.lock().await;
                    session_manager
                        .complete_session(
                            &call_id,
                            result.exit_code,
                            result.stdout.clone(),
                            result.stderr.clone(),
                        )
                        .await;
                }

                let mut output = format!(
                    "Command executed with exit code: {}\nDuration: {}ms",
                    result.exit_code, result.duration_ms
                );

                if !result.stdout.is_empty() {
                    output.push_str(&format!("\n\nSTDOUT:\n{}", result.stdout));
                }

                if !result.stderr.is_empty() {
                    output.push_str(&format!("\n\nSTDERR:\n{}", result.stderr));
                }

                if result.timed_out {
                    output.push_str("\n\n⚠️  Command timed out");
                }

                Ok(CallToolResult {
                    content: vec![ContentBlock::Text(TextContent { text: output })],
                    is_error: Some(result.exit_code != 0),
                })
            }
            Err(e) => {
                // Fail session
                {
                    let mut session_manager = self.session_manager.lock().await;
                    session_manager
                        .fail_session(&call_id, e.to_string())
                        .await;
                }

                Ok(CallToolResult {
                    content: vec![ContentBlock::Text(TextContent {
                        text: format!("Command execution failed: {}", e),
                    })],
                    is_error: Some(true),
                })
            }
        }
    }

    async fn handle_read_file_tool(&self, arguments: &Value) -> Result<CallToolResult> {
        let path = arguments["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path argument"))?;

        let path_buf = PathBuf::from(path);
        match tokio::fs::read_to_string(&path_buf).await {
            Ok(content) => Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: format!("File: {}\n\n{}", path, content),
                })],
                is_error: Some(false),
            }),
            Err(e) => Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: format!("Failed to read file {}: {}", path, e),
                })],
                is_error: Some(true),
            }),
        }
    }

    async fn handle_write_file_tool(&self, arguments: &Value) -> Result<CallToolResult> {
        let path = arguments["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path argument"))?;
        let content = arguments["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing content argument"))?;

        let path_buf = PathBuf::from(path);
        if let Some(parent) = path_buf.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(CallToolResult {
                    content: vec![ContentBlock::Text(TextContent {
                        text: format!("Failed to create directory {}: {}", parent.display(), e),
                    })],
                    is_error: Some(true),
                });
            }
        }

        match tokio::fs::write(&path_buf, content).await {
            Ok(_) => Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: format!("Successfully wrote {} bytes to {}", content.len(), path),
                })],
                is_error: Some(false),
            }),
            Err(e) => Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: format!("Failed to write file {}: {}", path, e),
                })],
                is_error: Some(true),
            }),
        }
    }

    async fn handle_apply_patch_tool(&self, arguments: &Value) -> Result<CallToolResult> {
        let patch_input = arguments["input"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing patch input"))?;

        use crate::tool_apply_patch::{tool_apply_patch, ApplyPatchInput};
        let result = tool_apply_patch(
            ApplyPatchInput {
                patch: patch_input.to_string(),
            },
            true,
        );

        Ok(CallToolResult {
            content: vec![ContentBlock::Text(TextContent {
                text: if result.applied {
                    format!("✅ Patch applied successfully: {}", result.message)
                } else {
                    format!("❌ Patch failed: {}", result.message)
                },
            })],
            is_error: Some(!result.applied),
        })
    }

    async fn handle_list_files_tool(&self, arguments: &Value) -> Result<CallToolResult> {
        let path = arguments["path"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        match tokio::fs::read_dir(&path).await {
            Ok(mut entries) => {
                let mut files = Vec::new();
                while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                    if let Ok(name) = entry.file_name().into_string() {
                        let file_type = if entry.path().is_dir() { "📁" } else { "📄" };
                        files.push(format!("{} {}", file_type, name));
                    }
                }
                files.sort();

                Ok(CallToolResult {
                    content: vec![ContentBlock::Text(TextContent {
                        text: format!(
                            "Files in {}:\n\n{}",
                            path.display(),
                            files.join("\n")
                        ),
                    })],
                    is_error: Some(false),
                })
            }
            Err(e) => Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: format!("Failed to list files in {}: {}", path.display(), e),
                })],
                is_error: Some(true),
            }),
        }
    }

    async fn handle_search_files_tool(&self, arguments: &Value) -> Result<CallToolResult> {
        let query = arguments["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query argument"))?;
        let search_path = arguments["path"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let results = self.search_files_recursive(&search_path, query).await?;

        if results.is_empty() {
            Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: format!("No files found matching '{}' in {}", query, search_path.display()),
                })],
                is_error: Some(false),
            })
        } else {
            Ok(CallToolResult {
                content: vec![ContentBlock::Text(TextContent {
                    text: format!(
                        "Found {} files matching '{}':\n\n{}",
                        results.len(),
                        query,
                        results.join("\n")
                    ),
                })],
                is_error: Some(false),
            })
        }
    }

    async fn search_files_recursive(&self, dir: &PathBuf, query: &str) -> Result<Vec<String>> {
        let mut results = Vec::new();
        let mut stack = vec![dir.clone()];

        while let Some(current_dir) = stack.pop() {
            if let Ok(mut entries) = tokio::fs::read_dir(&current_dir).await {
                while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                    let path = entry.path();
                    let file_name = entry.file_name();

                    if let Some(name_str) = file_name.to_str() {
                        if name_str.contains(query) {
                            results.push(path.to_string_lossy().to_string());
                        }
                    }

                    if path.is_dir() {
                        stack.push(path);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Get event dispatcher for external subscription
    pub fn get_event_dispatcher(&self) -> EventDispatcher {
        self.event_dispatcher.clone()
    }

    /// Get session manager for monitoring
    pub async fn get_session_manager(&self) -> tokio::sync::MutexGuard<SessionManager> {
        self.session_manager.lock().await
    }
}

/// MCP Tool Runner for standalone tool execution
pub struct MCPToolRunner {
    server: MCPToolServer,
}

impl MCPToolRunner {
    pub fn new(cwd: PathBuf) -> Self {
        let server = MCPToolServer::new(
            AskForApproval::Never, // Default to auto-approve for MCP
            SandboxPolicy::default(),
            cwd,
        );

        Self { server }
    }

    pub async fn run_tool_session(
        &self,
        initial_prompt: String,
    ) -> Result<String> {
        // Parse tool calls from the prompt
        let tool_calls = self.parse_tool_calls_from_prompt(&initial_prompt)?;

        if tool_calls.is_empty() {
            return Ok("No tool calls found in prompt".to_string());
        }

        let mut results = Vec::new();
        for (tool_name, arguments) in tool_calls {
            let request_id = RequestId::from(Uuid::new_v4().to_string());
            let result = self.server.handle_tool_call(request_id, &tool_name, &arguments).await?;

            let result_text = result
                .content
                .into_iter()
                .map(|content| match content {
                    ContentBlock::Text(text) => text.text,
                    ContentBlock::Image(_) => "[Image content]".to_string(),
                })
                .collect::<Vec<_>>()
                .join("\n");

            results.push(format!("Tool: {}\nResult: {}", tool_name, result_text));
        }

        Ok(results.join("\n\n"))
    }

    fn parse_tool_calls_from_prompt(&self, prompt: &str) -> Result<Vec<(String, Value)>> {
        // Simple JSON tool call parser
        let mut tool_calls = Vec::new();

        for line in prompt.lines() {
            let line = line.trim();
            if line.starts_with('{') && line.contains("\"tool\"") {
                if let Ok(value) = serde_json::from_str::<Value>(line) {
                    if let Some(tool_name) = value["tool"].as_str() {
                        tool_calls.push((tool_name.to_string(), value));
                    }
                }
            }
        }

        Ok(tool_calls)
    }
}