use std::time::{Duration, Instant};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// アニメーション管理用の構造体
pub struct AnimationManager {
    start_time: Instant,
}

impl AnimationManager {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }

    /// シマーアニメーション（テキストがキラキラする）
    pub fn shimmer_spans(&self, text: &str) -> Vec<Span<'static>> {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return Vec::new();
        }

        // codex-1から移植したシマー効果
        let padding = 10usize;
        let period = chars.len() + padding * 2;
        let sweep_seconds = 2.0f32;
        let pos_f = (self.start_time.elapsed().as_secs_f32() % sweep_seconds) 
            / sweep_seconds * (period as f32);
        let pos = pos_f as usize;
        
        // True colorサポート検証
        let has_true_color = true; // 簡単のため常にTrue colorを仮定
        let band_half_width = 3.0;

        let mut spans: Vec<Span<'static>> = Vec::with_capacity(chars.len());
        for (i, ch) in chars.iter().enumerate() {
            let i_pos = i as isize + padding as isize;
            let pos = pos as isize;
            let dist = (i_pos - pos).abs() as f32;

            let t = if dist <= band_half_width {
                let x = std::f32::consts::PI * (dist / band_half_width);
                0.5 * (1.0 + x.cos())
            } else {
                0.0
            };
            
            let brightness = 0.4 + 0.6 * t;
            let level = (brightness * 255.0).clamp(0.0, 255.0) as u8;
            
            let style = if has_true_color {
                Style::default()
                    .fg(Color::Rgb(level, level, level))
                    .add_modifier(Modifier::BOLD)
            } else {
                self.color_for_level(level)
            };
            
            spans.push(Span::styled(ch.to_string(), style));
        }
        spans
    }

    /// スピナーアニメーション（回転する点々）
    pub fn spinner_char(&self) -> &'static str {
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let idx = (self.start_time.elapsed().as_millis() / 100) as usize % frames.len();
        frames[idx]
    }

    /// シンプルなドットスピナー
    pub fn dot_spinner(&self) -> &'static str {
        let frames = [".", "..", "...", "…"];
        let idx = (self.start_time.elapsed().as_millis() / 300) as usize % frames.len();
        frames[idx]
    }

    /// フレーム要求用のタイミング
    pub fn should_request_frame(&self) -> bool {
        // 32ms間隔でフレーム更新（約30fps）
        self.start_time.elapsed().as_millis() % 32 == 0
    }

    /// 次のフレームまでの時間
    pub fn next_frame_delay(&self) -> Duration {
        Duration::from_millis(32)
    }

    fn color_for_level(&self, level: u8) -> Style {
        // True colorが使えない場合のフォールバック
        if level < 160 {
            Style::default().add_modifier(Modifier::DIM)
        } else if level < 224 {
            Style::default()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        }
    }
}

impl Default for AnimationManager {
    fn default() -> Self {
        Self::new()
    }
}
