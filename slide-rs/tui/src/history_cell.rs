use ratatui::style::Stylize;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::widgets::banner::banner_lines;

#[derive(Clone, Debug)]
pub enum HistoryCell {
    Banner,
    UserPrompt {
        prompt: String,
    },
    AssistantMessage {
        content: String,
    },
    SystemStatus {
        label: SystemLabel,
        lines: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemLabel {
    Exec,
    Patch,
    Diff,
    Error,
    Info,
}

impl HistoryCell {
    pub fn banner() -> Self {
        Self::Banner
    }

    pub fn new_user_prompt<S: Into<String>>(prompt: S) -> Self {
        Self::UserPrompt {
            prompt: prompt.into(),
        }
    }

    pub fn new_assistant_message<S: Into<String>>(content: S) -> Self {
        Self::AssistantMessage {
            content: content.into(),
        }
    }

    pub fn new_system_status<I, S>(label: SystemLabel, lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::SystemStatus {
            label,
            lines: lines.into_iter().map(Into::into).collect(),
        }
    }

    pub fn append_assistant_delta(&mut self, delta: &str) {
        if let Self::AssistantMessage { content } = self {
            content.push_str(delta);
        }
    }

    pub fn set_assistant_message<S: Into<String>>(&mut self, content: S) {
        if let Self::AssistantMessage { content: existing } = self {
            *existing = content.into();
        }
    }

    pub fn is_assistant(&self) -> bool {
        matches!(self, Self::AssistantMessage { .. })
    }

    pub fn lines(&self) -> Vec<Line<'static>> {
        match self {
            HistoryCell::Banner => banner_lines(),
            HistoryCell::UserPrompt { prompt } => build_role_block(RoleLabel::User, prompt),
            HistoryCell::AssistantMessage { content } => {
                build_role_block(RoleLabel::Assistant, content)
            }
            HistoryCell::SystemStatus { label, lines } => build_status_block(*label, lines),
        }
    }

    pub fn plain_text_lines(&self) -> Vec<String> {
        match self {
            HistoryCell::Banner => banner_lines().into_iter().map(line_to_plain).collect(),
            HistoryCell::UserPrompt { prompt } => {
                let mut out = Vec::new();
                out.push("user".to_string());
                out.extend(split_preserving_empty(prompt));
                out
            }
            HistoryCell::AssistantMessage { content } => {
                let mut out = Vec::new();
                out.push("assistant".to_string());
                out.extend(split_preserving_empty(content));
                out
            }
            HistoryCell::SystemStatus { label, lines } => {
                let mut out = Vec::new();
                let heading = label.heading_span();
                out.push(heading.content.to_string());
                out.extend(lines.iter().cloned());
                out
            }
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines().len()
    }
}

#[derive(Clone, Copy, Debug)]
enum RoleLabel {
    User,
    Assistant,
}

impl RoleLabel {
    fn heading_span(&self) -> Span<'static> {
        match self {
            RoleLabel::User => "user".cyan().bold(),
            RoleLabel::Assistant => "assistant".green().bold(),
        }
    }
}

impl SystemLabel {
    fn heading_span(&self) -> Span<'static> {
        match self {
            SystemLabel::Exec => ">_".magenta().bold(),
            SystemLabel::Patch => "✏️ patch".magenta().bold(),
            SystemLabel::Diff => "diff".light_blue().bold(),
            SystemLabel::Error => "error".red().bold(),
            SystemLabel::Info => "info".blue().bold(),
        }
    }
}

fn build_role_block(role: RoleLabel, body: &str) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    out.push(Line::from(""));

    out.push(Line::from(vec![role.heading_span()]));

    for line in split_preserving_empty(body) {
        let line = line.trim_end_matches('\r');
        let formatted = match role {
            RoleLabel::Assistant => format_content_line(line),
            RoleLabel::User => Line::from(line.to_string()),
        };
        out.push(formatted);
    }

    out
}

fn build_status_block(label: SystemLabel, lines: &[String]) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    out.push(Line::from(""));
    out.push(Line::from(vec![label.heading_span()]));

    for line in lines {
        let line = line.trim_end_matches('\r');
        let rendered = match label {
            SystemLabel::Exec => format_exec_line(line),
            SystemLabel::Patch => format_patch_line(line),
            SystemLabel::Error => format_error_line(line),
            SystemLabel::Diff => format_content_line(line),
            SystemLabel::Info => format_content_line(line),
        };
        out.push(rendered);
    }

    out
}

fn format_exec_line(line: &str) -> Line<'static> {
    let trimmed = line.trim();
    if trimmed.starts_with('$') {
        Line::from(line.to_string().yellow().bold())
    } else if trimmed.starts_with("exit") {
        Line::from(line.to_string().dim())
    } else {
        format_content_line(line)
    }
}

fn format_patch_line(line: &str) -> Line<'static> {
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("ok") {
        Line::from(line.to_string().green().bold())
    } else if trimmed.eq_ignore_ascii_case("failed") {
        Line::from(line.to_string().red().bold())
    } else {
        format_content_line(line)
    }
}

fn format_error_line(line: &str) -> Line<'static> {
    Line::from(line.to_string().red().bold())
}

pub(crate) fn format_content_line(line: &str) -> Line<'static> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Line::from(String::new());
    }

    // Basic markdown-ish affordances to improve readability (codex-1 風)
    if trimmed.starts_with("### ") {
        return Line::from(line.to_string().cyan().bold());
    } else if trimmed.starts_with("## ") {
        return Line::from(line.to_string().cyan().bold());
    } else if trimmed.starts_with("# ") {
        return Line::from(line.to_string().cyan().bold());
    } else if trimmed.starts_with("```") {
        return Line::from(line.to_string().magenta());
    }

    if trimmed.starts_with("Updated Plan") {
        Line::from(line.to_string().blue().bold())
    } else if trimmed.starts_with("Proposed Change") {
        Line::from(line.to_string().yellow().bold())
    } else if trimmed.starts_with("Change Approved") {
        Line::from(line.to_string().green().bold())
    } else if trimmed.starts_with("Explored") {
        Line::from(line.to_string().cyan().bold())
    } else if trimmed.starts_with("[Tool Execution Result]") {
        Line::from(line.to_string().magenta().bold())
    } else if trimmed.starts_with("[Tool Execution]") {
        Line::from(line.to_string().yellow().bold())
    } else if trimmed.starts_with('▶') {
        Line::from(line.to_string().yellow())
    } else if trimmed.starts_with('+') {
        Line::from(line.to_string().green())
    } else if trimmed.starts_with('-') {
        Line::from(line.to_string().red())
    } else if trimmed.starts_with("@@") {
        Line::from(line.to_string().cyan().bold())
    } else if trimmed.starts_with('□') || trimmed.starts_with('☑') {
        format_checkbox_line(trimmed)
    } else if looks_like_path(trimmed) {
        Line::from(line.to_string().light_blue())
    } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") || looks_like_numbered_list(trimmed) {
        Line::from(line.to_string().dim())
    } else {
        Line::from(line.to_string())
    }
}

fn format_checkbox_line(trimmed: &str) -> Line<'static> {
    let (checkbox, rest, color) = if trimmed.starts_with('□') {
        ("□", trimmed.get(3..).unwrap_or_default(), Color::Gray)
    } else {
        ("☑", trimmed.get(3..).unwrap_or_default(), Color::Green)
    };

    Line::from(vec![
        Span::raw("  "),
        Span::styled(checkbox.to_string(), Style::default().fg(color)),
        Span::raw(" "),
        Span::raw(rest.to_string()),
    ])
}

fn split_preserving_empty(text: &str) -> Vec<String> {
    text.split('\n').map(|s| s.to_string()).collect()
}

fn line_to_plain(line: Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.to_string())
        .collect()
}

fn looks_like_path(s: &str) -> bool {
    // simple heuristic: contains a directory separator and a dot-extension-like token
    (s.contains('/') || s.contains('\\')) && s.split_whitespace().any(|tok| tok.contains('.') )
}

fn looks_like_numbered_list(s: &str) -> bool {
    // e.g., "1. item" / "10. item"
    let mut it = s.chars();
    let mut saw_digit = false;
    while let Some(ch) = it.next() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        if ch == '.' {
            return saw_digit && it.next().map(|c| c == ' ').unwrap_or(false);
        }
        break;
    }
    false
}
