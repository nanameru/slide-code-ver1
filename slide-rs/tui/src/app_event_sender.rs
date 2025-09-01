#[derive(Clone, Default)]
pub struct AppEventSender;

impl AppEventSender {
    pub fn new<T>(_tx: T) -> Self {
        // 実装のないダミー送信機。将来、アプリ内イベントへ接続。
        Self
    }
    pub fn send<E>(&self, _event: E) {
        // no-op
    }
}

