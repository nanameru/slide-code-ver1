use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// シンプルな行単位ストリーミング制御。
/// - デルタをバッファに貯め、改行が来たら“完成した行”だけを返す。
/// - 先頭で見出し行（空行 + ラベル）を一度だけ挿入し、その後は本文のみを流す。
/// - finalize() で残りの未完テキストを1行として返す。
#[derive(Default, Debug)]
pub struct AnswerStreamState {
    buffer: String,
    header_emitted: bool,
    active: bool,
}

impl AnswerStreamState {
    pub fn new() -> Self { Self::default() }

    /// デルタを反映し、完成した行を返す（最後の未完部分は保持）。
    pub fn push_delta(&mut self, delta: &str) -> Vec<Line<'static>> {
        if delta.is_empty() { return Vec::new(); }
        self.active = true;
        self.buffer.push_str(delta);

        let mut out: Vec<Line<'static>> = Vec::new();
        if self.buffer.contains('\n') {
            // バッファを一旦取り出して借用衝突を避ける
            let owned = std::mem::take(&mut self.buffer);
            let mut parts: Vec<&str> = owned.split('\n').collect();
            // 最後の要素は未完テキスト（終端が\nなら空文字）
            let tail = parts.pop().unwrap_or("");
            for line in parts.into_iter() {
                let line = line.strip_suffix('\r').unwrap_or(line);
                if !self.header_emitted {
                    // 空行 + 見出し
                    out.push(Line::from(""));
                    out.push(self.header_line());
                    self.header_emitted = true;
                }
                out.push(Line::from(String::from(line)));
            }
            self.buffer.push_str(tail);
        }
        out
    }

    /// 未出力の残りを1行として返し、状態をクリア。
    pub fn finalize(&mut self) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        if !self.buffer.is_empty() {
            let tail = self.buffer.trim_end_matches('\r');
            if !self.header_emitted {
                out.push(Line::from(""));
                out.push(self.header_line());
                self.header_emitted = true;
            }
            out.push(Line::from(String::from(tail)));
        }
        self.buffer.clear();
        self.header_emitted = false;
        self.active = false;
        out
    }

    fn header_line(&self) -> Line<'static> {
        Line::from(
            Span::styled(
                "slide",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        )
    }
}
