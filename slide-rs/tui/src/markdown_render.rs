use ratatui::text::Line;

pub struct RenderedMarkdown {
    pub lines: Vec<Line<'static>>,
}

pub fn render_markdown_text(_markdown_source: &str) -> RenderedMarkdown {
    // Simplified implementation for testing
    RenderedMarkdown {
        lines: vec![Line::from("Test markdown")],
    }
}

pub fn render_markdown_text_with_citations(
    markdown_source: &str,
    _scheme: &str,
    _cwd: &std::path::Path,
) -> RenderedMarkdown {
    // Simplified implementation
    RenderedMarkdown {
        lines: markdown_source.lines()
            .map(|line| crate::history_cell::format_content_line(line))
            .collect(),
    }
}
