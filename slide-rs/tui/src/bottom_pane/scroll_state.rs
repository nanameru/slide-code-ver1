#[derive(Clone, Debug, Default)]
pub(crate) struct ScrollState {
    pub selected_idx: Option<usize>,
    pub scroll_top: usize,
}

impl ScrollState {
    pub fn new() -> Self { Self { selected_idx: None, scroll_top: 0 } }
    pub fn reset(&mut self) { self.selected_idx = None; self.scroll_top = 0; }
    pub fn clamp_selection(&mut self, len: usize) {
        if len == 0 { self.selected_idx = None; return; }
        if let Some(i) = self.selected_idx { if i >= len { self.selected_idx = Some(len - 1); } }
        else { self.selected_idx = Some(0); }
    }
    pub fn move_up_wrap(&mut self, len: usize) {
        if len == 0 { self.selected_idx=None; return; }
        let cur = self.selected_idx.unwrap_or(0);
        self.selected_idx = Some(if cur == 0 { len - 1 } else { cur - 1 });
    }
    pub fn move_down_wrap(&mut self, len: usize) {
        if len == 0 { self.selected_idx=None; return; }
        let cur = self.selected_idx.unwrap_or(0);
        self.selected_idx = Some(if cur + 1 >= len { 0 } else { cur + 1 });
    }
    pub fn ensure_visible(&mut self, len: usize, window: usize) {
        if len == 0 || window == 0 { return; }
        let sel = self.selected_idx.unwrap_or(0);
        if sel < self.scroll_top { self.scroll_top = sel; }
        else {
            let bottom = self.scroll_top + window - 1;
            if sel > bottom { self.scroll_top = sel + 1 - window; }
        }
        if self.scroll_top + window > len { if len >= window { self.scroll_top = len - window; } else { self.scroll_top = 0; } }
    }
}

