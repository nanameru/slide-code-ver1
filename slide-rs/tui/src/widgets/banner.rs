use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// ASCII banner rendered at startup above the chat history.
/// Keeping it ASCII-only ensures compatibility with our lint rules.
pub const MESSAGE_PREFIX: &str = "__SLIDE_ASCII_BANNER__\n";

pub const STARTUP_BANNER: &str = r"SLIDE CODE
  _____ _ _     _      _____          _      
 / ____| (_)   | |    / ____|        | |     
| (___ | |_  __| |___| |     ___   __| | ___ 
 \___ \| | |/ _` / __| |    / _ \ / _` |/ _ \
 ____) | | | (_| \__ \ |___| (_) | (_| |  __/
|_____/|_|_|\__,_|___/\_____\___/ \__,_|\___|
";

/// Build the banner message that can be pushed into the chat history list.
pub fn banner_message() -> String {
    format!("{MESSAGE_PREFIX}{STARTUP_BANNER}")
}

/// Lines rendered into the terminal scrollback at startup.
pub fn banner_history_lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = STARTUP_BANNER
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect();

    lines.push(Line::from(String::new()));

    let dim = Style::default().add_modifier(Modifier::DIM);
    lines.push(Line::from(vec![Span::styled("Welcome to Slide TUI!", dim)]));
    lines.push(Line::from(vec![Span::styled(
        "Type i to start composing, Enter to send.",
        dim,
    )]));
    lines.push(Line::from(vec![Span::styled(
        "Press h for help. Press q to quit.",
        dim,
    )]));

    lines
}
