use crate::conversation_history::{ResponseItem, ContentItem};
use protocol::protocol::{EventMsg, AgentMessageEvent};

/// Convert a `ResponseItem` into zero or more `EventMsg` values that the UI can render.
///
/// When `show_raw_agent_reasoning` is false, raw reasoning content events are omitted.
pub(crate) fn map_response_item_to_event_messages(
    item: &ResponseItem,
    _show_raw_agent_reasoning: bool,
) -> Vec<EventMsg> {
    match item {
        ResponseItem::Message { role, content, .. } => {
            // Do not surface system messages as user events.
            if role == "system" {
                return Vec::new();
            }

            let mut events: Vec<EventMsg> = Vec::new();
            let mut message = String::new();

            for content_item in content {
                match content_item {
                    ContentItem::OutputText { text } => {
                        message = text.clone();
                    }
                    ContentItem::InputText { text } => {
                        message = text.clone();
                    }
                    ContentItem::FunctionCall { .. } => {
                        // Function calls are handled separately
                        continue;
                    }
                    ContentItem::InputImage { .. } => {
                        // Images are not handled in this simplified version
                        continue;
                    }
                    ContentItem::FunctionResult { .. } => {
                        // Function results are not handled in this simplified version
                        continue;
                    }
                }
            }

            if !message.is_empty() {
                events.push(EventMsg::AgentMessage(AgentMessageEvent {
                    message,
                }));
            }

            events
        }

        _ => Vec::new(),
    }
}