use ratatui::{buffer::Buffer, layout::Rect, widgets::WidgetRef};

use super::{
    popup_consts::MAX_POPUP_ROWS,
    scroll_state::ScrollState,
    selection_popup_common::{render_rows, GenericDisplayRow},
};

#[derive(Clone, Debug)]
pub struct FileMatch {
    pub path: String,
    pub indices: Option<Vec<usize>>,
}

/// ファイル検索ポップアップ（簡易）
pub(crate) struct FileSearchPopup {
    display_query: String,
    pending_query: String,
    waiting: bool,
    matches: Vec<FileMatch>,
    state: ScrollState,
}

impl FileSearchPopup {
    pub(crate) fn new() -> Self {
        Self {
            display_query: String::new(),
            pending_query: String::new(),
            waiting: true,
            matches: Vec::new(),
            state: ScrollState::new(),
        }
    }
    pub(crate) fn set_query(&mut self, query: &str) {
        if query == self.pending_query {
            return;
        }
        let keep_existing = query.starts_with(&self.display_query);
        self.pending_query = query.to_string();
        self.waiting = true;
        if !keep_existing {
            self.matches.clear();
            self.state.reset();
        }
    }
    pub(crate) fn set_empty_prompt(&mut self) {
        self.display_query.clear();
        self.pending_query.clear();
        self.waiting = false;
        self.matches.clear();
        self.state.reset();
    }
    pub(crate) fn set_matches(&mut self, query: &str, matches: Vec<FileMatch>) {
        if query != self.pending_query {
            return;
        }
        self.display_query = query.to_string();
        self.matches = matches;
        self.waiting = false;
        let len = self.matches.len();
        self.state.clamp_selection(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }
    pub(crate) fn move_up(&mut self) {
        let len = self.matches.len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }
    pub(crate) fn move_down(&mut self) {
        let len = self.matches.len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }
    pub(crate) fn selected_match(&self) -> Option<&str> {
        self.state
            .selected_idx
            .and_then(|i| self.matches.get(i))
            .map(|m| m.path.as_str())
    }
    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.matches.len().clamp(1, MAX_POPUP_ROWS) as u16
    }
}

impl WidgetRef for FileSearchPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let rows_all: Vec<GenericDisplayRow> = if self.matches.is_empty() {
            Vec::new()
        } else {
            self.matches
                .iter()
                .map(|m| GenericDisplayRow {
                    name: m.path.clone(),
                    match_indices: m.indices.clone(),
                    is_current: false,
                    description: None,
                })
                .collect()
        };
        if self.waiting && rows_all.is_empty() {
            render_rows(area, buf, &[], &self.state, MAX_POPUP_ROWS, false);
        } else {
            render_rows(area, buf, &rows_all, &self.state, MAX_POPUP_ROWS, false);
        }
    }
}
