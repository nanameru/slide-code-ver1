use ratatui::style::Stylize;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::text::Text;
use std::any::Any;

use crate::widgets::banner::banner_lines;

/// Unified role heading helpers for transcript/history.
pub(crate) fn user_heading_line() -> Line<'static> {
    Line::from("> ".fg(Color::Rgb(128, 128, 128)))
}

pub(crate) fn assistant_heading_line() -> Line<'static> {
    Line::from("⚫︎ ".white())
}

/// Represents an event to display in the conversation history. Returns its
/// `Vec<Line<'static>>` representation to make it easier to display in a
/// scrollable list.
pub(crate) trait HistoryCellTrait: std::fmt::Debug + Send + Sync + Any {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;

    fn transcript_lines(&self) -> Vec<Line<'static>> {
        self.display_lines(u16::MAX)
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.display_lines(width).len() as u16
    }

    fn is_stream_continuation(&self) -> bool {
        false
    }
    
    fn is_user(&self) -> bool {
        false
    }
    
    fn is_assistant(&self) -> bool {
        false
    }
}

impl dyn HistoryCellTrait {
    pub(crate) fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
pub(crate) struct AgentMessageCell {
    lines: Vec<Line<'static>>,
    is_first_line: bool,
}

impl AgentMessageCell {
    pub(crate) fn new(lines: Vec<Line<'static>>, is_first_line: bool) -> Self {
        Self {
            lines,
            is_first_line,
        }
    }
}

impl HistoryCellTrait for AgentMessageCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        if self.is_first_line {
            out.push(Line::from(""));
            
            // 最初の非空行を探してプレフィックスを付与
            if let Some((first_idx, first_line)) = self.lines.iter().enumerate()
                .find(|(_, line)| !line.to_string().trim().is_empty()) {
                
                // プレフィックスと最初の内容行を同一行に表示
                let mut spans = vec![Span::styled("⚫︎ ", ratatui::style::Style::default().fg(ratatui::style::Color::White))];
                spans.extend(first_line.spans.clone());
                out.push(Line::from(spans));
                
                // 残りの行を追加
                out.extend(self.lines.iter().skip(first_idx + 1).cloned());
            } else {
                // 全て空行の場合はプレフィックスのみ
                out.push(assistant_heading_line());
            }
        } else {
            out.extend(self.lines.clone());
        }
        out
    }

    fn transcript_lines(&self) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        if self.is_first_line {
            // 最初の非空行を探してプレフィックスを付与
            if let Some((first_idx, first_line)) = self.lines.iter().enumerate()
                .find(|(_, line)| !line.to_string().trim().is_empty()) {
                
                // プレフィックスと最初の内容行を同一行に表示
                let mut spans = vec![Span::styled("⚫︎ ", ratatui::style::Style::default().fg(ratatui::style::Color::White))];
                spans.extend(first_line.spans.clone());
                out.push(Line::from(spans));
                
                // 残りの行を追加
                out.extend(self.lines.iter().skip(first_idx + 1).cloned());
            } else {
                // 全て空行の場合はプレフィックスのみ
                out.push(assistant_heading_line());
            }
        } else {
            out.extend(self.lines.clone());
        }
        out
    }

    fn is_stream_continuation(&self) -> bool {
        !self.is_first_line
    }
}

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
    Mcp,
    Search,
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
    
    pub fn is_user(&self) -> bool {
        matches!(self, Self::UserPrompt { .. })
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
                let lines = split_preserving_empty(prompt);
                if lines.is_empty() {
                    vec!["> ".to_string()]
                } else {
                    let mut out = Vec::new();
                    // 最初の行はプレフィックス付き
                    out.push(format!("> {}", lines[0]));
                    // 残りの行はそのまま
                    out.extend(lines[1..].iter().cloned());
                    out
                }
            }
            HistoryCell::AssistantMessage { content } => {
                let lines = split_preserving_empty(content);
                if lines.is_empty() {
                    vec!["⚫︎ ".to_string()]
                } else {
                    let mut out = Vec::new();
                    // 最初の行はプレフィックス付き
                    out.push(format!("・ {}", lines[0]));
                    // 残りの行はそのまま
                    out.extend(lines[1..].iter().cloned());
                    out
                }
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

impl HistoryCellTrait for HistoryCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        self.lines()
    }

    fn transcript_lines(&self) -> Vec<Line<'static>> {
        self.lines()
    }
    
    fn is_user(&self) -> bool {
        self.is_user()
    }
    
    fn is_assistant(&self) -> bool {
        self.is_assistant()
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
            RoleLabel::User => "> ".fg(Color::Rgb(128, 128, 128)),  // より薄いグレー
            RoleLabel::Assistant => "⚫︎ ".white(),
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
            SystemLabel::Mcp => "mcp".light_magenta().bold(),
            SystemLabel::Search => "search".yellow().bold(),
        }
    }
}

fn build_role_block(role: RoleLabel, body: &str) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    
    // プレフィックスと最初の行を同一行に表示
    let lines: Vec<String> = split_preserving_empty(body);
    if !lines.is_empty() {
        let first_line = lines[0].trim_end_matches('\r');
        let first_formatted = match role {
            RoleLabel::Assistant => {
                // Assistantの場合：最初の行が空でない場合のみプレフィックスを付けて同一行に表示
                if first_line.trim().is_empty() {
                    // 空行をスキップして次の非空行を探す
                    let mut found_content = false;
                    for (i, line) in lines.iter().enumerate().skip(1) {
                        if !line.trim().is_empty() {
                            let mut spans = vec![role.heading_span()];
                            spans.extend(format_content_line(line.trim_end_matches('\r')).spans);
                            out.push(Line::from(spans));
                            
                            // 残りの行を処理
                            for remaining_line in &lines[(i + 1)..] {
                                let line = remaining_line.trim_end_matches('\r');
                                out.push(format_content_line(line));
                            }
                            found_content = true;
                            break;
                        }
                    }
                    if !found_content {
                        // 全て空行の場合はプレフィックスのみ
                        out.push(Line::from(vec![role.heading_span()]));
                    }
                    return out;
                } else {
                    let mut spans = vec![role.heading_span()];
                    spans.extend(format_content_line(first_line).spans);
                    Line::from(spans)
                }
            }
            RoleLabel::User => {
                // Userの場合：「> 」+ 最初の行の内容
                if first_line.trim().is_empty() {
                    // 空行の場合はプレフィックスのみ
                    Line::from(vec![role.heading_span()])
                } else {
                    Line::from(vec![
                        role.heading_span(),
                        Span::styled(first_line.to_string(), Style::default().fg(Color::Rgb(128, 128, 128)))
                    ])
                }
            }
        };
        out.push(first_formatted);
        
        // 残りの行を処理
        for line in &lines[1..] {
            let line = line.trim_end_matches('\r');
            let formatted = match role {
                RoleLabel::Assistant => format_content_line(line),
                RoleLabel::User => Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Rgb(128, 128, 128)),
                )),
            };
            out.push(formatted);
        }
    } else {
        // 空のメッセージの場合はプレフィックスのみ
        out.push(Line::from(vec![role.heading_span()]));
    }

    out
}

fn build_status_block(label: SystemLabel, lines: &[String]) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    out.push(Line::from(""));
    out.push(Line::from(vec![label.heading_span()]));

    for line in lines {
        let line = line.trim_end_matches('\r');
        
        // HTTPサーバーエラーメッセージをフィルタリング
        if label == SystemLabel::Error && should_filter_http_server_error(line) {
            continue;
        }
        
        let rendered = match label {
            SystemLabel::Exec => format_exec_line(line),
            SystemLabel::Patch => format_patch_line(line),
            SystemLabel::Error => format_error_line(line),
            SystemLabel::Diff => format_content_line(line),
            SystemLabel::Info => format_content_line(line),
            SystemLabel::Mcp => format_content_line(line),
            SystemLabel::Search => format_content_line(line),
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

/// HTTPサーバーエラーメッセージをフィルタリングするかどうか判定
fn should_filter_http_server_error(line: &str) -> bool {
    let line_lower = line.to_lowercase();
    
    // "Failed to start HTTP server on port XXXX: Address already in use" パターンをチェック
    if line_lower.contains("failed to start http server") && 
       line_lower.contains("address already in use") {
        return true;
    }
    
    // より一般的なパターンもチェック
    if line_lower.contains("http server") && 
       line_lower.contains("port") &&
       (line_lower.contains("address already in use") || line_lower.contains("addrinuse")) {
        return true;
    }
    
    false
}
