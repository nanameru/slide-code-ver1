use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// ASCII banner rendered at startup above the chat history.
/// Keeping it ASCII-only ensures compatibility with our lint rules.
pub const MESSAGE_PREFIX: &str = "__SLIDE_ASCII_BANNER__\n";

pub const STARTUP_BANNER: &str = r"  _____    _         _____    _____     ______           _____     ____     _____     ______   
 / ____|  | |       |_   _|  |  __ \   |  ____|         / ____|   / __ \   |  __ \   |  ____|  
| (___    | |         | |    | |  | |  | |__           | |       | |  | |  | |  | |  | |__     
 \___ \  | |         | |    | |  | |  |  __|          | |       | |  | |  | |  | |  |  __|    
 ____) |  | |____    _| |_   | |__| |  | |____         | |____   | |__| |  | |__| |  | |____   
|_____/  |______|  |_____|  |_____/   |______|         \_____|   \____/   |_____/   |______|  
";

const GRADIENT_START: Color = Color::Rgb(102, 204, 255); // sky blue
const GRADIENT_MID: Color = Color::Rgb(128, 224, 176); // aqua mint
const GRADIENT_END: Color = Color::Rgb(171, 148, 255); // lavender

/// Build the banner message that can be pushed into the chat history list.
pub fn banner_message() -> String {
    format!("{MESSAGE_PREFIX}{STARTUP_BANNER}")
}

/// Lines rendered into the terminal scrollback at startup.
pub fn banner_history_lines() -> Vec<Line<'static>> {
    let max_width = STARTUP_BANNER
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .max(1);

    let mut lines: Vec<Line> = STARTUP_BANNER
        .lines()
        .map(|line| {
            let spans: Vec<Span> = line
                .chars()
                .enumerate()
                .map(|(col, ch)| {
                    if ch == ' ' {
                        return Span::raw(" ");
                    }
                    let ratio = col as f32 / (max_width as f32 - 1.0);
                    let color = if ratio < 0.5 {
                        lerp_color(GRADIENT_START, GRADIENT_MID, ratio * 2.0)
                    } else {
                        lerp_color(GRADIENT_MID, GRADIENT_END, (ratio - 0.5) * 2.0)
                    };
                    let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                    Span::styled(ch.to_string(), style)
                })
                .collect();
            Line::from(spans)
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

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    ((a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0)) as u8
}

fn lerp_color(start: Color, end: Color, t: f32) -> Color {
    if let Color::Rgb(sr, sg, sb) = start {
        if let Color::Rgb(er, eg, eb) = end {
            return Color::Rgb(lerp(sr, er, t), lerp(sg, eg, t), lerp(sb, eb, t));
        }
    }
    end
}
