use ratatui::text::Line;
use crate::history_cell::format_content_line;

pub(crate) fn append_markdown(
    markdown_source: &str,
    lines: &mut Vec<Line<'static>>,
    _config: &(),
) {
    // Simple markdown rendering - just process line by line
    for line in markdown_source.lines() {
        lines.push(format_content_line(line));
    }
}
