//! MCPメッセージハンドラー - JSON-RPC over stdioでのMCP通信を管理

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::{info, error, warn};

use mcp_types::{
    CallToolRequest, CallToolResult, ListToolsRequest, ListToolsResult,
    RequestId, Tool, ContentBlock, TextContent
};

use slide_core::ConversationManager;
use crate::tool_config::{create_tool_for_mcp_tool_call, create_tool_for_reply_param};
use crate::mcp_tool_runner::{run_tool_session, run_tool_session_reply};

/// MCPメッセージハンドラー
pub struct McpMessageHandler {
    conversation_manager: Arc<ConversationManager>,
    running_requests: Arc<Mutex<HashMap<RequestId, String>>>,
}

impl McpMessageHandler {
    pub fn new(conversation_manager: Arc<ConversationManager>) -> Self {
        Self {
            conversation_manager,
            running_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// MCPリクエストを処理
    pub async fn handle_request(&self, method: &str, params: Value, id: RequestId) -> Result<Value> {
        match method {
            "tools/list" => self.handle_list_tools(params, id).await,
            "tools/call" => self.handle_call_tool(params, id).await,
            _ => {
                warn!("Unknown MCP method: {}", method);
                Err(anyhow::anyhow!("Method not found: {}", method))
            }
        }
    }

    /// ツール一覧の要求を処理
    async fn handle_list_tools(&self, _params: Value, _id: RequestId) -> Result<Value> {
        info!("Handling tools/list request");

        let tools = vec![
            create_tool_for_mcp_tool_call(),
            create_tool_for_reply_param(),
        ];

        let response = ListToolsResult { tools };
        Ok(serde_json::to_value(response)?)
    }

    /// ツール実行の要求を処理
    async fn handle_call_tool(&self, params: Value, id: RequestId) -> Result<Value> {
        let request: CallToolRequest = serde_json::from_value(params)?;
        info!("Handling tools/call request for tool: {}", request.name);

        match request.name.as_str() {
            "slide" => self.handle_slide_tool_call(request, id).await,
            "slide-reply" => self.handle_slide_reply_tool_call(request, id).await,
            _ => {
                error!("Unknown tool: {}", request.name);
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text: format!("Unknown tool: {}", request.name),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                Ok(serde_json::to_value(result)?)
            }
        }
    }

    /// slideツール呼び出しを処理
    async fn handle_slide_tool_call(&self, request: CallToolRequest, id: RequestId) -> Result<Value> {
        use crate::tool_config::McpToolCallParam;

        // パラメータを解析
        let params: McpToolCallParam = if let Some(arguments) = request.arguments {
            serde_json::from_value(arguments)?
        } else {
            return Err(anyhow::anyhow!("Missing arguments for slide tool"));
        };

        // 設定をSlide Configに変換
        let (initial_prompt, config) = params.into_config(None)?;

        info!("Starting new slide session with prompt: {:?}", &initial_prompt[..initial_prompt.len().min(100)]);

        // バックグラウンドでツールセッションを実行
        let conversation_manager = self.conversation_manager.clone();
        let running_requests = self.running_requests.clone();
        tokio::spawn(async move {
            run_tool_session(
                id,
                initial_prompt,
                config,
                conversation_manager,
                running_requests,
            ).await;
        });

        // 即座に開始レスポンスを返す（実際の結果は後でnotificationとして送信）
        let result = CallToolResult {
            content: vec![ContentBlock::TextContent(TextContent {
                r#type: "text".to_string(),
                text: "Slide session started. Results will be streamed as notifications.".to_string(),
                annotations: None,
            })],
            is_error: None,
            structured_content: None,
        };

        Ok(serde_json::to_value(result)?)
    }

    /// slide-replyツール呼び出しを処理
    async fn handle_slide_reply_tool_call(&self, request: CallToolRequest, id: RequestId) -> Result<Value> {
        use crate::tool_config::McpToolCallReplyParam;

        // パラメータを解析
        let params: McpToolCallReplyParam = if let Some(arguments) = request.arguments {
            serde_json::from_value(arguments)?
        } else {
            return Err(anyhow::anyhow!("Missing arguments for slide-reply tool"));
        };

        info!("Continuing slide session: {} with prompt: {:?}", 
              params.conversation_id, &params.prompt[..params.prompt.len().min(100)]);

        // 会話を取得（実際の実装では会話管理機能が必要）
        // ここでは簡易的な実装
        let conversation = match self.conversation_manager.get_conversation(&params.conversation_id).await {
            Some(conv) => conv,
            None => {
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text: format!("Conversation not found: {}", params.conversation_id),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                return Ok(serde_json::to_value(result)?);
            }
        };

        // バックグラウンドで返信処理を実行
        let running_requests = self.running_requests.clone();
        tokio::spawn(async move {
            run_tool_session_reply(
                conversation,
                id,
                params.prompt,
                running_requests,
                params.conversation_id,
            ).await;
        });

        // 即座に開始レスポンスを返す
        let result = CallToolResult {
            content: vec![ContentBlock::TextContent(TextContent {
                r#type: "text".to_string(),
                text: "Slide reply processing started.".to_string(),
                annotations: None,
            })],
            is_error: None,
            structured_content: None,
        };

        Ok(serde_json::to_value(result)?)
    }

    /// 通知を送信（実際のMCP実装では必要）
    pub async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        // 実際のMCP実装では、JSON-RPC notificationとして送信
        // ここでは簡易実装
        info!("Sending notification: {} with params: {:?}", method, params);
        Ok(())
    }

    /// イベントをMCP notificationとして送信
    pub async fn send_event_notification(&self, event: &protocol::Event, request_id: Option<RequestId>) -> Result<()> {
        let params = json!({
            "event": event,
            "requestId": request_id
        });

        self.send_notification("slide/event", params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slide_core::ConversationManager;

    #[tokio::test]
    async fn test_list_tools() {
        let conv_manager = Arc::new(ConversationManager::new());
        let handler = McpMessageHandler::new(conv_manager);
        
        let result = handler.handle_list_tools(json!({}), RequestId::String("test".to_string())).await;
        assert!(result.is_ok());
        
        let response: ListToolsResult = serde_json::from_value(result.unwrap()).unwrap();
        assert_eq!(response.tools.len(), 2);
        assert!(response.tools.iter().any(|t| t.name == "slide"));
        assert!(response.tools.iter().any(|t| t.name == "slide-reply"));
    }
}
