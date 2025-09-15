/// Generic scroll/selection state for a vertical list menu.
///
/// Encapsulates the common behavior of a selectable list that supports:
/// - Optional selection (None when list is empty)
/// - Wrap-around navigation on Up/Down
/// - Maintaining a scroll window (`scroll_top`) so the selected row stays visible
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ScrollState {
    pub selected_idx: Option<usize>,
    pub scroll_top: usize,
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            selected_idx: None,
            scroll_top: 0,
        }
    }

    /// Reset selection and scroll.
    pub fn reset(&mut self) {
        self.selected_idx = None;
        self.scroll_top = 0;
    }

    /// Clamp selection to be within the [0, len-1] range, or None when empty.
    pub fn clamp_selection(&mut self, len: usize) {
        self.selected_idx = match len {
            0 => None,
            _ => Some(self.selected_idx.unwrap_or(0).min(len - 1)),
        };
        if len == 0 {
            self.scroll_top = 0;
        }
    }

    /// Move selection up by one, wrapping to the bottom when necessary.
    pub fn move_up_wrap(&mut self, len: usize) {
        if len == 0 {
            self.selected_idx = None;
            self.scroll_top = 0;
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) if idx > 0 => idx - 1,
            Some(_) => len - 1,
            None => 0,
        });
    }

    /// Move selection down by one, wrapping to the top when necessary.
    pub fn move_down_wrap(&mut self, len: usize) {
        if len == 0 {
            self.selected_idx = None;
            self.scroll_top = 0;
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) if idx + 1 < len => idx + 1,
            _ => 0,
        });
    }

    /// Adjust `scroll_top` so that the current `selected_idx` is visible within
    /// the window of `visible_rows`.
    pub fn ensure_visible(&mut self, len: usize, visible_rows: usize) {
        if len == 0 || visible_rows == 0 {
            self.scroll_top = 0;
            return;
        }
        if let Some(sel) = self.selected_idx {
            if sel < self.scroll_top {
                self.scroll_top = sel;
            } else {
                let bottom = self.scroll_top + visible_rows - 1;
                if sel > bottom {
                    self.scroll_top = sel + 1 - visible_rows;
                }
            }
        } else {
            self.scroll_top = 0;
        }
    }

    /// Move selection to a specific index without wrapping.
    pub fn set_selected(&mut self, idx: Option<usize>) {
        self.selected_idx = idx;
    }

    /// Get the currently selected index.
    pub fn selected(&self) -> Option<usize> {
        self.selected_idx
    }

    /// Get the current scroll position.
    pub fn scroll_position(&self) -> usize {
        self.scroll_top
    }

    /// Set the scroll position directly.
    pub fn set_scroll_position(&mut self, pos: usize) {
        self.scroll_top = pos;
    }

    /// Move selection up by one without wrapping.
    pub fn move_up(&mut self, len: usize) {
        if len == 0 {
            self.selected_idx = None;
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) if idx > 0 => idx - 1,
            Some(idx) => idx, // Stay at 0
            None => 0,
        });
    }

    /// Move selection down by one without wrapping.
    pub fn move_down(&mut self, len: usize) {
        if len == 0 {
            self.selected_idx = None;
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) if idx + 1 < len => idx + 1,
            Some(idx) => idx, // Stay at last index
            None => 0,
        });
    }

    /// Move selection to the first item.
    pub fn move_to_first(&mut self, len: usize) {
        if len > 0 {
            self.selected_idx = Some(0);
        } else {
            self.selected_idx = None;
        }
    }

    /// Move selection to the last item.
    pub fn move_to_last(&mut self, len: usize) {
        if len > 0 {
            self.selected_idx = Some(len - 1);
        } else {
            self.selected_idx = None;
        }
    }

    /// Page up by `page_size` items.
    pub fn page_up(&mut self, len: usize, page_size: usize) {
        if len == 0 {
            self.selected_idx = None;
            return;
        }
        let current = self.selected_idx.unwrap_or(0);
        self.selected_idx = Some(current.saturating_sub(page_size));
    }

    /// Page down by `page_size` items.
    pub fn page_down(&mut self, len: usize, page_size: usize) {
        if len == 0 {
            self.selected_idx = None;
            return;
        }
        let current = self.selected_idx.unwrap_or(0);
        self.selected_idx = Some((current + page_size).min(len - 1));
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollState;

    #[test]
    fn test_new() {
        let state = ScrollState::new();
        assert_eq!(state.selected_idx, None);
        assert_eq!(state.scroll_top, 0);
    }

    #[test]
    fn test_reset() {
        let mut state = ScrollState {
            selected_idx: Some(5),
            scroll_top: 10,
        };
        state.reset();
        assert_eq!(state.selected_idx, None);
        assert_eq!(state.scroll_top, 0);
    }

    #[test]
    fn test_clamp_selection() {
        let mut state = ScrollState::new();

        // Empty list
        state.clamp_selection(0);
        assert_eq!(state.selected_idx, None);

        // Non-empty list with no selection
        state.clamp_selection(5);
        assert_eq!(state.selected_idx, Some(0));

        // Selection within bounds
        state.selected_idx = Some(3);
        state.clamp_selection(5);
        assert_eq!(state.selected_idx, Some(3));

        // Selection out of bounds
        state.selected_idx = Some(10);
        state.clamp_selection(5);
        assert_eq!(state.selected_idx, Some(4));
    }

    #[test]
    fn test_wrap_navigation() {
        let mut state = ScrollState::new();
        let len = 5;

        // Start with no selection
        state.move_down_wrap(len);
        assert_eq!(state.selected_idx, Some(0));

        // Move down to 1
        state.move_down_wrap(len);
        assert_eq!(state.selected_idx, Some(1));

        // Move up to 0
        state.move_up_wrap(len);
        assert_eq!(state.selected_idx, Some(0));

        // Wrap to end
        state.move_up_wrap(len);
        assert_eq!(state.selected_idx, Some(4));

        // Wrap to beginning
        state.move_down_wrap(len);
        assert_eq!(state.selected_idx, Some(0));
    }

    #[test]
    fn test_non_wrap_navigation() {
        let mut state = ScrollState::new();
        let len = 5;

        // Start with no selection
        state.move_down(len);
        assert_eq!(state.selected_idx, Some(0));

        // Move down to 1
        state.move_down(len);
        assert_eq!(state.selected_idx, Some(1));

        // Move up to 0
        state.move_up(len);
        assert_eq!(state.selected_idx, Some(0));

        // Try to move up beyond beginning (should stay at 0)
        state.move_up(len);
        assert_eq!(state.selected_idx, Some(0));

        // Move to end
        state.move_to_last(len);
        assert_eq!(state.selected_idx, Some(4));

        // Try to move down beyond end (should stay at 4)
        state.move_down(len);
        assert_eq!(state.selected_idx, Some(4));
    }

    #[test]
    fn test_ensure_visible() {
        let mut state = ScrollState::new();
        let len = 10;
        let visible = 3;

        // Select item 0, should not scroll
        state.selected_idx = Some(0);
        state.ensure_visible(len, visible);
        assert_eq!(state.scroll_top, 0);

        // Select item 5, should scroll to show it
        state.selected_idx = Some(5);
        state.ensure_visible(len, visible);
        assert_eq!(state.scroll_top, 3); // 5 - 3 + 1 = 3

        // Select item 2, should scroll back
        state.selected_idx = Some(2);
        state.ensure_visible(len, visible);
        assert_eq!(state.scroll_top, 2);

        // Select item within visible range, should not change scroll
        state.selected_idx = Some(3);
        state.ensure_visible(len, visible);
        assert_eq!(state.scroll_top, 2);
    }

    #[test]
    fn test_paging() {
        let mut state = ScrollState::new();
        let len = 20;
        let page_size = 5;

        // Start at 0
        state.selected_idx = Some(0);

        // Page down
        state.page_down(len, page_size);
        assert_eq!(state.selected_idx, Some(5));

        // Page down again
        state.page_down(len, page_size);
        assert_eq!(state.selected_idx, Some(10));

        // Page up
        state.page_up(len, page_size);
        assert_eq!(state.selected_idx, Some(5));

        // Page down near end (should clamp)
        state.selected_idx = Some(18);
        state.page_down(len, page_size);
        assert_eq!(state.selected_idx, Some(19));

        // Page up at beginning (should clamp)
        state.selected_idx = Some(2);
        state.page_up(len, page_size);
        assert_eq!(state.selected_idx, Some(0));
    }

    #[test]
    fn test_first_last_navigation() {
        let mut state = ScrollState::new();
        let len = 10;

        state.move_to_first(len);
        assert_eq!(state.selected_idx, Some(0));

        state.move_to_last(len);
        assert_eq!(state.selected_idx, Some(9));

        // Empty list
        state.move_to_first(0);
        assert_eq!(state.selected_idx, None);

        state.move_to_last(0);
        assert_eq!(state.selected_idx, None);
    }

    #[test]
    fn test_accessors() {
        let mut state = ScrollState::new();

        assert_eq!(state.selected(), None);
        assert_eq!(state.scroll_position(), 0);

        state.set_selected(Some(5));
        state.set_scroll_position(10);

        assert_eq!(state.selected(), Some(5));
        assert_eq!(state.scroll_position(), 10);
    }

    #[test]
    fn test_edge_cases() {
        let mut state = ScrollState::new();

        // Operations on empty list
        state.move_up_wrap(0);
        assert_eq!(state.selected_idx, None);
        assert_eq!(state.scroll_top, 0);

        state.move_down_wrap(0);
        assert_eq!(state.selected_idx, None);
        assert_eq!(state.scroll_top, 0);

        state.ensure_visible(0, 5);
        assert_eq!(state.scroll_top, 0);

        // Operations with zero visible rows
        state.selected_idx = Some(5);
        state.ensure_visible(10, 0);
        assert_eq!(state.scroll_top, 0);
    }

    #[test]
    fn wrap_navigation_and_visibility() {
        let mut s = ScrollState::new();
        let len = 10;
        let vis = 5;

        s.clamp_selection(len);
        assert_eq!(s.selected_idx, Some(0));
        s.ensure_visible(len, vis);
        assert_eq!(s.scroll_top, 0);

        s.move_up_wrap(len);
        s.ensure_visible(len, vis);
        assert_eq!(s.selected_idx, Some(len - 1));
        match s.selected_idx {
            Some(sel) => assert!(s.scroll_top <= sel),
            None => panic!("expected Some(selected_idx) after wrap"),
        }

        s.move_down_wrap(len);
        s.ensure_visible(len, vis);
        assert_eq!(s.selected_idx, Some(0));
        assert_eq!(s.scroll_top, 0);
    }
}