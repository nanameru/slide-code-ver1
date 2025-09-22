use std::collections::VecDeque;
use ratatui::text::Line;
use crate::history_cell::format_content_line;

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
    pub fn commit_complete_lines(&mut self, _config: &()) -> Vec<Line<'static>> {
        let source = self.buffer.clone();
        let last_newline_idx = source.rfind('\n');
        let source = if let Some(last_newline_idx) = last_newline_idx {
            source[..=last_newline_idx].to_string()
        } else {
            return Vec::new();
        };
        
        let lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();
        let mut rendered: Vec<Line<'static>> = Vec::new();
        
        for line in lines {
            rendered.push(format_content_line(&line));
        }
        
        let complete_line_count = rendered.len();
        if self.committed_line_count >= complete_line_count {
            return Vec::new();
        }

        let out_slice = &rendered[self.committed_line_count..complete_line_count];
        let out = out_slice.to_vec();
        self.committed_line_count = complete_line_count;
        out
    }

    /// Finalize the stream: emit all remaining lines beyond the last commit.
    pub fn finalize_and_drain(&mut self, _config: &()) -> Vec<Line<'static>> {
        let raw_buffer = self.buffer.clone();
        let mut source: String = raw_buffer.clone();
        if !source.ends_with('\n') {
            source.push('\n');
        }

        let lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();
        let mut rendered: Vec<Line<'static>> = Vec::new();
        
        for line in lines {
            rendered.push(format_content_line(&line));
        }

        let out = if self.committed_line_count < rendered.len() {
            rendered[self.committed_line_count..].to_vec()
        } else {
            Vec::new()
        };

        self.clear();
        out
    }
}

/// Simple animated line streamer for displaying streaming content
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
        self.queue.extend(lines);
    }

    pub fn is_idle(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn step(&mut self) -> StepResult {
        if let Some(line) = self.queue.pop_front() {
            StepResult {
                history: vec![line],
                has_more: !self.queue.is_empty(),
            }
        } else {
            StepResult {
                history: Vec::new(),
                has_more: false,
            }
        }
    }

    pub fn drain_all(&mut self) -> StepResult {
        let history = self.queue.drain(..).collect();
        StepResult {
            history,
            has_more: false,
        }
    }
}

pub(crate) struct StepResult {
    pub(crate) history: Vec<Line<'static>>,
    pub(crate) has_more: bool,
}
