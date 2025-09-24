use std::time::Duration;
use std::time::Instant;

use tracing::error;

use protocol::protocol::McpInvocation;
use crate::conversation_history::FunctionCallOutputPayload;
use crate::conversation_history::ResponseInputItem;
use crate::mcp_connection_manager::McpConnectionManager;

/// Handles the specified tool call dispatches the appropriate
/// `McpToolCallBegin` and `McpToolCallEnd` events to the `Session`.
pub(crate) async fn handle_mcp_tool_call(
    mcp_manager: &McpConnectionManager,
    sub_id: &str,
    call_id: String,
    server: String,
    tool_name: String,
    arguments: String,
    timeout: Option<Duration>,
) -> ResponseInputItem {
    // Parse the `arguments` as JSON. An empty string is OK, but invalid JSON
    // is not.
    let arguments_value = if arguments.trim().is_empty() {
        None
    } else {
        match serde_json::from_str::<serde_json::Value>(&arguments) {
            Ok(value) => Some(value),
            Err(e) => {
                error!("failed to parse tool call arguments: {e}");
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: FunctionCallOutputPayload {
                        content: format!("err: {e}"),
                        success: Some(false),
                    },
                };
            }
        }
    };

    let invocation = McpInvocation {
        server: server.clone(),
        tool: tool_name.clone(),
        arguments: arguments_value.clone(),
    };

    // MCPツール実行開始をログ出力
    tracing::info!("Starting MCP tool call: server={}, tool={}, call_id={}", server, tool_name, call_id);

    let start = Instant::now();
    // Perform the tool call.
    let result = mcp_manager
        .call_tool(&server, &tool_name, arguments_value.clone(), timeout)
        .await
        .map_err(|e| format!("tool call error: {e}"));
    // MCPツール実行終了をログ出力
    let duration = start.elapsed();
    tracing::info!("Completed MCP tool call: call_id={}, duration={:?}, success={}", 
                   call_id, duration, result.is_ok());

    ResponseInputItem::McpToolCallOutput { call_id, result }
}

