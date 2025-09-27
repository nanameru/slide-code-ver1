use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::time::{Duration, Instant};

/// Marker prefix embedded in chat history strings so the chat widget can
/// recognise the banner entry and render it with full styling.
pub const MESSAGE_PREFIX: &str = "__SLIDE_ASCII_BANNER__";

const STARTUP_BANNER_LINES: &[&str] = &[
    " ███          ████████   ███        █████  █████████░  ██████████ ",
    "░░░███       ███░░░░░███ ███       ░░███    ███░░░███  ░███░░░░░█░",
    "  ░░░███     ███░░░░░░░  ███        ░███    ███░░░░███ ░███  █ ░",
    "    ░░░███   ░████████   ███        ░███    ███░░░░███ ░██████   ",
    "     ███░    ░░░░░░░███  ███        ░███    ███░░░░███ ░███░░█   ",
    "   ███░      ███░░░░░███ ███        ░███    ███░░░███  ░███ ░   █",
    " ███░        ░████████   █████████  █████  █████████░  ██████████",
    "░░░          ░░░░░░░░  ░░░░░░░░░  ░░░░░    ░░░░░░░░   ░░░░░░░░░░ ",
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

/// Animation configuration
const ANIMATION_DURATION: Duration = Duration::from_millis(2000); // 2秒間のアニメーション
const FRAME_DURATION: Duration = Duration::from_millis(100); // 100ms per frame

/// Banner animation state
#[derive(Clone, Debug)]
pub struct BannerAnimation {
    start_time: Option<Instant>,
    is_active: bool,
}

impl BannerAnimation {
    pub fn new() -> Self {
        Self {
            start_time: None, // 遅延開始：最初はNone
            is_active: true,
        }
    }

    /// アニメーションを開始する（初回描画時に呼び出される）
    pub fn start_if_not_started(&mut self) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
    }

    pub fn is_active(&self) -> bool {
        if let Some(start_time) = self.start_time {
            self.is_active && start_time.elapsed() < ANIMATION_DURATION
        } else {
            true // まだ開始していない場合はアクティブとみなす
        }
    }

    pub fn update(&mut self) {
        if let Some(start_time) = self.start_time {
            if start_time.elapsed() >= ANIMATION_DURATION {
                self.is_active = false;
            }
        }
    }

    pub fn animation_progress(&self) -> f32 {
        if !self.is_active() {
            return 1.0;
        }
        
        if let Some(start_time) = self.start_time {
            let elapsed = start_time.elapsed().as_millis() as f32;
            let total = ANIMATION_DURATION.as_millis() as f32;
            (elapsed / total).clamp(0.0, 1.0)
        } else {
            0.0 // まだ開始していない場合は0%
        }
    }
}

/// Build the banner message that can be pushed into the chat history list.
/// The message acts as a sentinel token that the chat widget expands into
/// the richly styled banner at render time.
pub fn banner_message() -> String {
    MESSAGE_PREFIX.to_string()
}

/// Lines used to render the banner both in terminal scrollback and inside the
/// chat widget.
pub fn banner_lines() -> Vec<Line<'static>> {
    banner_lines_with_animation(None)
}

/// Lines used to render the banner with optional animation state.
pub fn banner_lines_with_animation(mut animation: Option<&mut BannerAnimation>) -> Vec<Line<'static>> {
    // 描画時にアニメーションを開始
    if let Some(ref mut anim) = animation {
        anim.start_if_not_started();
    }
    
    let animation_progress = animation.as_ref().map(|a| a.animation_progress()).unwrap_or(1.0);
    
    let mut lines: Vec<Line> = STARTUP_BANNER_LINES
        .iter()
        .enumerate()
        .map(|(line_idx, line)| {
            let line_len = line.chars().count();
            
            // アニメーション効果: 上から下へ順次表示
            let line_reveal_progress = if animation.is_some() {
                let lines_count = STARTUP_BANNER_LINES.len();
                let line_start = line_idx as f32 / lines_count as f32;
                let line_end = (line_idx + 1) as f32 / lines_count as f32;
                
                if animation_progress < line_start {
                    0.0 // まだ表示されない
                } else if animation_progress > line_end {
                    1.0 // 完全に表示
                } else {
                    // 部分的に表示（この行の表示進行度）
                    (animation_progress - line_start) / (line_end - line_start)
                }
            } else {
                1.0
            };
            
            let visible_chars = (line_len as f32 * line_reveal_progress) as usize;
            
            let spans: Vec<Span> = line
                .chars()
                .enumerate()
                .map(|(col, ch)| {
                    if col >= visible_chars {
                        return Span::raw(" "); // まだ表示されない文字は空白
                    }
                    
                    if ch == ' ' {
                        return Span::raw(" ");
                    }
                    
                    let ratio = if line_len > 1 {
                        col as f32 / (line_len as f32 - 1.0)
                    } else {
                        0.0
                    };
                    
                    // アニメーション中は色を時間で変化させる
                    let color_offset = if animation.is_some() {
                        animation_progress * 0.5 // 色相を少しずつシフト
                    } else {
                        0.0
                    };
                    
                    let fg = rainbow_color((ratio + color_offset) % 1.0);
                    let style = if ch == '░' {
                        Style::default().fg(fg).add_modifier(Modifier::DIM)
                    } else {
                        Style::default().fg(fg).add_modifier(Modifier::BOLD)
                    };
                    Span::styled(ch.to_string(), style)
                })
                .collect();
            Line::from(spans)
        })
        .collect();

    lines.push(Line::default());

    let accent = Style::default()
        .fg(Color::Rgb(192, 230, 255))
        .add_modifier(Modifier::BOLD);
    let hint = Style::default()
        .fg(Color::Rgb(150, 170, 200))
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
