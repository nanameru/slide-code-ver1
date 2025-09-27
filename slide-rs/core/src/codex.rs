use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use async_channel::Receiver;
use async_channel::Sender;
use slide_apply_patch::ApplyPatchAction;
// use codex_login::AuthManager;  // Disabled for now
use crate::protocol::ConversationHistoryResponseEvent;
use crate::protocol::TaskStartedEvent;
use crate::protocol::TurnAbortReason;
use crate::protocol::TurnAbortedEvent;
use futures::prelude::*;
use mcp_types::CallToolResult;
use serde::Serialize;
use serde_json;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;
use uuid::Uuid;
use crate::model_provider_info::ModelProviderInfo;
use crate::apply_patch;
use crate::apply_patch::ApplyPatchExec;
use crate::apply_patch::CODEX_APPLY_PATCH_ARG1;
use crate::apply_patch::InternalApplyPatchInvocation;
use crate::apply_patch::convert_apply_patch_to_protocol;
use crate::client::ModelClient;
use crate::client_common::Prompt;
use crate::client::ResponseEvent as ClientResponseEvent;
use crate::config::Config;
use crate::config_types::ShellEnvironmentPolicy;
use crate::conversation_history::ConversationHistory;
use crate::environment_context::EnvironmentContext;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::error::SandboxErr;
use crate::error::get_error_message_ui;
use crate::exec::ExecParams;
use crate::exec::ExecToolCallOutput;
use crate::exec::SandboxType;
use crate::exec::StdoutStream;
use crate::exec::StreamOutput;
use crate::exec::process_exec_tool_call;
use crate::exec_command::EXEC_COMMAND_TOOL_NAME;
use crate::exec_command::ExecCommandParams;
use crate::exec_command::ExecSessionManager;
use crate::exec_command::WRITE_STDIN_TOOL_NAME;
use crate::exec_command::WriteStdinParams;
use crate::exec_env::create_env;
use crate::mcp_connection_manager::McpConnectionManager;
use crate::mcp_tool_call::handle_mcp_tool_call;
use crate::model_family::find_family_for_model;
use crate::openai_model_info::get_model_info;
use crate::openai_tools::ApplyPatchToolArgs;
use crate::openai_tools::ToolsConfig;
use crate::openai_tools::ToolsConfigParams;
use crate::openai_tools::get_openai_tools;
use crate::parse_command::parse_command;
use crate::plan_tool::handle_update_plan;
use crate::project_doc::get_user_instructions;
use crate::protocol::AgentMessageDeltaEvent;
use crate::protocol::AgentMessageEvent;
use crate::protocol::AgentReasoningDeltaEvent;
use crate::protocol::AgentReasoningEvent;
use crate::protocol::AgentReasoningRawContentDeltaEvent;
use crate::protocol::AgentReasoningRawContentEvent;
use crate::protocol::AgentReasoningSectionBreakEvent;
use crate::protocol::ApplyPatchApprovalRequestEvent;
use crate::protocol::AskForApproval;
use crate::protocol::BackgroundEventEvent;
use crate::protocol::ErrorEvent;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::ExecApprovalRequestEvent;
use crate::protocol::ExecCommandBeginEvent;
use crate::protocol::ExecCommandEndEvent;
use crate::protocol::FileChange;
use crate::protocol::InputItem;
use crate::protocol::ListCustomPromptsResponseEvent;
use crate::protocol::Op;
use crate::protocol::PatchApplyBeginEvent;
use crate::protocol::PatchApplyEndEvent;
use crate::protocol::ReviewDecision;
// 連続実行関連のインポート
use crate::protocol::ContinuousExecutionStartEvent;
use crate::protocol::ContinuousExecutionStepEvent;
use crate::protocol::ContinuousExecutionEndEvent;
use crate::protocol::ToolExecutionBeginEvent;
use crate::protocol::ToolExecutionEndEvent;
use crate::tool_executor::ToolExecutor;
use crate::approval_manager::AskForApproval;
use crate::seatbelt::SandboxPolicy;
use uuid;
use crate::protocol::ProcessedResponseItem;
pub use crate::protocol::{Op, ReviewDecision as PublicReviewDecision};
use crate::protocol::SessionConfiguredEvent;
use crate::protocol::StreamErrorEvent;
use crate::protocol::Submission;
use crate::protocol::TaskCompleteEvent;
use crate::protocol::TurnDiffEvent;
use crate::protocol::WebSearchBeginEvent;
use crate::protocol::WebSearchEndEvent;
use crate::rollout::RolloutRecorder;
use crate::safety::SafetyCheck;
use crate::safety::assess_command_safety;
use crate::safety::assess_safety_for_untrusted_command;
use crate::shell;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::user_notification::UserNotification;
use crate::util::backoff;
use std::time::Duration;
use codex_protocol::config_types::ReasoningEffort as ReasoningEffortConfig;
use codex_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use codex_protocol::custom_prompts::CustomPrompt;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::ShellToolCallParams;
use codex_protocol::models::WebSearchAction;

// A convenience extension trait for acquiring mutex locks where poisoning is
// unrecoverable and should abort the program. This avoids scattered `.unwrap()`
// calls on `lock()` while still surfacing a clear panic message when a lock is
// poisoned.
trait MutexExt<T> {
    fn lock_unchecked(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_unchecked(&self) -> MutexGuard<'_, T> {
        #[expect(clippy::expect_used)]
        self.lock().expect("poisoned lock")
    }
}

/// The high-level interface to the Codex system.
/// It operates as a queue pair where you send submissions and receive events.
pub struct Codex {
    next_id: AtomicU64,
    tx_sub: Sender<Submission>,
    rx_event: Receiver<Event>,
}

/// Wrapper returned by [`Codex::spawn`] containing the spawned [`Codex`],
/// the submission id for the initial `ConfigureSession` request and the
/// unique session id.
pub struct CodexSpawnOk {
    pub codex: Codex,
    pub session_id: Uuid,
}

pub(crate) const INITIAL_SUBMIT_ID: &str = "";
pub(crate) const SUBMISSION_CHANNEL_CAPACITY: usize = 64;

// Model-formatting limits: clients get full streams; only content sent to the model is truncated.
pub(crate) const MODEL_FORMAT_MAX_BYTES: usize = 10 * 1024; // 10 KiB
pub(crate) const MODEL_FORMAT_MAX_LINES: usize = 256; // lines
pub(crate) const MODEL_FORMAT_HEAD_LINES: usize = MODEL_FORMAT_MAX_LINES / 2;
pub(crate) const MODEL_FORMAT_TAIL_LINES: usize = MODEL_FORMAT_MAX_LINES - MODEL_FORMAT_HEAD_LINES; // 128
pub(crate) const MODEL_FORMAT_HEAD_BYTES: usize = MODEL_FORMAT_MAX_BYTES / 2;

impl Codex {
    /// Spawn a new [`Codex`] and initialize the session.
    pub async fn spawn(
        config: Config,
        auth_manager: Arc<AuthManager>,
        initial_history: Option<Vec<ResponseItem>>,
    ) -> CodexResult<CodexSpawnOk> {
        let (tx_sub, rx_sub) = async_channel::bounded(SUBMISSION_CHANNEL_CAPACITY);
        let (tx_event, rx_event) = async_channel::unbounded();
        
        let user_instructions = get_user_instructions(&config).await;
        let config = Arc::new(config);
        let resume_path = config.experimental_resume.clone();
        
        let configure_session = ConfigureSession {
            provider: config.model_provider.clone(),
            model: config.model.clone(),
            model_reasoning_effort: config.model_reasoning_effort,
            model_reasoning_summary: config.model_reasoning_summary,
            user_instructions,
            base_instructions: config.base_instructions.clone(),
            approval_policy: config.approval_policy,
            sandbox_policy: config.sandbox_policy.clone(),
            disable_response_storage: config.disable_response_storage,
            notify: config.notify.clone(),
            cwd: config.cwd.clone(),
            resume_path,
        };

        // Generate a unique ID for the lifetime of this Codex session.
        let (session, turn_context) = Session::new(
            configure_session,
            config.clone(),
            auth_manager.clone(),
            tx_event.clone(),
            initial_history,
        )
        .await
        .map_err(|e| {
            error!("Failed to create session: {e:#}");
            CodexErr::InternalAgentDied
        })?;
        
        let session_id = session.session_id;

        // This task will run until Op::Shutdown is received.
        tokio::spawn(submission_loop(
            session.clone(),
            turn_context,
            config,
            rx_sub,
        ));

        let codex = Codex {
            next_id: AtomicU64::new(0),
            tx_sub,
            rx_event,
        };

        Ok(CodexSpawnOk { codex, session_id })
    }

    /// Submit the `op` wrapped in a `Submission` with a unique ID.
    pub async fn submit(&self, op: Op) -> CodexResult<String> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .to_string();
        let sub = Submission { id: id.clone(), op };
        self.submit_with_id(sub).await?;
        Ok(id)
    }

    /// Use sparingly: prefer `submit()` so Codex is responsible for generating
    /// unique IDs for each submission.
    pub async fn submit_with_id(&self, sub: Submission) -> CodexResult<()> {
        self.tx_sub
            .send(sub)
            .await
            .map_err(|_| CodexErr::InternalAgentDied)?;
        Ok(())
    }

    pub async fn next_event(&self) -> CodexResult<Event> {
        let event = self
            .rx_event
            .recv()
            .await
            .map_err(|_| CodexErr::InternalAgentDied)?;
        Ok(event)
    }

    /// Convenience helper for frontends to submit an exec approval decision.
    pub async fn submit_exec_approval(&self, id: String, decision: ReviewDecision) -> CodexResult<String> {
        self.submit(Op::ExecApproval { id, decision }).await
    }

    /// Convenience helper for frontends to submit a patch approval decision.
    pub async fn submit_patch_approval(&self, id: String, decision: ReviewDecision) -> CodexResult<String> {
        self.submit(Op::PatchApproval { id, decision }).await
    }
}

/// Mutable state of the agent
#[derive(Default)]
struct State {
    approved_commands: HashSet<Vec<String>>,
    current_task: Option<AgentTask>,
    pending_approvals: HashMap<String, oneshot::Sender<ReviewDecision>>,
    pending_input: Vec<ResponseInputItem>,
    history: ConversationHistory,
}

/// Context for an initialized model agent
///
/// A session has at most 1 running task at a time, and can be interrupted by user input.
pub(crate) struct Session {
    session_id: Uuid,
    tx_event: Sender<Event>,
    /// Manager for external MCP servers/tools.
    mcp_connection_manager: McpConnectionManager,
    session_manager: ExecSessionManager,
    /// External notifier command (will be passed as args to exec()). When
    /// `None` this feature is disabled.
    notify: Option<Vec<String>>,
    /// Optional rollout recorder for persisting the conversation transcript so
    /// sessions can be replayed or inspected later.
    rollout: Mutex<Option<RolloutRecorder>>,
    state: Mutex<State>,
    codex_linux_sandbox_exe: Option<PathBuf>,
    user_shell: shell::Shell,
    show_raw_agent_reasoning: bool,
}

/// The context needed for a single turn of the conversation.
#[derive(Debug)]
pub(crate) struct TurnContext {
    pub(crate) client: ModelClient,
    /// The session's current working directory. All relative paths provided by
    /// the model as well as sandbox policies are resolved against this path
    /// instead of `std::env::current_dir()`.
    pub(crate) cwd: PathBuf,
    pub(crate) base_instructions: Option<String>,
    pub(crate) user_instructions: Option<String>,
    pub(crate) approval_policy: AskForApproval,
    pub(crate) sandbox_policy: SandboxPolicy,
    pub(crate) shell_environment_policy: ShellEnvironmentPolicy,
    pub(crate) disable_response_storage: bool,
    pub(crate) tools_config: ToolsConfig,
}

impl TurnContext {
    fn resolve_path(&self, path: Option<String>) -> PathBuf {
        path.as_ref()
            .map(PathBuf::from)
            .map_or_else(|| self.cwd.clone(), |p| self.cwd.join(p))
    }
}

/// Configure the model session.
struct ConfigureSession {
    /// Provider identifier ("openai", "openrouter", ...).
    provider: ModelProviderInfo,
    /// If not specified, server will use its default model.
    model: String,
    model_reasoning_effort: ReasoningEffortConfig,
    model_reasoning_summary: ReasoningSummaryConfig,
    /// Model instructions that are appended to the base instructions.
    user_instructions: Option<String>,
    /// Base instructions override.
    base_instructions: Option<String>,
    /// When to escalate for approval for execution
    approval_policy: AskForApproval,
    /// How to sandbox commands executed in the system
    sandbox_policy: SandboxPolicy,
    /// Disable server-side response storage (send full context each request)
    disable_response_storage: bool,
    /// Optional external notifier command tokens. Present only when the
    /// client wants the agent to spawn a program after each completed
    /// turn.
    notify: Option<Vec<String>>,
    /// Working directory that should be treated as the *root* of the
    /// session. All relative paths supplied by the model as well as the
    /// execution sandbox are resolved against this directory **instead**
    /// of the process-wide current working directory. CLI front-ends are
    /// expected to expand this to an absolute path before sending the
    /// `ConfigureSession` operation so that the business-logic layer can
    /// operate deterministically.
    cwd: PathBuf,
    resume_path: Option<PathBuf>,
}

impl Session {
    async fn new(
        configure_session: ConfigureSession,
        config: Arc<Config>,
        auth_manager: Arc<AuthManager>,
        tx_event: Sender<Event>,
        initial_history: Option<Vec<ResponseItem>>,
    ) -> anyhow::Result<(Arc<Self>, TurnContext)> {
        let ConfigureSession {
            provider,
            model,
            model_reasoning_effort,
            model_reasoning_summary,
            user_instructions,
            base_instructions,
            approval_policy,
            sandbox_policy,
            disable_response_storage,
            notify,
            cwd,
            resume_path,
        } = configure_session;

        debug!("Configuring session: model={model}; provider={provider:?}");

        if !cwd.is_absolute() {
            return Err(anyhow::anyhow!("cwd is not absolute: {cwd:?}"));
        }

        // Error messages to dispatch after SessionConfigured is sent.
        let mut post_session_configured_error_events = Vec::<Event>::new();

        // Kick off independent async setup tasks in parallel to reduce startup latency.
        //
        // - initialize RolloutRecorder with new or resumed session info
        // - spin up MCP connection manager
        // - perform default shell discovery
        // - load history metadata
        let rollout_fut = async {
            match resume_path.as_ref() {
                Some(path) => RolloutRecorder::resume(path, cwd.clone())
                    .await
                    .map(|(rec, saved)| (saved.session_id, Some(saved), rec)),
                None => {
                    let session_id = Uuid::new_v4();
                    RolloutRecorder::new(&config, session_id, user_instructions.clone())
                        .await
                        .map(|rec| (session_id, None, rec))
                }
            }
        };

        let mcp_fut = McpConnectionManager::new(config.mcp_servers.clone());
        let default_shell_fut = shell::default_user_shell();
        let history_meta_fut = crate::message_history::history_metadata(&config);

        // Join all independent futures.
        let (rollout_res, mcp_res, default_shell, (history_log_id, history_entry_count)) =
            tokio::join!(rollout_fut, mcp_fut, default_shell_fut, history_meta_fut);

        // Handle rollout result, which determines the session_id.
        struct RolloutResult {
            session_id: Uuid,
            rollout_recorder: Option<RolloutRecorder>,
            restored_items: Option<Vec<ResponseItem>>,
        }

        let rollout_result = match rollout_res {
            Ok((session_id, maybe_saved, recorder)) => {
                let restored_items: Option<Vec<ResponseItem>> = initial_history.or_else(|| {
                    maybe_saved.and_then(|saved_session| {
                        if saved_session.items.is_empty() {
                            None
                        } else {
                            Some(saved_session.items)
                        }
                    })
                });
                RolloutResult {
                    session_id,
                    rollout_recorder: Some(recorder),
                    restored_items,
                }
            }
            Err(e) => {
                if let Some(path) = resume_path.as_ref() {
                    return Err(anyhow::anyhow!(
                        "failed to resume rollout from {path:?}: {e}"
                    ));
                }
                let message = format!("failed to initialize rollout recorder: {e}");
                post_session_configured_error_events.push(Event {
                    id: INITIAL_SUBMIT_ID.to_owned(),
                    msg: EventMsg::Error(ErrorEvent {
                        message: message.clone(),
                    }),
                });
                warn!("{message}");
                RolloutResult {
                    session_id: Uuid::new_v4(),
                    rollout_recorder: None,
                    restored_items: None,
                }
            }
        };

        let RolloutResult {
            session_id,
            rollout_recorder,
            restored_items,
        } = rollout_result;

        // Create the mutable state for the Session.
        let mut state = State {
            history: ConversationHistory::new(),
            ..Default::default()
        };

        if let Some(restored_items) = restored_items {
            state.history.record_items(&restored_items);
        }

        // Handle MCP manager result and record any startup failures.
        let (mcp_connection_manager, failed_clients) = match mcp_res {
            Ok((mgr, failures)) => (mgr, failures),
            Err(e) => {
                let message = format!("Failed to create MCP connection manager: {e:#}");
                error!("{message}");
                post_session_configured_error_events.push(Event {
                    id: INITIAL_SUBMIT_ID.to_owned(),
                    msg: EventMsg::Error(ErrorEvent { message }),
                });
                (McpConnectionManager::default(), Default::default())
            }
        };

        // Surface individual client start-up failures to the user.
        if !failed_clients.is_empty() {
            for (server_name, err) in failed_clients {
                let message = format!("MCP client for `{server_name}` failed to start: {err:#}");
                error!("{message}");
                post_session_configured_error_events.push(Event {
                    id: INITIAL_SUBMIT_ID.to_owned(),
                    msg: EventMsg::Error(ErrorEvent { message }),
                });
            }
        }

        // Now that `session_id` is final (may have been updated by resume),
        // construct the model client.
        let client = ModelClient::new(
            config.clone(),
            Some(auth_manager.clone()),
            provider.clone(),
            model_reasoning_effort,
            model_reasoning_summary,
            session_id,
        );

        let turn_context = TurnContext {
            client,
            tools_config: ToolsConfig::new(&ToolsConfigParams {
                model_family: &config.model_family,
                approval_policy,
                sandbox_policy: sandbox_policy.clone(),
                include_plan_tool: config.include_plan_tool,
                include_apply_patch_tool: config.include_apply_patch_tool,
                include_web_search_request: config.tools_web_search_request,
                use_streamable_shell_tool: config.use_experimental_streamable_shell_tool,
                include_view_image_tool: config.include_view_image_tool,
            }),
            user_instructions,
            base_instructions,
            approval_policy,
            sandbox_policy,
            shell_environment_policy: config.shell_environment_policy.clone(),
            cwd,
            disable_response_storage,
        };

        let sess = Arc::new(Session {
            session_id,
            tx_event: tx_event.clone(),
            mcp_connection_manager,
            session_manager: ExecSessionManager::default(),
            notify,
            state: Mutex::new(state),
            rollout: Mutex::new(rollout_recorder),
            codex_linux_sandbox_exe: config.codex_linux_sandbox_exe.clone(),
            user_shell: default_shell,
            show_raw_agent_reasoning: config.show_raw_agent_reasoning,
        });

        // record the initial user instructions and environment context,
        // regardless of whether we restored items.
        let mut conversation_items = Vec::<ResponseItem>::with_capacity(2);
        if let Some(user_instructions) = turn_context.user_instructions.as_deref() {
            conversation_items.push(Prompt::format_user_instructions_message(user_instructions));
        }
        conversation_items.push(ResponseItem::from(EnvironmentContext::new(
            Some(turn_context.cwd.clone()),
            Some(turn_context.approval_policy),
            Some(turn_context.sandbox_policy.clone()),
            Some(sess.user_shell.clone()),
        )));

        sess.record_conversation_items(&conversation_items).await;

        // Dispatch the SessionConfiguredEvent first and then report any errors.
        let events = std::iter::once(Event {
            id: INITIAL_SUBMIT_ID.to_owned(),
            msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
                session_id,
                model,
                history_log_id,
                history_entry_count,
            }),
        })
        .chain(post_session_configured_error_events.into_iter());

        for event in events {
            if let Err(e) = tx_event.send(event).await {
                error!("failed to send event: {e:?}");
            }
        }

        Ok((sess, turn_context))
    }

    pub fn set_task(&self, task: AgentTask) {
        let mut state = self.state.lock_unchecked();
        if let Some(current_task) = state.current_task.take() {
            current_task.abort(TurnAbortReason::Replaced);
        }
        state.current_task = Some(task);
    }

    pub fn remove_task(&self, sub_id: &str) {
        let mut state = self.state.lock_unchecked();
        if let Some(task) = &state.current_task
            && task.sub_id == sub_id
        {
            state.current_task.take();
        }
    }

    /// Sends the given event to the client and swallows the send event, if
    /// any, logging it as an error.
    pub(crate) async fn send_event(&self, event: Event) {
        if let Err(e) = self.tx_event.send(event).await {
            error!("failed to send tool call event: {e}");
        }
    }

    pub async fn request_command_approval(
        &self,
        sub_id: String,
        call_id: String,
        command: Vec<String>,
        cwd: PathBuf,
        reason: Option<String>,
    ) -> oneshot::Receiver<ReviewDecision> {
        let (tx_approve, rx_approve) = oneshot::channel();

        let event = Event {
            id: sub_id.clone(),
            msg: EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                call_id,
                command,
                cwd,
                reason,
            }),
        };

        let _ = self.tx_event.send(event).await;

        {
            let mut state = self.state.lock_unchecked();
            state.pending_approvals.insert(sub_id, tx_approve);
        }

        rx_approve
    }

    pub async fn request_patch_approval(
        &self,
        sub_id: String,
        call_id: String,
        action: &ApplyPatchAction,
        reason: Option<String>,
        grant_root: Option<PathBuf>,
    ) -> oneshot::Receiver<ReviewDecision> {
        let (tx_approve, rx_approve) = oneshot::channel();

        let event = Event {
            id: sub_id.clone(),
            msg: EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                call_id,
                changes: convert_apply_patch_to_protocol(action),
                reason,
                grant_root,
            }),
        };

        let _ = self.tx_event.send(event).await;

        {
            let mut state = self.state.lock_unchecked();
            state.pending_approvals.insert(sub_id, tx_approve);
        }

        rx_approve
    }

    pub fn notify_approval(&self, sub_id: &str, decision: ReviewDecision) {
        let entry = {
            let mut state = self.state.lock_unchecked();
            state.pending_approvals.remove(sub_id)
        };

        match entry {
            Some(tx_approve) => {
                tx_approve.send(decision).ok();
            }
            None => {
                warn!("No pending approval found for sub_id: {sub_id}");
            }
        }
    }

    pub fn add_approved_command(&self, cmd: Vec<String>) {
        let mut state = self.state.lock_unchecked();
        state.approved_commands.insert(cmd);
    }

    /// Records items to both the rollout and the chat completions/ZDR
    /// transcript, if enabled.
    async fn record_conversation_items(&self, items: &[ResponseItem]) {
        debug!("Recording items for conversation: {items:?}");
        self.record_state_snapshot(items).await;
        self.state.lock_unchecked().history.record_items(items);
    }

    async fn record_state_snapshot(&self, items: &[ResponseItem]) {
        let snapshot = { crate::rollout::SessionStateSnapshot {} };
        let recorder = {
            let guard = self.rollout.lock_unchecked();
            guard.as_ref().cloned()
        };

        if let Some(rec) = recorder {
            if let Err(e) = rec.record_state(snapshot).await {
                error!("failed to record rollout state: {e:#}");
            }
            if let Err(e) = rec.record_items(items).await {
                error!("failed to record rollout items: {e:#}");
            }
        }
    }

    /// Returns the input if there was no task running to inject into
    pub fn inject_input(&self, input: Vec<InputItem>) -> Result<(), Vec<InputItem>> {
        let mut state = self.state.lock_unchecked();
        if state.current_task.is_some() {
            state.pending_input.push(input.into());
            Ok(())
        } else {
            Err(input)
        }
    }

    pub fn get_pending_input(&self) -> Vec<ResponseInputItem> {
        let mut state = self.state.lock_unchecked();
        if state.pending_input.is_empty() {
            Vec::with_capacity(0)
        } else {
            let mut ret = Vec::new();
            std::mem::swap(&mut ret, &mut state.pending_input);
            ret
        }
    }

    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> anyhow::Result<CallToolResult> {
        self.mcp_connection_manager
            .call_tool(server, tool, arguments, timeout)
            .await
    }

    fn interrupt_task(&self) {
        info!("interrupt received: abort current task, if any");
        let mut state = self.state.lock_unchecked();
        state.pending_approvals.clear();
        state.pending_input.clear();
        if let Some(task) = state.current_task.take() {
            task.abort(TurnAbortReason::Interrupted);
        }
    }

    /// Build the full turn input by concatenating the current conversation
    /// history with additional items for this turn.
    pub fn turn_input_with_history(&self, extra: Vec<ResponseItem>) -> Vec<ResponseItem> {
        [self.state.lock_unchecked().history.contents(), extra].concat()
    }

    /// Spawn the configured notifier (if any) with the given JSON payload as
    /// the last argument. Failures are logged but otherwise ignored so that
    /// notification issues do not interfere with the main workflow.
    fn maybe_notify(&self, notification: UserNotification) {
        let Some(notify_command) = &self.notify else {
            return;
        };
        if notify_command.is_empty() {
            return;
        }

        let Ok(json) = serde_json::to_string(&notification) else {
            error!("failed to serialise notification payload");
            return;
        };

        let mut command = std::process::Command::new(&notify_command[0]);
        if notify_command.len() > 1 {
            command.args(&notify_command[1..]);
        }
        command.arg(json);

        // Fire-and-forget – we do not wait for completion.
        if let Err(e) = command.spawn() {
            warn!("failed to spawn notifier '{}': {e}", notify_command[0]);
        }
    }

    /// Helper that emits a BackgroundEvent with the given message. This keeps
    /// the call‑sites terse so adding more diagnostics does not clutter the
    /// core agent logic.
    async fn notify_background_event(&self, sub_id: &str, message: impl Into<String>) {
        let event = Event {
            id: sub_id.to_string(),
            msg: EventMsg::BackgroundEvent(BackgroundEventEvent {
                message: message.into(),
            }),
        };
        let _ = self.tx_event.send(event).await;
    }

    async fn notify_stream_error(&self, sub_id: &str, message: impl Into<String>) {
        let event = Event {
            id: sub_id.to_string(),
            msg: EventMsg::StreamError(StreamErrorEvent {
                message: message.into(),
            }),
        };
        let _ = self.tx_event.send(event).await;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.interrupt_task();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExecCommandContext {
    pub(crate) sub_id: String,
    pub(crate) call_id: String,
    pub(crate) command_for_display: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) apply_patch: Option<ApplyPatchCommandContext>,
}

#[derive(Clone, Debug)]
pub(crate) struct ApplyPatchCommandContext {
    pub(crate) user_explicitly_approved_this_action: bool,
    pub(crate) changes: HashMap<PathBuf, FileChange>,
}

/// A series of Turns in response to user input.
pub(crate) struct AgentTask {
    sess: Arc<Session>,
    sub_id: String,
    handle: AbortHandle,
}

impl AgentTask {
    fn spawn(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Self {
        let handle = {
            let sess = sess.clone();
            let sub_id = sub_id.clone();
            let tc = Arc::clone(&turn_context);
            tokio::spawn(async move { run_task(sess, tc.as_ref(), sub_id, input).await })
                .abort_handle()
        };

        Self {
            sess,
            sub_id,
            handle,
        }
    }

    fn compact(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
        compact_instructions: String,
    ) -> Self {
        let handle = {
            let sess = sess.clone();
            let sub_id = sub_id.clone();
            let tc = Arc::clone(&turn_context);
            tokio::spawn(async move {
                run_compact_task(sess, tc.as_ref(), sub_id, input, compact_instructions).await
            })
            .abort_handle()
        };

        Self {
            sess,
            sub_id,
            handle,
        }
    }

    fn abort(self, reason: TurnAbortReason) {
        // TOCTOU?
        if !self.handle.is_finished() {
            self.handle.abort();
            let event = Event {
                id: self.sub_id,
                msg: EventMsg::TurnAborted(TurnAbortedEvent { reason }),
            };
            let tx_event = self.sess.tx_event.clone();
            tokio::spawn(async move {
                tx_event.send(event).await.ok();
            });
        }
    }
}

async fn submission_loop(
    sess: Arc<Session>,
    turn_context: TurnContext,
    config: Arc<Config>,
    rx_sub: Receiver<Submission>,
) {
    // Wrap once to avoid cloning TurnContext for each task.
    let mut turn_context = Arc::new(turn_context);

    // To break out of this loop, send Op::Shutdown.
    while let Ok(sub) = rx_sub.recv().await {
        debug!(?sub, "Submission");
        match sub.op {
            Op::Interrupt => {
                sess.interrupt_task();
            }
            Op::Shutdown => {
                info!("Shutting down Codex instance");
                // Gracefully flush and shutdown rollout recorder on session end so tests
                // that inspect the rollout file do not race with the background writer.
                let recorder_opt = sess.rollout.lock_unchecked().take();
                if let Some(rec) = recorder_opt
                    && let Err(e) = rec.shutdown().await
                {
                    warn!("failed to shutdown rollout recorder: {e}");
                    let event = Event {
                        id: sub.id.clone(),
                        msg: EventMsg::Error(ErrorEvent {
                            message: "Failed to shutdown rollout recorder".to_string(),
                        }),
                    };
                    if let Err(e) = sess.tx_event.send(event).await {
                        warn!("failed to send error message: {e:?}");
                    }
                }
                let event = Event {
                    id: sub.id.clone(),
                    msg: EventMsg::ShutdownComplete,
                };
                if let Err(e) = sess.tx_event.send(event).await {
                    warn!("failed to send Shutdown event: {e}");
                }
                break;
            }
            Op::Compact => {
                // Create a summarization request as user input
                const SUMMARIZATION_PROMPT: &str = include_str!("prompt_for_compact_command.md");
                // Attempt to inject input into current task
                if let Err(items) = sess.inject_input(vec![InputItem::Text {
                    text: "Start Summarization".to_string(),
                }]) {
                    let task = AgentTask::compact(
                        sess.clone(),
                        Arc::clone(&turn_context),
                        sub.id,
                        items,
                        SUMMARIZATION_PROMPT.to_string(),
                    );
                    sess.set_task(task);
                }
            }
            Op::ExecApproval { id, decision } => {
                sess.notify_approval(&id, decision);
            }
            Op::PatchApproval { id, decision } => {
                sess.notify_approval(&id, decision);
            }
            _ => {
                // Ignore unknown ops; enum is non_exhaustive to allow extensions.
            }
        }
    }
    debug!("Agent loop exited");
}

/// Takes a user message as input and runs a loop where, at each turn, the model
/// replies with either:
///
/// - requested function calls
async fn run_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
) {
    if input.is_empty() {
        return;
    }
    let event = Event {
        id: sub_id.clone(),
        msg: EventMsg::TaskStarted(TaskStartedEvent {
            model_context_window: turn_context.client.get_model_context_window(),
        }),
    };
    sess.send_event(event).await;

    let initial_input_for_turn: ResponseInputItem = ResponseInputItem::from(input);
    // For review threads, keep an isolated in-memory history so the
    // model sees a fresh conversation without the parent session's history.
    // For normal turns, continue recording to the session history as before.
    let is_review_mode = turn_context.is_review_mode;
    let mut review_thread_history: Vec<ResponseItem> = Vec::new();
    if is_review_mode {
        // Seed review threads with environment context so the model knows the working directory.
        review_thread_history.extend(sess.build_initial_context(turn_context.as_ref()));
        review_thread_history.push(initial_input_for_turn.into());
    } else {
        sess.record_input_and_rollout_usermsg(&initial_input_for_turn)
            .await;
    }

    let mut last_agent_message: Option<String> = None;
    // Although from the perspective of codex.rs, TurnDiffTracker has the lifecycle of a Task which contains
    // many turns, from the perspective of the user, it is a single turn.
    let mut turn_diff_tracker = TurnDiffTracker::new();
    let mut auto_compact_recently_attempted = false;

    loop {
        // Note that pending_input would be something like a message the user
        // submitted through the UI while the model was running. Though the UI
        // may support this, the model might not.
        let pending_input = sess
            .get_pending_input()
            .await
            .into_iter()
            .map(ResponseItem::from)
            .collect::<Vec<ResponseItem>>();

        // Construct the input that we will send to the model.
        //
        // - For review threads, use the isolated in-memory history so the
        //   model sees a fresh conversation (no parent history/user_instructions).
        //
        // - For normal turns, use the session's full history. When using the
        //   chat completions API (or ZDR clients), the model needs the full
        //   conversation history on each turn. The rollout file, however, should
        //   only record the new items that originated in this turn so that it
        //   represents an append-only log without duplicates.
        let turn_input: Vec<ResponseItem> = if is_review_mode {
            if !pending_input.is_empty() {
                review_thread_history.extend(pending_input);
            }
            review_thread_history.clone()
        } else {
            sess.record_conversation_items(&pending_input).await;
            sess.turn_input_with_history(pending_input).await
        };

        let turn_input_messages: Vec<String> = turn_input
            .iter()
            .filter_map(|item| match item {
                ResponseItem::Message { content, .. } => Some(content),
                _ => None,
            })
            .flat_map(|content| {
                content.iter().filter_map(|item| match item {
                    ContentItem::OutputText { text } => Some(text.clone()),
                    _ => None,
                })
            })
            .collect();

        match run_turn(
            &sess,
            turn_context.as_ref(),
            &mut turn_diff_tracker,
            sub_id.clone(),
            turn_input,
        )
        .await
        {
            Ok(turn_output) => {
                let TurnRunResult {
                    processed_items,
                    token_usage,
                } = turn_output;
                
                // codex-1レベルのトークン制限管理
                let limit = turn_context
                    .client
                    .get_auto_compact_token_limit()
                    .unwrap_or(i64::MAX);
                let total_usage_tokens = token_usage
                    .as_ref()
                    .map(|usage| usage.tokens_in_context_window());
                let token_limit_reached = total_usage_tokens
                    .map(|tokens| (tokens as i64) >= limit)
                    .unwrap_or(false);
                
                let mut items_to_record_in_conversation_history = Vec::<ResponseItem>::new();
                let mut responses = Vec::<ResponseInputItem>::new();

                // codex-1レベルの詳細なレスポンス処理
                for processed_response_item in processed_items {
                    let ProcessedResponseItem { item, response } = processed_response_item;
                    match (&item, &response) {
                        (ResponseItem::Message { role, .. }, None) if role == "assistant" => {
                            // If the model returned a message, we need to record it.
                            items_to_record_in_conversation_history.push(item);
                        }
                        (
                            ResponseItem::LocalShellCall { .. },
                            Some(ResponseInputItem::FunctionCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::FunctionCall { .. },
                            Some(ResponseInputItem::FunctionCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::CustomToolCall { .. },
                            Some(ResponseInputItem::CustomToolCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::CustomToolCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::FunctionCall { .. },
                            Some(ResponseInputItem::McpToolCallOutput { call_id, result }),
                        ) => {
                            items_to_record_in_conversation_history.push(item);
                            let output = match result {
                                Ok(call_tool_result) => {
                                    convert_call_tool_result_to_function_call_output_payload(
                                        call_tool_result,
                                    )
                                }
                                Err(err) => FunctionCallOutputPayload {
                                    content: err.clone(),
                                    success: Some(false),
                                },
                            };
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output,
                                },
                            );
                        }
                        (
                            ResponseItem::Reasoning {
                                id,
                                summary,
                                content,
                                encrypted_content,
                            },
                            None,
                        ) => {
                            items_to_record_in_conversation_history.push(ResponseItem::Reasoning {
                                id: id.clone(),
                                summary: summary.clone(),
                                content: content.clone(),
                                encrypted_content: encrypted_content.clone(),
                            });
                        }
                        _ => {
                            warn!("Unexpected response item: {item:?} with response: {response:?}");
                        }
                    };
                    if let Some(response) = response {
                        responses.push(response);
                    }
                }

                // Only attempt to take the lock if there is something to record.
                if !items_to_record_in_conversation_history.is_empty() {
                    if is_review_mode {
                        review_thread_history
                            .extend(items_to_record_in_conversation_history.clone());
                    } else {
                        sess.record_conversation_items(&items_to_record_in_conversation_history)
                            .await;
                    }
                }

                // codex-1レベルのトークン制限チェックとauto-compact
                if token_limit_reached {
                    if auto_compact_recently_attempted {
                        let limit_str = limit.to_string();
                        let current_tokens = total_usage_tokens
                            .map(|tokens| tokens.to_string())
                            .unwrap_or_else(|| "unknown".to_string());
                        let event = Event {
                            id: sub_id.clone(),
                            msg: EventMsg::Error(ErrorEvent {
                                message: format!(
                                    "Conversation is still above the token limit after automatic summarization (limit {limit_str}, current {current_tokens}). Please start a new session or trim your input."
                                ),
                            }),
                        };
                        sess.send_event(event).await;
                        break;
                    }
                    auto_compact_recently_attempted = true;
                    crate::compact::run_inline_auto_compact_task(sess.clone(), turn_context.clone()).await;
                    continue;
                }

                auto_compact_recently_attempted = false;

                if responses.is_empty() {
                    last_agent_message = get_last_assistant_message_from_turn(
                        &items_to_record_in_conversation_history,
                    );
                    sess.maybe_notify(UserNotification::AgentTurnComplete {
                        turn_id: sub_id.clone(),
                        input_messages: turn_input_messages,
                        last_assistant_message: last_agent_message.clone(),
                    });
                    break;
                }
                continue;
            }
            Err(e) => {
                info!("Turn error: {e:#}");
                let event = Event {
                    id: sub_id.clone(),
                    msg: EventMsg::Error(ErrorEvent {
                        message: e.to_string(),
                    }),
                };
                sess.tx_event.send(event).await.ok();
                // let the user continue the conversation
                break;
            }
        }
    }

    // If this was a review thread and we have a final assistant message,
    // try to parse it as a ReviewOutput.
    //
    // If parsing fails, construct a minimal ReviewOutputEvent using the plain
    // text as the overall explanation. Else, just exit review mode with None.
    //
    // Emits an ExitedReviewMode event with the parsed review output.
    if turn_context.is_review_mode {
        exit_review_mode(
            sess.clone(),
            sub_id.clone(),
            last_agent_message.as_deref().map(parse_review_output_event),
        )
        .await;
    }

    sess.remove_task(&sub_id).await;
    let event = Event {
        id: sub_id,
        msg: EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }),
    };
    sess.send_event(event).await;
}

async fn run_compact_task(
    sess: Arc<Session>,
    turn_context: &TurnContext,
    sub_id: String,
    input: Vec<InputItem>,
    compact_instructions: String,
) {
    let model_context_window = turn_context.client.get_model_context_window();
    let start_event = Event {
        id: sub_id.clone(),
        msg: EventMsg::TaskStarted(TaskStartedEvent {
            model_context_window,
        }),
    };
    if sess.tx_event.send(start_event).await.is_err() {
        return;
    }

    let initial_input_for_turn: ResponseInputItem = ResponseInputItem::from(input);
    let turn_input: Vec<ResponseItem> =
        sess.turn_input_with_history(vec![initial_input_for_turn.clone().into()]);

    let prompt = Prompt {
        input: turn_input,
        store: !turn_context.disable_response_storage,
        tools: Vec::new(),
        base_instructions_override: Some(compact_instructions.clone()),
    };

    let max_retries = turn_context.client.get_provider().stream_max_retries();
    let mut retries = 0;

    loop {
        let attempt_result = drain_to_completed(&sess, turn_context, &sub_id, &prompt).await;
        match attempt_result {
            Ok(()) => break,
            Err(CodexErr::Interrupted) => return,
            Err(e) => {
                if retries < max_retries {
                    retries += 1;
                    let delay = backoff(retries);
                    sess.notify_stream_error(
                        &sub_id,
                        format!(
                            "stream error: {e}; retrying {retries}/{max_retries} in {delay:?}…"
                        ),
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                    continue;
                } else {
                    let event = Event {
                        id: sub_id.clone(),
                        msg: EventMsg::Error(ErrorEvent {
                            message: e.to_string(),
                        }),
                    };
                    sess.send_event(event).await;
                    return;
                }
            }
        }
    }

    sess.remove_task(&sub_id);
    {
        let mut state = sess.state.lock_unchecked();
        state.history.keep_last_messages(1);
    }

    let event = Event {
        id: sub_id.clone(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Compact task completed".to_string(),
        }),
    };
    sess.send_event(event).await;

    let event = Event {
        id: sub_id.clone(),
        msg: EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
    };
    sess.send_event(event).await;
}

async fn run_turn(
    sess: &Session,
    turn_context: &TurnContext,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    input: Vec<ResponseItem>,
) -> CodexResult<TurnRunResult> {
    let tools = get_openai_tools(
        &turn_context.tools_config,
        Some(sess.mcp_connection_manager.list_all_tools()),
    );

    let prompt = Prompt {
        input,
        store: !turn_context.disable_response_storage,
        tools,
        base_instructions_override: turn_context.base_instructions.clone(),
    };

    let mut retries = 0;
    loop {
        match try_run_turn(sess, turn_context, turn_diff_tracker, &sub_id, &prompt).await {
            Ok(output) => return Ok(output),
            Err(CodexErr::Interrupted) => return Err(CodexErr::Interrupted),
            Err(CodexErr::EnvVar(var)) => return Err(CodexErr::EnvVar(var)),
            Err(e @ (CodexErr::UsageLimitReached(_) | CodexErr::UsageNotIncluded)) => {
                return Err(e);
            }
            Err(e) => {
                // Use the configured provider-specific stream retry budget.
                let max_retries = turn_context.client.get_provider().stream_max_retries();
                if retries < max_retries {
                    retries += 1;
                    let delay = match e {
                        CodexErr::Stream(_, Some(delay)) => delay,
                        _ => backoff(retries),
                    };
                    warn!(
                        "stream disconnected - retrying turn ({retries}/{max_retries} in {delay:?})...",
                    );
                    // Surface retry information to any UI/front‑end so the
                    // user understands what is happening instead of staring
                    // at a seemingly frozen screen.
                    sess.notify_stream_error(
                        &sub_id,
                        format!(
                            "stream error: {e}; retrying {retries}/{max_retries} in {delay:?}…"
                        ),
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                } else {
                    return Err(e);
                }
            }
        }
    }
}

/// When the model is prompted, it returns a stream of events. Some of these
/// events map to a `ResponseItem`. A `ResponseItem` may need to be
/// "handled" such that it produces a `ResponseInputItem` that needs to be
/// sent back to the model on the next turn.
#[derive(Debug)]
struct ProcessedResponseItem {
    item: ResponseItem,
    response: Option<ResponseInputItem>,
}

async fn try_run_turn(
    sess: &Session,
    turn_context: &TurnContext,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    prompt: &Prompt,
) -> CodexResult<TurnRunResult> {
    use std::borrow::Cow;
    
    // call_ids that are part of this response.
    let completed_call_ids = prompt
        .input
        .iter()
        .filter_map(|ri| match ri {
            ResponseItem::FunctionCallOutput { call_id, .. } => Some(call_id),
            ResponseItem::LocalShellCall {
                call_id: Some(call_id),
                ..
            } => Some(call_id),
            ResponseItem::CustomToolCallOutput { call_id, .. } => Some(call_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    // call_ids that were pending but are not part of this response.
    // This usually happens because the user interrupted the model before we responded to one of its tool calls
    // and then the user sent a follow-up message.
    let missing_calls = {
        prompt
            .input
            .iter()
            .filter_map(|ri| match ri {
                ResponseItem::FunctionCall { call_id, .. } => Some(call_id),
                ResponseItem::LocalShellCall {
                    call_id: Some(call_id),
                    ..
                } => Some(call_id),
                ResponseItem::CustomToolCall { call_id, .. } => Some(call_id),
                _ => None,
            })
            .filter_map(|call_id| {
                if completed_call_ids.contains(&call_id) {
                    None
                } else {
                    Some(call_id.clone())
                }
            })
            .map(|call_id| ResponseItem::CustomToolCallOutput {
                call_id,
                output: "aborted".to_string(),
            })
            .collect::<Vec<_>>()
    };
    
    let prompt: Cow<Prompt> = if missing_calls.is_empty() {
        Cow::Borrowed(prompt)
    } else {
        // Add the synthetic aborted missing calls to the beginning of the input to ensure all call ids have responses.
        let input = [missing_calls, prompt.input.clone()].concat();
        Cow::Owned(Prompt {
            input,
            ..prompt.clone()
        })
    };

    let rollout_item = RolloutItem::TurnContext(TurnContextItem {
        cwd: turn_context.cwd.clone(),
        approval_policy: turn_context.approval_policy,
        sandbox_policy: turn_context.sandbox_policy.clone(),
        model: turn_context.client.get_model(),
        effort: turn_context.client.get_reasoning_effort(),
        summary: turn_context.client.get_reasoning_summary(),
    });
    sess.persist_rollout_items(&[rollout_item]).await;
    let mut stream = turn_context.client.clone().stream(&prompt).await?;

    let mut output = Vec::new();

    loop {
        // Poll the next item from the model stream. We must inspect *both* Ok and Err
        // cases so that transient stream failures (e.g., dropped SSE connection before
        // `response.completed`) bubble up and trigger the caller's retry logic.
        let event = stream.next().await;
        let Some(event) = event else {
            // Channel closed without yielding a final Completed event or explicit error.
            // Treat as a disconnected stream so the caller can retry.
            return Err(CodexErr::Stream(
                "stream closed before response.completed".into(),
                None,
            ));
        };

        match event {
            Ok(response_event) => match response_event {
                ResponseEvent::Created => {}
                ResponseEvent::OutputItemDone(item) => {
                    let response = handle_response_item(
                        sess,
                        turn_context,
                        turn_diff_tracker,
                        sub_id,
                        item.clone(),
                    )
                    .await?;
                    output.push(ProcessedResponseItem { item, response });
                }
                ResponseEvent::WebSearchCallBegin { call_id } => {
                    let _ = sess
                        .tx_event
                        .send(Event {
                            id: sub_id.to_string(),
                            msg: EventMsg::WebSearchBegin(WebSearchBeginEvent { call_id }),
                        })
                        .await;
                }
                ResponseEvent::RateLimits(snapshot) => {
                    // Update internal state with latest rate limits, but defer sending until
                    // token usage is available to avoid duplicate TokenCount events.
                    sess.update_rate_limits(snapshot).await;
                }
                ResponseEvent::Completed {
                    response_id: _,
                    token_usage,
                } => {
                    sess.update_token_usage_info(turn_context, token_usage.as_ref())
                        .await;
                    let token_event = sess.get_token_count_event().await;
                    let _ = sess
                        .send_event(Event {
                            id: sub_id.to_string(),
                            msg: EventMsg::TokenCount(token_event),
                        })
                        .await;

                    let unified_diff = turn_diff_tracker.get_unified_diff();
                    if let Ok(Some(unified_diff)) = unified_diff {
                        let msg = EventMsg::TurnDiff(TurnDiffEvent { unified_diff });
                        let event = Event {
                            id: sub_id.to_string(),
                            msg,
                        };
                        sess.send_event(event).await;
                    }

                    let result = TurnRunResult {
                        processed_items: output,
                        token_usage: token_usage.clone(),
                    };

                    return Ok(result);
                }
                ResponseEvent::OutputTextDelta(delta) => {
                    // In review child threads, suppress assistant text deltas; the
                    // UI will show a selection popup from the final ReviewOutput.
                    if !turn_context.is_review_mode {
                        let event = Event {
                            id: sub_id.to_string(),
                            msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }),
                        };
                        sess.send_event(event).await;
                    } else {
                        trace!("suppressing OutputTextDelta in review mode");
                    }
                }
                ResponseEvent::ReasoningSummaryDelta(delta) => {
                    let event = Event {
                        id: sub_id.to_string(),
                        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta }),
                    };
                    sess.send_event(event).await;
                }
                ResponseEvent::ReasoningContentDelta(delta) => {
                    let event = Event {
                        id: sub_id.to_string(),
                        msg: EventMsg::AgentReasoningRawContentDelta(
                            AgentReasoningRawContentDeltaEvent { delta },
                        ),
                    };
                    sess.send_event(event).await;
                }
                ResponseEvent::ReasoningSummaryPartAdded => {
                    // No specific action needed for this event
                }
                ResponseEvent::CompletedWithDetails => {
                    // Handle detailed completion if needed
                }
            },
            Err(e) => {
                return Err(e);
            }
        }
    }
}

// 既存のSimple implementation用の関数（後方互換性維持）
async fn try_run_turn_simple(
    sess: &Session,
    turn_context: &TurnContext,
    _turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    prompt: &Prompt,
) -> CodexResult<TurnRunResult> {
    // Minimal streaming implementation: stream deltas from the client and forward as events.
    let rendered = prompt.render();
    let mut rx = turn_context
        .client
        .stream(rendered)
        .await
        .map_err(|_| CodexErr::InternalAgentDied)?;

    let mut assembled = String::new();
    let mut output = Vec::new();
    
    while let Some(ev) = rx.recv().await {
        match ev {
            // 既存のイベント処理（互換性維持）
            ClientResponseEvent::TextDelta(delta) => {
                assembled.push_str(&delta);
                let event = Event {
                    id: sub_id.to_string(),
                    msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }),
                };
                sess.send_event(event).await;
            }
            ClientResponseEvent::Completed => {
                // Process the assembled message to check for function calls
                let mut processed_items = Vec::new();
                
                // Try to detect and parse function calls from the assembled text
                if let Some(function_calls) = detect_function_calls_from_text(&assembled) {
                    // Process each function call
                    for (name, arguments) in function_calls {
                        let function_call_item = ResponseItem::Message {
                            role: "assistant".into(),
                            content: vec![ContentItem::FunctionCall { name, arguments }],
                        };
                        
                        let response = handle_response_item(
                            sess,
                            turn_context,
                            turn_diff_tracker,
                            sub_id,
                            function_call_item.clone(),
                        )
                        .await?;
                        
                        processed_items.push(ProcessedResponseItem {
                            item: function_call_item,
                            response,
                        });
                    }
                } else {
                    // No function calls detected, treat as regular message
                    let event = Event {
                        id: sub_id.to_string(),
                        msg: EventMsg::AgentMessage(AgentMessageEvent { message: assembled.clone() }),
                    };
                    sess.send_event(event).await;

                    let item = ResponseItem::Message {
                        role: "assistant".into(),
                        content: vec![ContentItem::OutputText { text: assembled.clone() }],
                    };
                    processed_items.push(ProcessedResponseItem { item, response: None });
                }
                
                return Ok(TurnRunResult {
                    processed_items,
                    token_usage: None,
                });
            }
            ClientResponseEvent::Error(message) => {
                let event = Event { id: sub_id.to_string(), msg: EventMsg::Error(ErrorEvent { message }) };
                sess.send_event(event).await;
                return Err(CodexErr::InternalAgentDied);
            }
            
            // 新しいResponseEventの処理
            ClientResponseEvent::Created => {
                // Created イベントは特別な処理は不要
                debug!("Response stream created");
            }
            ClientResponseEvent::OutputItemDone(item) => {
                // 最重要: 直接的なResponseItem処理
                let response = handle_response_item(
                    sess,
                    turn_context,
                    turn_diff_tracker,
                    sub_id,
                    item.clone(),
                )
                .await?;
                output.push(ProcessedResponseItem { item, response });
            }
            ClientResponseEvent::CompletedWithDetails { response_id: _, token_usage } => {
                // 詳細な完了情報付きの処理（codex-1レベル）
                
                // トークン使用量情報の更新と送信
                if let Some(token_usage) = &token_usage {
                    let token_event = Event {
                        id: sub_id.to_string(),
                        msg: EventMsg::TokenCount(token_usage.clone()),
                    };
                    sess.send_event(token_event).await;
                }
                
                // 差分情報の送信（turn_diff_trackerから取得）
                // 注意: turn_diff_trackerの実装が必要な場合は後で追加
                
                // TurnRunResult を返す（codex-1レベル）
                let result = TurnRunResult {
                    processed_items: output,
                    token_usage: token_usage.clone(),
                };
                return Ok(result);
            }
            ClientResponseEvent::OutputTextDelta(delta) => {
                // OutputTextDelta は TextDelta と同じ処理
                assembled.push_str(&delta);
                let event = Event {
                    id: sub_id.to_string(),
                    msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }),
                };
                sess.send_event(event).await;
            }
            ClientResponseEvent::ReasoningSummaryDelta(delta) => {
                // 推論サマリーのデルタ処理（codex-1レベル）
                let event = Event {
                    id: sub_id.to_string(),
                    msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta }),
                };
                sess.send_event(event).await;
            }
            ClientResponseEvent::ReasoningContentDelta(delta) => {
                // 推論コンテンツのデルタ処理（codex-1レベル）
                let event = Event {
                    id: sub_id.to_string(),
                    msg: EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent { delta }),
                };
                sess.send_event(event).await;
            }
            ClientResponseEvent::ReasoningSummaryPartAdded => {
                // 推論サマリー区切り処理
                let event = Event {
                    id: sub_id.to_string(),
                    msg: EventMsg::AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent {}),
                };
                sess.send_event(event).await;
            }
            ClientResponseEvent::WebSearchCallBegin { call_id } => {
                // Web検索開始イベント（codex-1レベル）
                let event = Event {
                    id: sub_id.to_string(),
                    msg: EventMsg::WebSearchBegin(WebSearchBeginEvent { call_id }),
                };
                sess.send_event(event).await;
            }
            ClientResponseEvent::RateLimits(snapshot) => {
                // レート制限情報の処理（codex-1レベル）
                debug!("Rate limit information received: {:?}", snapshot);
                
                // 将来的にはsess.update_rate_limits(snapshot)のような処理を追加
                // 現在は基本的なログ出力のみ
            }
        }
    }

    // OutputItemDoneで処理されたアイテムがある場合はそれを返す
    Ok(TurnRunResult {
        processed_items: output,
        token_usage: None,
    })
}

async fn drain_to_completed(
    sess: &Session,
    turn_context: &TurnContext,
    sub_id: &str,
    prompt: &Prompt,
) -> CodexResult<()> {
    let rendered = prompt.render();
    let mut rx = turn_context
        .client
        .stream(rendered)
        .await
        .map_err(|_| CodexErr::InternalAgentDied)?;

    let mut assembled = String::new();
    while let Some(ev) = rx.recv().await {
        match ev {
            ClientResponseEvent::TextDelta(delta) => {
                assembled.push_str(&delta);
                let event = Event { id: sub_id.to_string(), msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) };
                sess.send_event(event).await;
            }
            ClientResponseEvent::Completed => {
                let event = Event { id: sub_id.to_string(), msg: EventMsg::AgentMessage(AgentMessageEvent { message: assembled.clone() }) };
                sess.send_event(event).await;
                
                // 基本的な完了処理（後方互換性のため）
                let result = TurnRunResult {
                    processed_items: output,
                    token_usage: None,
                };
                return Ok(result);
            }
            ClientResponseEvent::Error(message) => {
                return Err(CodexErr::Stream(message, None));
            }
        }
    }

    // 通常はここに到達しないが、安全のため
    let result = TurnRunResult {
        processed_items: output,
        token_usage: None,
    };
    Ok(result)
}

/// Handle a response item from the model and potentially execute tools
async fn handle_response_item(
    sess: &Session,
    turn_context: &TurnContext,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    item: ResponseItem,
) -> CodexResult<Option<ResponseInputItem>> {
    use crate::conversation_history::ResponseItem;
    
    debug!(?item, "Output item");
    let output = match item {
        ResponseItem::FunctionCall {
            name,
            arguments,
            call_id,
            ..
        } => {
            info!("FunctionCall: {name}({arguments})");
            Some(
                handle_function_call(
                    sess,
                    turn_context,
                    turn_diff_tracker,
                    sub_id.to_string(),
                    name,
                    arguments,
                    call_id,
                )
                .await?,
            )
        }
        ResponseItem::LocalShellCall {
            id,
            call_id,
            status: _,
            action,
        } => {
            Some(
                handle_local_shell_call(
                    sess,
                    turn_context,
                    turn_diff_tracker,
                    sub_id.to_string(),
                    id,
                    call_id,
                    action,
                )
                .await?,
            )
        }
        ResponseItem::CustomToolCall {
            id: _,
            call_id,
            name,
            input,
            status: _,
        } => Some(
            handle_custom_tool_call(
                sess,
                turn_context,
                turn_diff_tracker,
                sub_id.to_string(),
                name,
                input,
                call_id,
            )
            .await?,
        ),
        ResponseItem::FunctionCallOutput { .. } => {
            debug!("unexpected FunctionCallOutput from stream");
            None
        }
        ResponseItem::CustomToolCallOutput { .. } => {
            debug!("unexpected CustomToolCallOutput from stream");
            None
        }
        ResponseItem::Message { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::WebSearchCall { .. } => {
            // In review child threads, suppress assistant message events but
            // keep reasoning/web search.
            let msgs = match &item {
                ResponseItem::Message { role, .. } if turn_context.is_review_mode => {
                    trace!("suppressing assistant Message in review mode");
                    Vec::new()
                }
                _ => map_response_item_to_event_messages(&item, sess.show_raw_agent_reasoning),
            };
            for msg in msgs {
                let event = Event {
                    id: sub_id.to_string(),
                    msg,
                };
                sess.send_event(event).await;
            }
            None
        }
        ResponseItem::Other => None,
    };
    Ok(output)
}

/// Handle a function call by executing the tool and returning the result
async fn handle_function_call(
    sess: &Session,
    turn_context: &TurnContext,
    _turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    name: String,
    arguments: String,
    call_id: String,
) -> CodexResult<ResponseInputItem> {
    // First, check if this is an MCP tool call
    match sess.mcp_connection_manager.parse_tool_name(&name) {
        Some((server, tool_name)) => {
            // This is an MCP tool call - handle it via MCP
            let timeout = None; // TODO: Determine appropriate timeout for tool call
            Ok(handle_mcp_tool_call(
                sess, &sub_id, call_id, server, tool_name, arguments, timeout,
            )
            .await)
        }
        None => {
            // Check for specialized tool implementations
            match name.as_str() {
                "container.exec" | "shell" => {
                    let params = match crate::container_exec::parse_container_exec_arguments(
                        arguments, turn_context, &call_id
                    ) {
                        Ok(params) => params,
                        Err(output) => {
                            return Ok(*output);
                        }
                    };
                    Ok(crate::container_exec::handle_container_exec_with_params(
                        params,
                        sess,
                        turn_context,
                        _turn_diff_tracker,
                        sub_id,
                        call_id,
                    )
                    .await)
                }
                "unified_exec" => {
                    let args = match serde_json::from_str::<crate::unified_exec::UnifiedExecArgs>(&arguments) {
                        Ok(args) => args,
                        Err(err) => {
                            return Ok(ResponseInputItem::FunctionCallOutput {
                                call_id,
                                output: FunctionCallOutputPayload {
                                    content: format!("failed to parse function arguments: {err}"),
                                    success: Some(false),
                                },
                            });
                        }
                    };

                    Ok(crate::unified_exec::handle_unified_exec_tool_call(
                        sess,
                        call_id,
                        args.session_id,
                        args.input,
                        args.timeout_ms,
                    )
                    .await)
                }
                _ => {
                    // This is a regular tool call - handle it via ToolExecutor
                    let start = std::time::Instant::now();
                    
                    // Send tool execution begin event
                    let begin_event = Event {
                        id: sub_id.clone(),
                        msg: EventMsg::ToolExecutionBegin(ToolExecutionBeginEvent {
                            call_id: call_id.clone(),
                            tool_name: name.clone(),
                            tool_input: arguments.clone(),
                        }),
                    };
                    sess.send_event(begin_event).await;
                    
                    // Create tool executor
                    let mut tool_executor = ToolExecutor::new(
                        AskForApproval::Never,
                        SandboxPolicy::Disabled,
                        turn_context.cwd.clone(),
                        turn_context.shell_environment_policy.clone(),
                    );
                    
                    // Execute the function call
                    let result = tool_executor.execute_function_call(&name, &arguments).await;
                    let duration_ms = start.elapsed().as_millis() as u64;
            
            // Send tool execution end event
            let end_event = Event {
                id: sub_id,
                msg: EventMsg::ToolExecutionEnd(ToolExecutionEndEvent {
                    call_id: call_id.clone(),
                    success: result.is_ok(),
                    duration_ms,
                }),
            };
            sess.send_event(end_event).await;
            
            // Return the result as ResponseInputItem
            match result {
                Ok(output) => Ok(ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: protocol::models::FunctionCallOutputPayload {
                        content: output,
                        success: Some(true),
                    },
                }),
                Err(e) => Ok(ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: protocol::models::FunctionCallOutputPayload {
                        content: format!("Error: {}", e),
                        success: Some(false),
                    },
                }),
            }
        }
    }
}

/// Handle LocalShellCall execution
async fn handle_local_shell_call(
    sess: &Session,
    turn_context: &TurnContext,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    id: Option<String>,
    call_id: Option<String>,
    action: crate::conversation_history::LocalShellAction,
) -> CodexResult<ResponseInputItem> {
    use crate::conversation_history::{LocalShellAction, LocalShellExecAction};
    
    let LocalShellAction::Exec(action) = action;
    tracing::info!("LocalShellCall: {action:?}");
    
    let effective_call_id = match (call_id, id) {
        (Some(call_id), _) => call_id,
        (None, Some(id)) => id,
        (None, None) => {
            error!("LocalShellCall without call_id or id");
            return Ok(ResponseInputItem::FunctionCallOutput {
                call_id: "".to_string(),
                output: crate::conversation_history::FunctionCallOutputPayload {
                    content: "LocalShellCall without call_id or id".to_string(),
                    success: Some(false),
                },
            });
        }
    };

    // Create shell command execution
    let command_str = action.command.join(" ");
    let start = std::time::Instant::now();
    
    // Send tool execution begin event
    let begin_event = Event {
        id: sub_id.clone(),
        msg: EventMsg::ToolExecutionBegin(ToolExecutionBeginEvent {
            call_id: effective_call_id.clone(),
            tool_name: "shell".to_string(),
            tool_input: command_str.clone(),
        }),
    };
    sess.send_event(begin_event).await;
    
    // Execute shell command using ToolExecutor
    let mut tool_executor = ToolExecutor::new(
        AskForApproval::Never,
        SandboxPolicy::Disabled,
        turn_context.cwd.clone(),
        turn_context.shell_environment_policy.clone(),
    );
    
    let result = tool_executor.execute_function_call("shell", &command_str).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    
    // Send tool execution end event
    let end_event = Event {
        id: sub_id,
        msg: EventMsg::ToolExecutionEnd(ToolExecutionEndEvent {
            call_id: effective_call_id.clone(),
            success: result.is_ok(),
            duration_ms,
        }),
    };
    sess.send_event(end_event).await;
    
    // Return the result as ResponseInputItem
    match result {
        Ok(output) => Ok(ResponseInputItem::FunctionCallOutput {
            call_id: effective_call_id,
            output: crate::conversation_history::FunctionCallOutputPayload {
                content: output,
                success: Some(true),
            },
        }),
        Err(e) => Ok(ResponseInputItem::FunctionCallOutput {
            call_id: effective_call_id,
            output: crate::conversation_history::FunctionCallOutputPayload {
                content: format!("Error: {}", e),
                success: Some(false),
            },
        }),
    }
}

/// Handle CustomToolCall execution
async fn handle_custom_tool_call(
    sess: &Session,
    turn_context: &TurnContext,
    _turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    name: String,
    input: String,
    call_id: String,
) -> CodexResult<ResponseInputItem> {
    tracing::info!("CustomToolCall: {name}({input})");
    
    let start = std::time::Instant::now();
    
    // Send tool execution begin event
    let begin_event = Event {
        id: sub_id.clone(),
        msg: EventMsg::ToolExecutionBegin(ToolExecutionBeginEvent {
            call_id: call_id.clone(),
            tool_name: name.clone(),
            tool_input: input.clone(),
        }),
    };
    sess.send_event(begin_event).await;
    
    // Execute custom tool using ToolExecutor
    let mut tool_executor = ToolExecutor::new(
        AskForApproval::Never,
        SandboxPolicy::Disabled,
        turn_context.cwd.clone(),
        turn_context.shell_environment_policy.clone(),
    );
    
    let result = tool_executor.execute_function_call(&name, &input).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    
    // Send tool execution end event
    let end_event = Event {
        id: sub_id,
        msg: EventMsg::ToolExecutionEnd(ToolExecutionEndEvent {
            call_id: call_id.clone(),
            success: result.is_ok(),
            duration_ms,
        }),
    };
    sess.send_event(end_event).await;
    
    // Return the result as ResponseInputItem
    match result {
        Ok(output) => Ok(ResponseInputItem::CustomToolCallOutput {
            call_id,
            output,
        }),
        Err(e) => Ok(ResponseInputItem::CustomToolCallOutput {
            call_id,
            output: format!("Error: {}", e),
        }),
    }
}

/// Convert a ResponseItem into zero or more EventMsg values that the UI can render
/// This is equivalent to codex-1's event_mapping.rs functionality
fn map_response_item_to_event_messages(
    item: &crate::conversation_history::ResponseItem,
    show_raw_agent_reasoning: bool,
) -> Vec<EventMsg> {
    use crate::conversation_history::{ResponseItem, ContentItem, ReasoningItemReasoningSummary, ReasoningItemContent, WebSearchAction};
    
    match item {
        ResponseItem::Message { role, content, .. } => {
            // Do not surface system messages as user events.
            if role == "system" {
                return Vec::new();
            }

            let mut events: Vec<EventMsg> = Vec::new();
            let mut message_parts: Vec<String> = Vec::new();
            let mut images: Vec<String> = Vec::new();

            for content_item in content.iter() {
                match content_item {
                    ContentItem::InputText { text } => {
                        message_parts.push(text.clone());
                    }
                    ContentItem::InputImage { image_url } => {
                        images.push(image_url.clone());
                    }
                    ContentItem::OutputText { text } => {
                        events.push(EventMsg::AgentMessage(AgentMessageEvent {
                            message: text.clone(),
                        }));
                    }
                    ContentItem::FunctionCall { .. } | ContentItem::FunctionResult { .. } => {
                        // These are handled by higher layers
                    }
                }
            }

            if !message_parts.is_empty() || !images.is_empty() {
                let message = if message_parts.is_empty() {
                    String::new()
                } else {
                    message_parts.join("")
                };

                events.push(EventMsg::UserMessage(UserMessageEvent {
                    message,
                    images: if images.is_empty() { None } else { Some(images) },
                }));
            }

            events
        }

        ResponseItem::Reasoning { summary, content, .. } => {
            let mut events = Vec::new();
            for ReasoningItemReasoningSummary::SummaryText { text } in summary {
                events.push(EventMsg::AgentReasoning(AgentReasoningEvent {
                    text: text.clone(),
                }));
            }
            if let Some(items) = content.as_ref().filter(|_| show_raw_agent_reasoning) {
                for c in items {
                    let text = match c {
                        ReasoningItemContent::ReasoningText { text }
                        | ReasoningItemContent::Text { text } => text,
                    };
                    events.push(EventMsg::AgentReasoningRawContent(
                        AgentReasoningRawContentEvent { text: text.clone() },
                    ));
                }
            }
            events
        }

        ResponseItem::WebSearchCall { id, action, .. } => match action {
            WebSearchAction::Search { query } => {
                let call_id = id.clone().unwrap_or_else(|| "".to_string());
                vec![EventMsg::WebSearchEnd(WebSearchEndEvent {
                    call_id,
                    query: query.clone(),
                })]
            }
            WebSearchAction::Other => Vec::new(),
        },

        // Variants that require side effects are handled by higher layers and do not emit events here.
        ResponseItem::FunctionCall { .. }
        | ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::Other => Vec::new(),
    }
}

/// Detect function calls from assembled text
/// This is a simple implementation that looks for JSON-like function call patterns
fn detect_function_calls_from_text(text: &str) -> Option<Vec<(String, String)>> {
    use regex::Regex;
    
    // Look for patterns like: function_name({"arg1": "value1", "arg2": "value2"})
    // or more complex JSON structures
    let re = Regex::new(r#"(\w+)\s*\(\s*(\{[^}]*\}|\{.*?\})\s*\)"#).ok()?;
    let mut function_calls = Vec::new();
    
    for cap in re.captures_iter(text) {
        if let (Some(name), Some(args)) = (cap.get(1), cap.get(2)) {
            function_calls.push((name.as_str().to_string(), args.as_str().to_string()));
        }
    }
    
    // Also look for explicit tool call markers that some models use
    if text.contains("<tool_call>") || text.contains("```json") {
        if let Some(calls) = parse_structured_tool_calls(text) {
            function_calls.extend(calls);
        }
    }
    
    if function_calls.is_empty() {
        None
    } else {
        Some(function_calls)
    }
}

/// Parse structured tool calls from text (e.g., markdown code blocks)
fn parse_structured_tool_calls(text: &str) -> Option<Vec<(String, String)>> {
    use regex::Regex;
    
    // Look for JSON code blocks that might contain tool calls
    let re = Regex::new(r#"```json\s*\n(.*?)\n```"#).ok()?;
    let mut function_calls = Vec::new();
    
    for cap in re.captures_iter(text) {
        if let Some(json_text) = cap.get(1) {
            // Try to parse as JSON and extract function call information
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(json_text.as_str()) {
                if let Some(obj) = json_value.as_object() {
                    if let (Some(name), Some(args)) = (obj.get("function"), obj.get("arguments")) {
                        if let (Some(name_str), Some(args_obj)) = (name.as_str(), args.as_object()) {
                            if let Ok(args_json) = serde_json::to_string(args_obj) {
                                function_calls.push((name_str.to_string(), args_json));
                            }
                        }
                    }
                }
            }
        }
    }
    
    if function_calls.is_empty() {
        None
    } else {
        Some(function_calls)
    }
}

fn get_last_assistant_message_from_turn(
    items: &[ResponseItem],
) -> Option<String> {
    for item in items.iter().rev() {
        if let ResponseItem::Message { role, content } = item {
            if role == "assistant" {
                for content_item in content {
                    if let ContentItem::OutputText { text } = content_item {
                        return Some(text.clone());
                    }
                }
            }
        }
    }
    None
}

fn format_exec_output_str(exec_output: &ExecToolCallOutput) -> String {
    let ExecToolCallOutput {
        aggregated_output, ..
    } = exec_output;

    // Head+tail truncation for the model: show the beginning and end with an elision.
    // Clients still receive full streams; only this formatted summary is capped.
    let s = aggregated_output.text.as_str();
    let total_lines = s.lines().count();

    if s.len() <= MODEL_FORMAT_MAX_BYTES && total_lines <= MODEL_FORMAT_MAX_LINES {
        return s.to_string();
    }

    let lines: Vec<&str> = s.lines().collect();
    let head_take = MODEL_FORMAT_HEAD_LINES.min(lines.len());
    let tail_take = MODEL_FORMAT_TAIL_LINES.min(lines.len().saturating_sub(head_take));
    let omitted = lines.len().saturating_sub(head_take + tail_take);

    // Join head and tail blocks (lines() strips newlines; reinsert them)
    let head_block = lines
        .iter()
        .take(head_take)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    let tail_block = if tail_take > 0 {
        lines[lines.len() - tail_take..].join("\n")
    } else {
        String::new()
    };

    let marker = format!("\n[... omitted {omitted} of {total_lines} lines ...]\n\n");

    // Byte budgets for head/tail around the marker
    let mut head_budget = MODEL_FORMAT_HEAD_BYTES.min(MODEL_FORMAT_MAX_BYTES);
    let tail_budget = MODEL_FORMAT_MAX_BYTES.saturating_sub(head_budget + marker.len());

    if tail_budget == 0 && marker.len() >= MODEL_FORMAT_MAX_BYTES {
        // Degenerate case: marker alone exceeds budget; return a clipped marker
        return take_bytes_at_char_boundary(&marker, MODEL_FORMAT_MAX_BYTES).to_string();
    }

    if tail_budget == 0 {
        // Make room for the marker by shrinking head
        head_budget = MODEL_FORMAT_MAX_BYTES.saturating_sub(marker.len());
    }

    // Enforce line-count cap by trimming head/tail lines
    let head_lines_text = head_block;
    let tail_lines_text = tail_block;

    // Build final string respecting byte budgets
    let head_part = take_bytes_at_char_boundary(&head_lines_text, head_budget);
    let mut result = String::with_capacity(MODEL_FORMAT_MAX_BYTES.min(s.len()));
    result.push_str(head_part);
    result.push_str(&marker);

    let remaining = MODEL_FORMAT_MAX_BYTES.saturating_sub(result.len());
    let tail_budget_final = remaining;
    let tail_part = take_last_bytes_at_char_boundary(&tail_lines_text, tail_budget_final);
    result.push_str(tail_part);

    result
}

// Truncate a &str to a byte budget at a char boundary (prefix)
#[inline]
fn take_bytes_at_char_boundary(s: &str, maxb: usize) -> &str {
    if s.len() <= maxb {
        return s;
    }

    let mut last_ok = 0;
    for (i, ch) in s.char_indices() {
        let nb = i + ch.len_utf8();
        if nb > maxb {
            break;
        }
        last_ok = nb;
    }
    &s[..last_ok]
}

// Take a suffix of a &str within a byte budget at a char boundary
#[inline]
fn take_last_bytes_at_char_boundary(s: &str, maxb: usize) -> &str {
    if s.len() <= maxb {
        return s;
    }

    let mut start = s.len();
    let mut used = 0usize;
    for (i, ch) in s.char_indices().rev() {
        let nb = ch.len_utf8();
        if used + nb > maxb {
            break;
        }
        start = i;
        used += nb;
        if start == 0 {
            break;
        }
    }
    &s[start..]
}