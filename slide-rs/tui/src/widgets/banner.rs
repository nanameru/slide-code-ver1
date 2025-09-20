use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Marker prefix embedded in chat history strings so the chat widget can
/// recognise the banner entry and render it with full styling.
pub const MESSAGE_PREFIX: &str = "__SLIDE_ASCII_BANNER__";

const STARTUP_BANNER_LINES: &[&str] = &[
    " ██████  ██      ██ ██ ██████  ███████      ██████   ██████  ██████  ███████ ",
    "██       ██      ██ ██ ██   ██ ██          ██   ██ ██    ██ ██   ██ ██      ",
    "██   ███ ██      ██ ██ ██████  █████       ██████  ██    ██ ██████  █████   ",
    "██    ██ ██      ██ ██ ██   ██ ██          ██   ██ ██    ██ ██   ██ ██      ",
    " ██████   ██████ ██ ██ ██   ██ ███████     ██   ██  ██████  ██   ██ ███████ ",
];

const RAINBOW_STOPS: &[Color] = &[
    Color::Rgb(70, 235, 160), // mint green
    Color::Rgb(50, 205, 255), // cyan
    Color::Rgb(80, 120, 255), // blue
    Color::Rgb(150, 90, 255), // violet
    Color::Rgb(255, 85, 170), // magenta
    Color::Rgb(255, 120, 70), // orange
    Color::Rgb(255, 215, 70), // gold
    Color::Rgb(160, 255, 90), // lime
];

const BANNER_BG: Color = Color::Rgb(18, 24, 38);

/// Build the banner message that can be pushed into the chat history list.
/// The message acts as a sentinel token that the chat widget expands into
/// the richly styled banner at render time.
pub fn banner_message() -> String {
    MESSAGE_PREFIX.to_string()
}

/// Lines used to render the banner both in terminal scrollback and inside the
/// chat widget.
pub fn banner_lines() -> Vec<Line<'static>> {
    let max_width = STARTUP_BANNER_LINES
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(1);

    let mut lines: Vec<Line> = STARTUP_BANNER_LINES
        .iter()
        .map(|line| {
            let spans: Vec<Span> = line
                .chars()
                .enumerate()
                .map(|(col, ch)| {
                    if ch == ' ' {
                        return Span::styled(" ".to_string(), Style::default().bg(BANNER_BG));
                    }
                    let ratio = if max_width > 1 {
                        col as f32 / (max_width as f32 - 1.0)
                    } else {
                        0.0
                    };
                    let fg = rainbow_color(ratio);
                    Span::styled(
                        ch.to_string(),
                        Style::default()
                            .fg(fg)
                            .bg(BANNER_BG)
                            .add_modifier(Modifier::BOLD),
                    )
                })
                .collect();
            Line::from(spans)
        })
        .collect();

    lines.push(Line::from(vec![Span::styled(
        String::new(),
        Style::default().bg(BANNER_BG),
    )]));

    let accent = Style::default()
        .fg(Color::Rgb(192, 230, 255))
        .bg(BANNER_BG)
        .add_modifier(Modifier::BOLD);
    let hint = Style::default()
        .fg(Color::Rgb(150, 170, 200))
        .bg(BANNER_BG)
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

/// Lines rendered into the terminal scrollback at startup.
pub fn banner_history_lines() -> Vec<Line<'static>> {
    banner_lines()
}

fn rainbow_color(ratio: f32) -> Color {
    if RAINBOW_STOPS.len() < 2 {
        return RAINBOW_STOPS.first().cloned().unwrap_or(Color::White);
    }
    let clamped = ratio.clamp(0.0, 1.0);
    let scaled = clamped * (RAINBOW_STOPS.len() as f32 - 1.0);
    let idx = scaled.floor() as usize;
    let next_idx = idx.min(RAINBOW_STOPS.len() - 1);
    let next = (idx + 1).min(RAINBOW_STOPS.len() - 1);
    let local_t = scaled - idx as f32;
    lerp_color(RAINBOW_STOPS[next_idx], RAINBOW_STOPS[next], local_t)
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    ((a as f32 + (b as f32 - a as f32) * t)
        .round()
        .clamp(0.0, 255.0)) as u8
}

fn lerp_color(start: Color, end: Color, t: f32) -> Color {
    if let Color::Rgb(sr, sg, sb) = start {
        if let Color::Rgb(er, eg, eb) = end {
            return Color::Rgb(lerp(sr, er, t), lerp(sg, eg, t), lerp(sb, eb, t));
        }
    }
    end
}
