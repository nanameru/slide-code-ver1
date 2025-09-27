use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::{BottomPane, BottomPaneParams, InputResult};
use crate::history_cell::{HistoryCell, HistoryCellTrait, SystemLabel, AgentMessageCell, new_active_mcp_tool_call, new_completed_mcp_tool_call};
use crate::user_approval_widget::ApprovalRequest;
use crate::streaming::controller::{StreamController, AppEventHistorySink};
use crate::agent::AgentHandle;
use crate::tui::FrameRequester;

use crossterm::event::{KeyEvent, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Rect, Layout, Constraint};
use ratatui::widgets::WidgetRef;
use ratatui::buffer::Buffer;
use slide_core::protocol::{InputItem, Op, TokenUsage};
use slide_core::protocol::{CoreAskForApproval as AskForApproval, CoreSandboxPolicy as SandboxPolicy};
// 連続実行関連イベントのインポート
use slide_core::protocol::{
    ContinuousExecutionStartEvent, ContinuousExecutionStepEvent, ContinuousExecutionEndEvent,
    ToolExecutionBeginEvent, ToolExecutionEndEvent,
    McpToolCallBeginEvent, McpToolCallEndEvent
};
use tokio::sync::mpsc::UnboundedSender;

/// Common initialization parameters shared by all `ChatWidget` constructors.
pub(crate) struct ChatWidgetInit {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) agent: Option<AgentHandle>,
    pub(crate) initial_prompt: Option<String>,
    pub(crate) initial_images: Vec<PathBuf>,
    pub(crate) frame_requester: FrameRequester,
}

// Track information about an in-flight exec command.
struct RunningCommand {
    command: Vec<String>,
}

struct UserMessage {
    text: String,
    image_paths: Vec<PathBuf>,
}

impl From<String> for UserMessage {
    fn from(text: String) -> Self {
        Self {
            text,
            image_paths: Vec::new(),
        }
    }
}

pub(crate) struct ChatWidget {
    app_event_tx: AppEventSender,
    codex_op_tx: Option<UnboundedSender<Op>>,
    bottom_pane: BottomPane,
    frame_requester: FrameRequester,
    
    // Stream lifecycle controller
    stream: StreamController,
    running_commands: HashMap<String, RunningCommand>,
    task_complete_pending: bool,
    
    // Agent integration
    agent: Option<AgentHandle>,
    
    // Chat state
    history: Vec<HistoryCell>,
    chat_scroll_top: usize,
    chat_follow_bottom: bool,
    chat_viewport_height: usize,
    
    // Status tracking
    last_agent_preview: String,
    approx_output_chars: usize,
    
    // Token information
    token_info: Option<TokenUsage>,
    
    // Queued user messages
    queued_user_messages: VecDeque<UserMessage>,
}

impl ChatWidget {
    pub(crate) fn new(init: ChatWidgetInit) -> Self {
        let ChatWidgetInit {
            app_event_tx,
            agent,
            initial_prompt: _,
            initial_images: _,
            frame_requester,
        } = init;

        Self {
            app_event_tx,
            codex_op_tx: None,
            bottom_pane: BottomPane::new(BottomPaneParams {
                has_input_focus: true,
                placeholder_text: "Ask Slide Code to do anything".into(),
            }),
            frame_requester,
            stream: StreamController::new(),
            running_commands: HashMap::new(),
            task_complete_pending: false,
            agent,
            history: vec![HistoryCell::animated_banner()],
            chat_scroll_top: 0,
            chat_follow_bottom: true,
            chat_viewport_height: 0,
            last_agent_preview: String::new(),
            approx_output_chars: 0,
            token_info: None,
            queued_user_messages: VecDeque::new(),
        }
    }

    pub(crate) fn handle_key_event(&mut self, key: KeyEvent) -> Option<InputResult> {
        if let Some(result) = self.bottom_pane.handle_key_event(key) {
            match &result {
                InputResult::Submitted(text) => {
                    if !text.trim().is_empty() {
                        // Handle submission with images support
                        self.submit_message_with_images(text.clone());
                    }
                }
                InputResult::None => {}
            }
            Some(result)
        } else {
            None
        }
    }

    fn submit_message(&mut self, text: String) {
        // Insert user message
        let cell = HistoryCell::new_user_prompt(text.clone());
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));

        // Submit to agent
        if let Some(agent) = &self.agent {
            agent.submit_text_bg(text);
        }
    }

    pub(crate) fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::InsertHistoryCell(cell) => {
                // This would be handled at the app level in the real implementation
                // Here we just track for compatibility
            }
            AppEvent::ToolOutput { text } => {
                let cell = HistoryCell::new_system_status(SystemLabel::Info, [text]);
                self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            }
            _ => {
                // Handle other events as needed
            }
        }
    }

    pub(crate) fn handle_agent_message_delta(&mut self, delta: &str) {
        self.handle_streaming_delta(delta.to_string());
    }

    pub(crate) fn handle_agent_message(&mut self, message: &str) {
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        let _finished = self.stream.apply_final_answer(message, &sink);
        self.handle_if_stream_finished(true);
        
        // Update preview
        self.last_agent_preview = message.chars().take(100).collect();
        self.approx_output_chars = self.approx_output_chars.saturating_add(message.len());
    }

    fn handle_streaming_delta(&mut self, delta: String) {
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        self.stream.begin(&sink);
        self.stream.push_and_maybe_commit(&delta, &sink);
    }

    fn handle_if_stream_finished(&mut self, finished: bool) {
        if finished {
            if self.task_complete_pending {
                self.bottom_pane.set_task_running(false);
                self.task_complete_pending = false;
            }
        }
    }

    pub(crate) fn finalize_stream(&mut self) {
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        self.stream.finalize(true, &sink);
    }

    fn flush_answer_stream_with_separator(&mut self) {
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        let _ = self.stream.finalize(true, &sink);
    }

    pub(crate) fn handle_task_complete(&mut self) {
        // Finalize any streaming
        self.finalize_stream();

        // Show completion info
        if !self.last_agent_preview.is_empty() {
            let note = format!("✓ {}", self.last_agent_preview);
            let cell = HistoryCell::new_system_status(SystemLabel::Info, [note]);
            self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            self.last_agent_preview.clear();
        }

        // Show token info
        if self.approx_output_chars > 0 {
            let approx_tokens = (self.approx_output_chars as f32 / 4.0).ceil() as u64;
            let line = format!("Token approx: ~{} tokens", approx_tokens);
            let cell = HistoryCell::new_system_status(SystemLabel::Info, [line]);
            self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
            self.approx_output_chars = 0;
        }
    }

    pub(crate) fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.bottom_pane.cursor_pos(area)
    }

    pub(crate) fn set_task_running(&mut self, running: bool) {
        self.bottom_pane.set_task_running(running);
    }

    pub(crate) fn update_status_header(&mut self, text: String) {
        self.bottom_pane.update_status_header(text);
    }

    pub(crate) fn set_agent(&mut self, agent: Option<AgentHandle>) {
        self.agent = agent;
    }

    pub(crate) fn is_intercepting_input(&self) -> bool {
        self.bottom_pane.is_intercepting_input()
    }

    pub(crate) fn show_approval_modal(&mut self, req: ApprovalRequest, tx: AppEventSender) {
        self.bottom_pane.show_approval_modal(req, tx);
    }

    pub(crate) fn show_file_search(&mut self) {
        self.bottom_pane.show_file_search();
    }

    pub(crate) fn file_search_mut(&mut self) -> Option<&mut crate::bottom_pane::file_search_popup::FileSearchPopup> {
        self.bottom_pane.file_search_mut()
    }

    pub(crate) fn hide_file_search(&mut self) {
        self.bottom_pane.hide_file_search();
    }

    pub(crate) fn show_selection_view(
        &mut self,
        title: String,
        subtitle: Option<String>,
        hint: Option<String>,
        items: Vec<crate::bottom_pane::list_selection_view::SelectionItem>,
        tx: AppEventSender,
    ) {
        // Simplified implementation - delegate to bottom_pane if possible
        // For now, just update status to indicate selection
        self.update_status_header(title);
    }

    pub(crate) fn is_task_running(&self) -> bool {
        self.bottom_pane.is_task_running()
    }

    pub(crate) fn interrupt_agent(&self) {
        if let Some(agent) = &self.agent {
            agent.interrupt_bg();
        }
    }

    pub(crate) fn clear_history(&mut self) {
        self.history.clear();
    }

    pub(crate) fn take_recent_submission_images(&mut self) -> Vec<std::path::PathBuf> {
        self.bottom_pane.take_recent_submission_images()
    }

    pub(crate) fn submit_message_with_images(&mut self, text: String) {
        // Insert user message (only once)
        let cell = HistoryCell::new_user_prompt(text.clone());
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));

        // Handle images
        let images = self.take_recent_submission_images();
        
        // Submit to agent (with or without images)
        if images.is_empty() {
            // Text only - submit directly to agent
            if let Some(agent) = &self.agent {
                agent.submit_text_bg(text);
            }
        } else {
            // Build items: text then LocalImage(s)
            let mut items: Vec<InputItem> = Vec::new();
            if !text.is_empty() {
                items.push(InputItem::Text { text: text.clone() });
            }
            for p in images {
                items.push(InputItem::LocalImage { path: p });
            }
            if let Some(agent) = &self.agent {
                agent.submit_items_bg(items);
            }
        }
    }

    pub(crate) fn desired_height(&self, width: u16) -> u16 {
        self.bottom_pane.desired_height(width)
    }

    pub(crate) fn on_mouse_wheel(&mut self, delta_lines: isize) {
        // Mouse scroll controls chat history only; disable follow-to-bottom on user scroll
        if delta_lines == 0 {
            return;
        }
        if delta_lines < 0 {
            // scroll down (towards bottom)
            self.chat_scroll_top = self
                .chat_scroll_top
                .saturating_add(delta_lines.unsigned_abs() as usize);
        } else {
            // scroll up (towards top)
            let dec = delta_lines as usize;
            self.chat_scroll_top = self.chat_scroll_top.saturating_sub(dec);
        }
        self.clamp_scroll_top();
        self.chat_follow_bottom = self.chat_scroll_top >= self.max_scroll_top();
    }

    fn clamp_scroll_top(&mut self) {
        let max_top = self.max_scroll_top();
        if self.chat_scroll_top > max_top {
            self.chat_scroll_top = max_top;
        }
    }

    fn max_scroll_top(&self) -> usize {
        let total_lines = self.total_chat_lines();
        total_lines.saturating_sub(self.chat_viewport_height)
    }

    fn total_chat_lines(&self) -> usize {
        let mut history_lines: usize = self.history.iter().map(HistoryCell::line_count).sum();
        if !self.history.is_empty() {
            history_lines = history_lines.saturating_sub(1);
        }
        // plus one prompt line always
        history_lines + 1
    }

    pub(crate) fn get_agent(&self) -> &Option<AgentHandle> {
        &self.agent
    }

    pub(crate) fn get_agent_mut(&mut self) -> &mut Option<AgentHandle> {
        &mut self.agent
    }

    // --- codex-1 compatible methods ---
    
    pub(crate) fn on_commit_tick(&mut self) {
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        let finished = self.stream.step(&sink);
        self.handle_if_stream_finished(finished);
    }
    
    fn on_task_started(&mut self) {
        self.bottom_pane.set_task_running(true);
        self.stream.reset_headers_for_new_turn();
    }
    
    fn on_task_complete(&mut self, _last_agent_message: Option<String>) {
        if self.stream.is_write_cycle_active() {
            let sink = AppEventHistorySink(self.app_event_tx.clone());
            let _ = self.stream.finalize(true, &sink);
        }
        self.bottom_pane.set_task_running(false);
        self.running_commands.clear();
        self.maybe_send_next_queued_input();
    }

    fn maybe_send_next_queued_input(&mut self) {
        if self.bottom_pane.is_task_running() {
            return;
        }
        if let Some(user_message) = self.queued_user_messages.pop_front() {
            self.submit_user_message(user_message);
        }
    }
    
    fn submit_user_message(&mut self, user_message: UserMessage) {
        let UserMessage { text, image_paths } = user_message;
        
        // Only show the text portion in conversation history (once)
        if !text.is_empty() {
            let cell = HistoryCell::new_user_prompt(text.clone());
            self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        }

        // Submit to agent
        let mut items: Vec<InputItem> = Vec::new();
        if !text.is_empty() {
            items.push(InputItem::Text { text: text.clone() });
        }
        for path in image_paths {
            items.push(InputItem::LocalImage { path });
        }

        if !items.is_empty() {
            if let Some(agent) = &self.agent {
                agent.submit_items_bg(items);
            }
        }
    }
    
    pub(crate) fn submit_text_message(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.submit_user_message(text.into());
    }
    
    pub(crate) fn submit_op(&self, op: Op) {
        if let Some(tx) = &self.codex_op_tx {
            if let Err(e) = tx.send(op) {
                eprintln!("failed to submit op: {e}");
            }
        }
    }
    
    pub(crate) fn set_token_info(&mut self, info: Option<TokenUsage>) {
        self.token_info = info;
    }
    
    pub(crate) fn token_usage(&self) -> TokenUsage {
        self.token_info
            .clone()
            .unwrap_or_default()
    }
    
    pub(crate) fn handle_codex_event(&mut self, event: slide_core::codex::Event) {
        use slide_core::codex::Event;
        // Simplified event handling - delegate to app for now
        self.app_event_tx.send(AppEvent::CodexEvent(event));
    }
    
    pub(crate) fn is_normal_backtrack_mode(&self) -> bool {
        !self.bottom_pane.is_task_running()
    }
    
    pub(crate) fn composer_is_empty(&self) -> bool {
        // Simplified implementation - assume not empty for now
        false
    }
    
    fn add_to_history(&mut self, cell: HistoryCell) {
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
    }

    pub(crate) fn request_redraw(&self) {
        self.frame_requester.schedule_frame();
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        // Simplified paste handling - insert as user message
        if !text.trim().is_empty() {
            self.submit_message_with_images(text);
            self.request_redraw();
        }
    }

    // 連続実行関連のイベント処理メソッド
    pub(crate) fn on_continuous_execution_start(&mut self, event: ContinuousExecutionStartEvent) {
        let cell = HistoryCell::new_system_status(
            SystemLabel::Info,
            [format!("🚀 Starting continuous execution: {}", event.initial_input)]
        );
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        self.update_status_header("Continuous execution started".to_string());
    }

    pub(crate) fn on_continuous_execution_step(&mut self, event: ContinuousExecutionStepEvent) {
        let cell = HistoryCell::new_system_status(
            SystemLabel::Info,
            [format!("⚡ Step {}: {} - {}", event.step_number, event.tool_name, event.tool_input)]
        );
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        self.update_status_header(format!("Executing step {}: {}", event.step_number, event.tool_name));
    }

    pub(crate) fn on_continuous_execution_end(&mut self, event: ContinuousExecutionEndEvent) {
        let cell = HistoryCell::new_system_status(
            SystemLabel::Info,
            [format!("✅ Continuous execution completed in {} steps", event.total_steps)]
        );
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        self.update_status_header("Continuous execution completed".to_string());
    }

    pub(crate) fn on_tool_execution_begin(&mut self, event: ToolExecutionBeginEvent) {
        let cell = HistoryCell::new_system_status(
            SystemLabel::Info,
            [format!("🔧 Executing tool: {} ({})", event.tool_name, event.call_id)]
        );
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        self.update_status_header(format!("Executing {}", event.tool_name));
    }

    pub(crate) fn on_tool_execution_end(&mut self, event: ToolExecutionEndEvent) {
        let status_icon = if event.success { "✅" } else { "❌" };
        let cell = HistoryCell::new_system_status(
            SystemLabel::Info,
            [format!("{} Tool {} completed in {}ms", status_icon, event.tool_name, event.duration_ms)]
        );
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        
        if event.success {
            self.update_status_header(format!("{} completed successfully", event.tool_name));
        } else {
            self.update_status_header(format!("{} failed", event.tool_name));
        }
    }

    // MCP関連イベント処理メソッド
    pub(crate) fn on_mcp_tool_call_begin(&mut self, event: McpToolCallBeginEvent) {
        let cell = new_active_mcp_tool_call(event.invocation.clone());
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        self.update_status_header(format!("MCP: {} on {}", event.invocation.tool, event.invocation.server));
    }

    pub(crate) fn on_mcp_tool_call_end(&mut self, event: McpToolCallEndEvent) {
        let success = event.is_success();
        let cell = new_completed_mcp_tool_call(
            80, // num_cols - reasonable default for terminal width
            event.invocation.clone(),
            event.duration,
            success,
            event.result.clone(),
        );
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
        let status = if success { "Success" } else { "Failed" };
        self.update_status_header(format!("MCP: {} - {}", event.invocation.tool, status));
    }

    /// Update banner animation and convert to static banner when animation completes
    pub fn update_banner_animation(&mut self) {
        if let Some(first_cell) = self.history.first_mut() {
            match first_cell {
                HistoryCell::AnimatedBanner { animation } => {
                    animation.update();
                    
                    // アニメーションが完了したら静的バナーに変換
                    if !animation.is_active() {
                        *first_cell = HistoryCell::banner();
                    }
                }
                _ => {} // 他の種類のセルは何もしない
            }
        }
    }
}

impl WidgetRef for &ChatWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // 🧹 背景をクリアして、後ろのシェル履歴が透けて見えないようにする
        use ratatui::widgets::{Clear, Widget};
        Clear.render(area, buf);
        
        // 🎯 codex-1風の実装：bottom_paneを正しく描画
        // 暫定的に全エリアをbottom_paneに割り当て
        (&self.bottom_pane).render_ref(area, buf);
    }
}
