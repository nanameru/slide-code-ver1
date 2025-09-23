use crate::history_cell::{HistoryCellTrait, AgentMessageCell};
use crate::app_event::AppEvent;
use ratatui::text::Line;

use super::HeaderEmitter;
use super::StreamState;

/// Sink for history insertions and animation control.
pub(crate) trait HistorySink {
    fn insert_history_cell(&self, cell: Box<dyn HistoryCellTrait>);
    fn start_commit_animation(&self);
    fn stop_commit_animation(&self);
}

/// Concrete sink backed by `AppEventSender`.
pub(crate) struct AppEventHistorySink(pub(crate) crate::app_event_sender::AppEventSender);

impl HistorySink for AppEventHistorySink {
    fn insert_history_cell(&self, cell: Box<dyn HistoryCellTrait>) {
        self.0
            .send(AppEvent::InsertHistoryCell(cell))
    }
    fn start_commit_animation(&self) {
        self.0
            .send(AppEvent::StartCommitAnimation)
    }
    fn stop_commit_animation(&self) {
        self.0
            .send(AppEvent::StopCommitAnimation)
    }
}

type Lines = Vec<Line<'static>>;

/// Controller that manages newline-gated streaming, header emission, and
/// commit animation across streams.
pub(crate) struct StreamController {
    header: HeaderEmitter,
    state: StreamState,
    active: bool,
    finishing_after_drain: bool,
}

impl StreamController {
    pub(crate) fn new() -> Self {
        Self {
            header: HeaderEmitter::new(),
            state: StreamState::new(),
            active: false,
            finishing_after_drain: false,
        }
    }

    pub(crate) fn reset_headers_for_new_turn(&mut self) {
        self.header.reset_for_new_turn();
    }

    pub(crate) fn is_write_cycle_active(&self) -> bool {
        self.active
    }

    pub(crate) fn clear_all(&mut self) {
        self.state.clear();
        self.active = false;
        self.finishing_after_drain = false;
        // leave header state unchanged; caller decides when to reset
    }

    /// Begin an answer stream. Does not emit header yet; it is emitted on first commit.
    pub(crate) fn begin(&mut self, _sink: &impl HistorySink) {
        // Starting a new stream cancels any pending finish-from-previous-stream animation.
        if !self.active {
            self.header.reset_for_stream();
        }
        self.finishing_after_drain = false;
        self.active = true;
    }

    /// Push a delta; if it contains a newline, commit completed lines and start animation.
    pub(crate) fn push_and_maybe_commit(&mut self, delta: &str, sink: &impl HistorySink) {
        if !self.active {
            return;
        }
        let state = &mut self.state;
        // Record that at least one delta was received for this stream
        if !delta.is_empty() {
            state.has_seen_delta = true;
        }
        state.collector.push_delta(delta);
        if delta.contains('\n') {
            let newly_completed = state.collector.commit_complete_lines(&());
            if !newly_completed.is_empty() {
                state.enqueue(newly_completed);
                sink.start_commit_animation();
            }
        }
    }

    /// Finalize the active stream. If `flush_immediately` is true, drain and emit now.
    pub(crate) fn finalize(&mut self, flush_immediately: bool, sink: &impl HistorySink) -> bool {
        if !self.active {
            return false;
        }
        // Finalize collector first.
        let remaining = {
            let state = &mut self.state;
            state.collector.finalize_and_drain(&())
        };
        if flush_immediately {
            // Collect all output first to avoid emitting headers when there is no content.
            let mut out_lines: Lines = Vec::new();
            {
                let state = &mut self.state;
                if !remaining.is_empty() {
                    state.enqueue(remaining);
                }
                let step = state.drain_all();
                out_lines.extend(step.history);
            }
            if !out_lines.is_empty() {
                // Insert as a HistoryCell so display drops the header while transcript keeps it.
                sink.insert_history_cell(Box::new(AgentMessageCell::new(
                    out_lines,
                    self.header.maybe_emit_header(),
                )));
            }

            // Cleanup
            self.state.clear();
            // Allow a subsequent block in this turn to emit its header.
            self.header.allow_reemit_in_turn();
            // Also clear the per-stream emitted flag so the header can render again.
            self.header.reset_for_stream();
            self.active = false;
            self.finishing_after_drain = false;
            true
        } else {
            if !remaining.is_empty() {
                let state = &mut self.state;
                state.enqueue(remaining);
            }
            // Spacer animated out
            self.state.enqueue(vec![Line::from("")]);
            self.finishing_after_drain = true;
            sink.start_commit_animation();
            false
        }
    }

    /// Process one animation step.
    pub(crate) fn step(&mut self, sink: &impl HistorySink) -> bool {
        let step = self.state.step();
        if !step.history.is_empty() {
            sink.insert_history_cell(Box::new(AgentMessageCell::new(
                step.history,
                self.header.maybe_emit_header(),
            )));
        }

        if self.finishing_after_drain && self.state.is_idle() {
            // Reset and notify
            self.state.clear();
            // Allow a subsequent block in this turn to emit its header.
            self.header.allow_reemit_in_turn();
            // Also clear the per-stream emitted flag so the header can render again.
            self.header.reset_for_stream();
            self.active = false;
            self.finishing_after_drain = false;
            sink.stop_commit_animation();
            return false;
        }

        !self.state.is_idle() || self.finishing_after_drain
    }

    pub(crate) fn has_seen_delta(&self) -> bool {
        self.state.has_seen_delta
    }
}
