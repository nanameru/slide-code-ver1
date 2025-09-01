use crossterm::event::KeyEvent;
use ratatui::{buffer::Buffer, layout::Rect};

use super::{BottomPane, CancellationEvent};

/// 下部ペインに表示できるビューが実装するトレイト
pub(crate) trait BottomPaneView {
    /// アクティブ中にキーイベントを処理（処理後は再描画が行われる想定）
    fn handle_key_event(&mut self, _pane: &mut BottomPane, _key_event: KeyEvent) {}

    /// ビューが完了した場合は true を返す（ペインから取り除かれる）
    fn is_complete(&self) -> bool {
        false
    }

    /// Ctrl-C の処理（既定は無視）
    fn on_ctrl_c(&mut self, _pane: &mut BottomPane) -> CancellationEvent {
        CancellationEvent::Ignored
    }

    /// 望ましい高さ（行数）
    fn desired_height(&self, width: u16) -> u16;

    /// コンテンツの描画（コンポーザーの代わりに描画される）
    fn render(&self, area: Rect, buf: &mut Buffer);
}

