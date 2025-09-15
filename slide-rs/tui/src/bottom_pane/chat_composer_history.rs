use std::collections::HashMap;
use crate::history_store::HistoryStore;

/// シェル風の履歴ナビゲーションを扱う簡易実装
pub(crate) struct ChatComposerHistory {
    history_log_id: Option<u64>,
    history_entry_count: usize,
    local_history: Vec<String>,
    fetched_history: HashMap<usize, String>,
    history_cursor: Option<isize>,
    last_history_text: Option<String>,
    store: HistoryStore,
}

impl ChatComposerHistory {
    pub fn new() -> Self {
        let store = HistoryStore::default();
        let (log_id, count) = store.metadata();
        Self {
            history_log_id: None,
            history_entry_count: 0,
            local_history: Vec::new(),
            fetched_history: HashMap::new(),
            history_cursor: None,
            last_history_text: None,
            store,
        }
        .with_metadata(log_id, count)
    }

    fn with_metadata(mut self, log_id: u64, count: usize) -> Self {
        if count > 0 {
            self.history_log_id = Some(log_id);
            self.history_entry_count = count;
        }
        self
    }

    pub fn set_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.history_log_id = Some(log_id);
        self.history_entry_count = entry_count;
        self.fetched_history.clear();
        self.local_history.clear();
        self.history_cursor = None;
        self.last_history_text = None;
    }

    pub fn record_local_submission(&mut self, text: &str) {
        if text.is_empty() { return; }
        if self.local_history.last().is_some_and(|p| p == text) { return; }
        // Best-effort: append to persistent store (ignore errors)
        let _ = self.store.append(text);
        // local echo for this UI session
        self.local_history.push(text.to_string());
        self.history_cursor = None;
        self.last_history_text = None;
    }

    pub fn should_handle_navigation(&self, text: &str, cursor: usize) -> bool {
        if self.history_entry_count == 0 && self.local_history.is_empty() { return false; }
        if text.is_empty() { return true; }
        if cursor != 0 { return false; }
        matches!(&self.last_history_text, Some(prev) if prev == text)
    }

    pub fn navigate_up(&mut self) -> Option<String> {
        let total = self.history_entry_count + self.local_history.len();
        if total == 0 { return None; }
        let next = match self.history_cursor { None => (total as isize) - 1, Some(0) => return None, Some(i) => i - 1 };
        self.history_cursor = Some(next);
        self.get_by_index(next as usize)
    }

    pub fn navigate_down(&mut self) -> Option<String> {
        let total = self.history_entry_count + self.local_history.len();
        if total == 0 { return None; }
        let next = match self.history_cursor { None => return None, Some(i) if (i as usize) + 1 >= total => return Some(String::new()), Some(i) => i + 1 };
        self.history_cursor = Some(next);
        self.get_by_index(next as usize)
    }

    fn get_by_index(&mut self, idx: usize) -> Option<String> {
        if idx >= self.history_entry_count {
            let local_idx = idx - self.history_entry_count;
            let text = self.local_history.get(local_idx)?.clone();
            self.last_history_text = Some(text.clone());
            Some(text)
        } else {
            if let Some(text) = self.fetched_history.get(&idx).cloned() {
                self.last_history_text = Some(text.clone());
                Some(text)
            } else {
                // lazy fetch from persistent store
                if let Some(id) = self.history_log_id {
                    if let Some(text) = self.store.lookup(id, idx) {
                        self.fetched_history.insert(idx, text.clone());
                        self.last_history_text = Some(text.clone());
                        return Some(text);
                    }
                }
                None
            }
        }
    }

    pub fn on_entry_response(&mut self, log_id: u64, offset: usize, entry: Option<String>) -> Option<String> {
        if self.history_log_id != Some(log_id) { return None; }
        let text = entry?;
        self.fetched_history.insert(offset, text.clone());
        if self.history_cursor == Some(offset as isize) {
            self.last_history_text = Some(text.clone());
            return Some(text);
        }
        None
    }
}
