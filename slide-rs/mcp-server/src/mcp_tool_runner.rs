//! MCPツール実行エンジン - slide-code-test実装
//! codex-1のcodex_tool_runner.rsに相当する機能を提供

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use mcp_types::{CallToolResult, ContentBlock, RequestId, TextContent};
use serde_json::json;
use tokio::sync::Mutex;
use tracing::{error, info};

use slide_core::{
    Config as SlideConfig, ConversationManager, NewConversation, 
    SlideConversation, protocol::{Event, EventMsg}
};
use protocol::models::{FunctionCallOutputPayload, ResponseInputItem};

pub(crate) const INVALID_PARAMS_ERROR_CODE: i64 = -32602;

/// MCPツールセッションを実行し、イベントをクライアントにストリーミング
pub async fn run_tool_session(
    request_id: RequestId,
    initial_prompt: String,
    config: SlideConfig,
    conversation_manager: Arc<ConversationManager>,
    running_requests: Arc<Mutex<HashMap<RequestId, String>>>,
) {
    let NewConversation {
        conversation_id,
        conversation,
        session_configured,
    } = match conversation_manager.new_conversation(config).await {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to start slide session: {}", e);
            let result = CallToolResult {
                content: vec![ContentBlock::TextContent(TextContent {
                    r#type: "text".to_string(),
                    text: format!("Failed to start Slide session: {e}"),
                    annotations: None,
                })],
                is_error: Some(true),
                structured_content: None,
            };
            // Note: In a real implementation, this would send response via MCP
            return;
        }
    };

    info!("Started new slide session: {}", conversation_id);

    // セッション設定イベントを送信
    let session_event = Event {
        id: request_id.to_string(),
        msg: EventMsg::SessionConfigured(session_configured.clone()),
    };
    // Note: In a real implementation, this would send event via MCP

    // リクエストIDを登録
    let req_id_str = match &request_id {
        RequestId::String(s) => s.clone(),
        RequestId::Integer(n) => n.to_string(),
    };
    running_requests.lock().await.insert(request_id.clone(), conversation_id.clone());

    // 初期プロンプトを送信
    let submission = protocol::Submission {
        id: req_id_str.clone(),
        op: protocol::Op::UserInput {
            items: vec![protocol::InputItem::Text {
                text: initial_prompt.clone(),
            }],
        },
    };

    if let Err(e) = conversation.submit_with_id(submission).await {
        error!("Failed to submit initial prompt: {e}");
        running_requests.lock().await.remove(&request_id);
        return;
    }

    // メインイベントループを実行
    run_tool_session_inner(
        conversation,
        request_id,
        running_requests,
    )
    .await;
}

/// ツールセッションのメインイベントループ
async fn run_tool_session_inner(
    conversation: Arc<SlideConversation>,
    request_id: RequestId,
    running_requests: Arc<Mutex<HashMap<RequestId, String>>>,
) {
    let request_id_str = match &request_id {
        RequestId::String(s) => s.clone(),
        RequestId::Integer(n) => n.to_string(),
    };

    // イベントループ
    loop {
        match conversation.next_event().await {
            Ok(event) => {
                info!("Received event: {:?}", event.msg);
                
                // Note: In a real implementation, this would send event via MCP
                // send_event_as_notification(&event, Some(request_id.clone())).await;

                match event.msg {
                    EventMsg::ApplyPatchApprovalRequest(approval_event) => {
                        // パッチ適用承認を処理
                        crate::approval_processor::handle_patch_approval(
                            approval_event,
                            conversation.clone(),
                            request_id.clone(),
                        )
                        .await;
                        continue;
                    }
                    EventMsg::ExecApprovalRequest(exec_event) => {
                        // コマンド実行承認を処理
                        crate::approval_processor::handle_exec_approval(
                            exec_event,
                            conversation.clone(),
                            request_id.clone(),
                        )
                        .await;
                        continue;
                    }
                    EventMsg::Error(err_event) => {
                        error!("Session error: {}", err_event.message);
                        let result = json!({
                            "error": err_event.message,
                        });
                        // Note: In a real implementation, this would send response via MCP
                        break;
                    }
                    EventMsg::TaskComplete(task_complete) => {
                        info!("Task completed");
                        let text = task_complete.last_agent_message.unwrap_or_default();
                        let result = CallToolResult {
                            content: vec![ContentBlock::TextContent(TextContent {
                                r#type: "text".to_string(),
                                text,
                                annotations: None,
                            })],
                            is_error: None,
                            structured_content: None,
                        };
                        // Note: In a real implementation, this would send response via MCP
                        running_requests.lock().await.remove(&request_id);
                        break;
                    }
                    _ => {
                        // その他のイベントは単純に通知として転送
                        // Note: In a real implementation, this would send event via MCP
                    }
                }
            }
            Err(e) => {
                error!("Event stream error: {}", e);
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text: format!("Slide runtime error: {e}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                // Note: In a real implementation, this would send response via MCP
                break;
            }
        }
    }
}

/// MCPツールセッションの返信処理
pub async fn run_tool_session_reply(
    conversation: Arc<SlideConversation>,
    request_id: RequestId,
    prompt: String,
    running_requests: Arc<Mutex<HashMap<RequestId, String>>>,
    conversation_id: String,
) {
    running_requests.lock().await.insert(request_id.clone(), conversation_id);
    
    if let Err(e) = conversation
        .submit(protocol::Op::UserInput {
            items: vec![protocol::InputItem::Text { text: prompt }],
        })
        .await
    {
        error!("Failed to submit user input: {e}");
        running_requests.lock().await.remove(&request_id);
        return;
    }

    run_tool_session_inner(conversation, request_id, running_requests).await;
}
