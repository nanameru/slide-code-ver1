use ratatui::text::Line;

/// Options for wrapping text with ratatui
pub(crate) struct RtOptions {
    width: usize,
    initial_indent: String,
    subsequent_indent: String,
}

impl RtOptions {
    pub fn new(width: usize) -> Self {
        Self {
            width,
            initial_indent: String::new(),
            subsequent_indent: String::new(),
        }
    }

    pub fn initial_indent(mut self, indent: String) -> Self {
        self.initial_indent = indent;
        self
    }

    pub fn subsequent_indent(mut self, indent: String) -> Self {
        self.subsequent_indent = indent;
        self
    }
}

/// Word wrap lines with ratatui options
pub(crate) fn word_wrap_lines(
    lines: &[Line<'static>],
    options: RtOptions,
) -> Vec<Line<'static>> {
    let mut result = Vec::new();
    
    for (line_idx, line) in lines.iter().enumerate() {
        let content: String = line.spans.iter()
            .map(|span| span.content.to_string())
            .collect();
        
        let prefix = if line_idx == 0 {
            &options.initial_indent
        } else {
            &options.subsequent_indent
        };
        
        // Simple word wrapping
        let wrapped = textwrap::wrap(&content, options.width.saturating_sub(prefix.len()));
        
        for (wrap_idx, wrapped_line) in wrapped.iter().enumerate() {
            let mut spans = Vec::new();
            if wrap_idx == 0 && !prefix.is_empty() {
                spans.push(ratatui::text::Span::raw(prefix.clone()));
            } else if wrap_idx > 0 && !options.subsequent_indent.is_empty() {
                spans.push(ratatui::text::Span::raw(options.subsequent_indent.clone()));
            }
            spans.push(ratatui::text::Span::raw(wrapped_line.to_string()));
            
            result.push(Line::from(spans));
        }
    }
    
    result
}

/// Simple word wrap for a single line
pub(crate) fn word_wrap_line(
    line: &Line<'static>,
    options: RtOptions,
) -> Vec<Line<'static>> {
    word_wrap_lines(&[line.clone()], options)
}
