use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// ASCII banner rendered at startup above the chat history.
/// Keeping it ASCII-only ensures compatibility with our lint rules.
pub const MESSAGE_PREFIX: &str = "__SLIDE_ASCII_BANNER__\n";

pub const STARTUP_BANNER: &str = r"   _____  _      _____  _____  ______      _____   ____  _____  ______ 
  / ____|| |    |_   _|/ ____||  ____|    / ____| / __ \|  __ \|  ____|
 | (___  | |      | | | |     | |__      | (___  | |  | | |__) | |__   
  \___ \ | |      | | | |     |  __|      \___ \ | |  | |  _  /|  __|  
  ____) || |____ _| |_| |____ | |____     ____) || |__| | | \ \| |____ 
 |_____/ |______|_____ |\_____| \_____|   |_____/  \____/|_|  \_\______|
";

const BANNER_COLORS: &[Color] = &[
    Color::Rgb(255, 105, 180), // hot pink
    Color::Rgb(255, 140, 105), // coral
    Color::Rgb(255, 204, 92),  // amber
    Color::Rgb(134, 214, 128), // mint
    Color::Rgb(102, 204, 255), // sky blue
    Color::Rgb(171, 148, 255), // lavender
];

/// Build the banner message that can be pushed into the chat history list.
pub fn banner_message() -> String {
    format!("{MESSAGE_PREFIX}{STARTUP_BANNER}")
}

/// Lines rendered into the terminal scrollback at startup.
pub fn banner_history_lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = STARTUP_BANNER
        .lines()
        .enumerate()
        .map(|(idx, line)| {
            let color = BANNER_COLORS
                .get(idx)
                .copied()
                .unwrap_or(Color::Rgb(144, 202, 249));
            let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
            Line::from(vec![Span::styled(line.to_string(), style)])
        })
        .collect();

    lines.push(Line::from(String::new()));

    let accent = Style::default()
        .fg(Color::Rgb(173, 216, 230))
        .add_modifier(Modifier::BOLD);
    let hint = Style::default()
        .fg(Color::Rgb(160, 174, 192))
        .add_modifier(Modifier::DIM);
    lines.push(Line::from(vec![Span::styled(
        "Welcome to Slide TUI",
        accent,
    )]));
    lines.push(Line::from(vec![Span::styled(
        "Type i to start composing, Enter to send.",
        hint,
    )]));
    lines.push(Line::from(vec![Span::styled(
        "Press h for help. Press q to quit.",
        hint,
    )]));

    lines
}
