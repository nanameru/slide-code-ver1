use std::sync::Arc;

use protocol::protocol::{Event, EventMsg, InputItem, TaskStartedEvent, TaskCompleteEvent, ErrorEvent};
use tracing::warn;

pub(crate) const COMPACT_TRIGGER_TEXT: &str = "Start Summarization";
const SUMMARIZATION_PROMPT: &str = r#"
You are an AI assistant tasked with summarizing a conversation history to reduce token usage while preserving important context.

Your goal is to create a concise summary that:
1. Preserves the key decisions, outcomes, and current state
2. Maintains context needed for future interactions
3. Removes redundant or less important details
4. Keeps the conversation flow understandable

Please provide a clear, structured summary of the conversation history provided.
"#;

// 簡略化されたauto-compact実装（将来の完全実装のためのプレースホルダー）
pub(crate) async fn run_inline_auto_compact_task(
    _sess: std::sync::Arc<crate::container_exec::Session>,
    _turn_context: std::sync::Arc<crate::container_exec::TurnContext>,
) {
    warn!("Auto-compact triggered but simplified implementation does nothing yet");
    // 将来的にはここで会話履歴の要約処理を実装
    // 現在は警告ログのみ出力
}

