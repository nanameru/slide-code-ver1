// Re-export the standalone `protocol` crate so existing `crate::protocol::*`
// imports continue to work within the `core` crate and downstream crates.
pub use protocol::*;

// Export our enhanced types for compatibility
pub use crate::approval_manager::{AskForApproval as CoreAskForApproval, ApprovalManager, ApprovalRequest, ApprovalResponse};
pub use crate::seatbelt::SandboxPolicy as CoreSandboxPolicy;
pub use crate::exec_sandboxed::{SandboxedExecutor, ExecParams as CoreExecParams, ExecResult as CoreExecResult};

// Re-export exec event types for unified access
pub use crate::exec::{
    Event, EventMsg, ExecCommandOutputDeltaEvent, ExecCompleteEvent,
    ExecApprovalRequestEvent, TaskCompleteEvent, AgentMessageEvent,
    ExecOutputStream, StdoutStream
};

use async_channel::{Receiver, Sender};
use tokio::sync::mpsc;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Event dispatcher for managing and routing events
#[derive(Clone)]
pub struct EventDispatcher {
    event_sender: Sender<Event>,
    subscribers: HashMap<String, mpsc::UnboundedSender<Event>>,
}

impl EventDispatcher {
    pub fn new() -> (Self, Receiver<Event>) {
        let (event_sender, event_receiver) = async_channel::unbounded();
        let dispatcher = Self {
            event_sender,
            subscribers: HashMap::new(),
        };
        (dispatcher, event_receiver)
    }

    /// Send an event to all subscribers
    pub async fn dispatch(&self, event: Event) -> Result<(), async_channel::SendError<Event>> {
        self.event_sender.send(event).await
    }

    /// Subscribe to events with a given subscription ID
    pub fn subscribe(&mut self, sub_id: String) -> mpsc::UnboundedReceiver<Event> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.insert(sub_id, tx);
        rx
    }

    /// Unsubscribe from events
    pub fn unsubscribe(&mut self, sub_id: &str) {
        self.subscribers.remove(sub_id);
    }

    /// Get the event sender for creating StdoutStream
    pub fn get_event_sender(&self) -> Sender<Event> {
        self.event_sender.clone()
    }
}

/// Event processor for handling stream events
pub struct EventProcessor {
    dispatcher: EventDispatcher,
    receiver: Receiver<Event>,
}

impl EventProcessor {
    pub fn new() -> Self {
        let (dispatcher, receiver) = EventDispatcher::new();
        Self {
            dispatcher,
            receiver,
        }
    }

    /// Start processing events in a background task
    pub async fn start_processing(&mut self) {
        while let Ok(event) = self.receiver.recv().await {
            self.process_event(event).await;
        }
    }

    /// Process a single event
    async fn process_event(&self, event: Event) {
        match &event.data {
            EventMsg::ExecCommandOutputDelta(delta_event) => {
                tracing::debug!(
                    "Processing output delta for call_id: {}, stream: {:?}",
                    delta_event.call_id,
                    delta_event.stream
                );
            }
            EventMsg::ExecComplete(complete_event) => {
                tracing::info!(
                    "Command completed for call_id: {} with exit_code: {}",
                    complete_event.call_id,
                    complete_event.exit_code
                );
            }
            EventMsg::ExecApprovalRequest(approval_event) => {
                tracing::warn!(
                    "Approval requested for call_id: {}, command: {:?}",
                    approval_event.call_id,
                    approval_event.command
                );
            }
            EventMsg::TaskComplete(task_event) => {
                tracing::info!(
                    "Task completed for sub_id: {}, success: {}",
                    task_event.sub_id,
                    task_event.success
                );
            }
            EventMsg::AgentMessage(agent_event) => {
                tracing::info!(
                    "Agent message from sub_id: {}, level: {}",
                    agent_event.sub_id,
                    agent_event.level
                );
            }
        }
    }

    /// Get the dispatcher for external use
    pub fn get_dispatcher(&self) -> EventDispatcher {
        self.dispatcher.clone()
    }
}

/// Session manager for tracking active tool executions
#[derive(Default)]
pub struct SessionManager {
    active_sessions: HashMap<String, SessionInfo>,
    event_dispatcher: Option<EventDispatcher>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub call_id: String,
    pub command: Vec<String>,
    pub start_time: std::time::Instant,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    Completed { exit_code: i32 },
    Failed { error: String },
    TimedOut,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_dispatcher(mut self, dispatcher: EventDispatcher) -> Self {
        self.event_dispatcher = Some(dispatcher);
        self
    }

    /// Start a new session
    pub fn start_session(
        &mut self,
        session_id: String,
        call_id: String,
        command: Vec<String>,
    ) -> SessionInfo {
        let session_info = SessionInfo {
            session_id: session_id.clone(),
            call_id,
            command,
            start_time: std::time::Instant::now(),
            status: SessionStatus::Running,
        };

        self.active_sessions.insert(session_id, session_info.clone());
        session_info
    }

    /// Complete a session
    pub async fn complete_session(
        &mut self,
        session_id: &str,
        exit_code: i32,
        stdout: String,
        stderr: String,
    ) {
        if let Some(mut session) = self.active_sessions.remove(session_id) {
            session.status = SessionStatus::Completed { exit_code };

            // Send completion event
            if let Some(dispatcher) = &self.event_dispatcher {
                let event = Event {
                    event_type: "exec_complete".to_string(),
                    data: EventMsg::ExecComplete(ExecCompleteEvent {
                        sub_id: session.session_id.clone(),
                        call_id: session.call_id.clone(),
                        exit_code,
                        duration_ms: session.start_time.elapsed().as_millis() as u64,
                        stdout,
                        stderr,
                    }),
                };
                let _ = dispatcher.dispatch(event).await;
            }
        }
    }

    /// Fail a session
    pub async fn fail_session(&mut self, session_id: &str, error: String) {
        if let Some(mut session) = self.active_sessions.remove(session_id) {
            session.status = SessionStatus::Failed { error: error.clone() };

            // Send failure event
            if let Some(dispatcher) = &self.event_dispatcher {
                let event = Event {
                    event_type: "task_complete".to_string(),
                    data: EventMsg::TaskComplete(TaskCompleteEvent {
                        sub_id: session.session_id.clone(),
                        success: false,
                        message: error,
                    }),
                };
                let _ = dispatcher.dispatch(event).await;
            }
        }
    }

    /// Get active sessions
    pub fn get_active_sessions(&self) -> &HashMap<String, SessionInfo> {
        &self.active_sessions
    }
}