use std::collections::VecDeque;
use ratatui::text::Line;
use crate::markdown;
use crate::render::line_utils;

/// Newline-gated accumulator that renders markdown and commits only fully
/// completed logical lines.
pub(crate) struct MarkdownStreamCollector {
    buffer: String,
    committed_line_count: usize,
}

impl MarkdownStreamCollector {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            committed_line_count: 0,
        }
    }

    /// Returns the number of logical lines that have already been committed
    /// (i.e., previously returned from `commit_complete_lines`).
    pub fn committed_count(&self) -> usize {
        self.committed_line_count
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.committed_line_count = 0;
    }

    /// Replace the buffered content and mark that the first `committed_count`
    /// logical lines are already committed.
    pub fn replace_with_and_mark_committed(&mut self, s: &str, committed_count: usize) {
        self.buffer.clear();
        self.buffer.push_str(s);
        self.committed_line_count = committed_count;
    }

    pub fn push_delta(&mut self, delta: &str) {
        self.buffer.push_str(delta);
    }

    /// Render the full buffer and return only the newly completed logical lines
    /// since the last commit. When the buffer does not end with a newline, the
    /// final rendered line is considered incomplete and is not emitted.
    pub fn commit_complete_lines(&mut self, config: &()) -> Vec<Line<'static>> {
        let source = self.buffer.clone();
        let last_newline_idx = source.rfind('\n');
        let source = if let Some(last_newline_idx) = last_newline_idx {
            source[..=last_newline_idx].to_string()
        } else {
            return Vec::new();
        };
        let mut rendered: Vec<Line<'static>> = Vec::new();
        markdown::append_markdown(&source, &mut rendered, config);
        let mut complete_line_count = rendered.len();
        if complete_line_count > 0
            && line_utils::is_blank_line_spaces_only(&rendered[complete_line_count - 1])
        {
            complete_line_count -= 1;
        }

        if self.committed_line_count >= complete_line_count {
            return Vec::new();
        }

        let out_slice = &rendered[self.committed_line_count..complete_line_count];

        let out = out_slice.to_vec();
        self.committed_line_count = complete_line_count;
        out
    }

    /// Finalize the stream: emit all remaining lines beyond the last commit.
    /// If the buffer does not end with a newline, a temporary one is appended
    /// for rendering. Optionally unwraps ```markdown language fences in
    /// non-test builds.
    pub fn finalize_and_drain(&mut self, config: &()) -> Vec<Line<'static>> {
        let raw_buffer = self.buffer.clone();
        let mut source: String = raw_buffer.clone();
        if !source.ends_with('\n') {
            source.push('\n');
        }

        let mut rendered: Vec<Line<'static>> = Vec::new();
        markdown::append_markdown(&source, &mut rendered, config);

        let out = if self.committed_line_count >= rendered.len() {
            Vec::new()
        } else {
            rendered[self.committed_line_count..].to_vec()
        };

        // Reset collector state for next stream.
        self.clear();
        out
    }
}

pub(crate) struct StepResult {
    pub history: Vec<Line<'static>>, // lines to insert into history this step
}

/// Streams already-rendered rows into history while computing the newest K
/// rows to show in a live overlay.
pub(crate) struct AnimatedLineStreamer {
    queue: VecDeque<Line<'static>>,
}

impl AnimatedLineStreamer {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }

    pub fn enqueue(&mut self, lines: Vec<Line<'static>>) {
        for l in lines {
            self.queue.push_back(l);
        }
    }

    pub fn step(&mut self) -> StepResult {
        let mut history = Vec::new();
        // Move exactly one per tick to animate gradual insertion.
        let burst = if self.queue.is_empty() { 0 } else { 1 };
        for _ in 0..burst {
            if let Some(l) = self.queue.pop_front() {
                history.push(l);
            }
        }

        StepResult { history }
    }

    pub fn drain_all(&mut self) -> StepResult {
        let mut history = Vec::new();
        while let Some(l) = self.queue.pop_front() {
            history.push(l);
        }
        StepResult { history }
    }

    pub fn is_idle(&self) -> bool {
        self.queue.is_empty()
    }
}
