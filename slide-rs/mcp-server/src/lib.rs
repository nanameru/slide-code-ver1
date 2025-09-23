pub mod mcp_tool_runner;
pub mod tool_config;
pub mod approval_processor;
pub mod message_handler;

use anyhow::Result;
use std::sync::Arc;
use slide_core::ConversationManager;

pub use mcp_tool_runner::run_tool_session;
pub use tool_config::{McpToolCallParam, create_tool_for_mcp_tool_call};
pub use approval_processor::{handle_patch_approval, handle_exec_approval};

/// MCPサーバーの初期化と実行
pub async fn start_mcp_server(
    conversation_manager: Arc<ConversationManager>,
) -> Result<()> {
    tracing::info!("Starting MCP server...");
    
    // MCPサーバーの実装はここに追加
    // 現在は基本的な構造のみ提供
    
    Ok(())
}
