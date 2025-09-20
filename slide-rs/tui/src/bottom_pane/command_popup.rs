use ratatui::{buffer::Buffer, layout::Rect, widgets::WidgetRef};

use super::{
    popup_consts::MAX_POPUP_ROWS,
    scroll_state::ScrollState,
    selection_popup_common::{render_rows, GenericDisplayRow},
};

// 依存の簡易スタブ
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlashCommand {
    Init,
    Compact,
    Mention,
}
impl SlashCommand {
    pub fn command(&self) -> &'static str {
        match self {
            SlashCommand::Init => "init",
            SlashCommand::Compact => "compact",
            SlashCommand::Mention => "mention",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            SlashCommand::Init => "initialize",
            SlashCommand::Compact => "compact transcript",
            SlashCommand::Mention => "insert mention",
        }
    }
}
pub(crate) fn built_in_slash_commands() -> Vec<(&'static str, SlashCommand)> {
    vec![
        ("init", SlashCommand::Init),
        ("compact", SlashCommand::Compact),
        ("mention", SlashCommand::Mention),
    ]
}

#[derive(Clone, Debug)]
pub(crate) struct CustomPrompt {
    pub name: String,
    pub content: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandItem {
    Builtin(SlashCommand),
    UserPrompt(usize),
}

pub(crate) struct CommandPopup {
    command_filter: String,
    builtins: Vec<(&'static str, SlashCommand)>,
    prompts: Vec<CustomPrompt>,
    state: ScrollState,
}

impl CommandPopup {
    pub(crate) fn new(prompts: Vec<CustomPrompt>) -> Self {
        Self {
            command_filter: String::new(),
            builtins: built_in_slash_commands(),
            prompts,
            state: ScrollState::new(),
        }
    }
    pub(crate) fn set_prompts(&mut self, prompts: Vec<CustomPrompt>) {
        self.prompts = prompts;
    }
    pub(crate) fn prompt_name(&self, idx: usize) -> Option<&str> {
        self.prompts.get(idx).map(|p| p.name.as_str())
    }
    pub(crate) fn prompt_content(&self, idx: usize) -> Option<&str> {
        self.prompts.get(idx).map(|p| p.content.as_str())
    }
    pub(crate) fn on_composer_text_change(&mut self, text: String) {
        let first = text.lines().next().unwrap_or("");
        if let Some(stripped) = first.strip_prefix('/') {
            let token = stripped.trim_start();
            let cmd_token = token.split_whitespace().next().unwrap_or("");
            self.command_filter = cmd_token.to_string();
        } else {
            self.command_filter.clear();
        }
        let len = self.filtered_items().len();
        self.state.clamp_selection(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }
    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.filtered_items().len().clamp(1, MAX_POPUP_ROWS) as u16
    }
    fn filtered(&self) -> Vec<(CommandItem, Option<Vec<usize>>, i32)> {
        let filter = self.command_filter.trim().to_ascii_lowercase();
        let mut out = Vec::new();
        if filter.is_empty() {
            for (_, cmd) in &self.builtins {
                out.push((CommandItem::Builtin(*cmd), None, 0));
            }
            for i in 0..self.prompts.len() {
                out.push((CommandItem::UserPrompt(i), None, 0));
            }
            return out;
        }
        // 簡易: 前方一致 + 太字位置を 0..filter.len
        for (_, cmd) in &self.builtins {
            let name = cmd.command();
            if name.starts_with(&filter) {
                out.push((
                    CommandItem::Builtin(*cmd),
                    Some((0..filter.len()).collect()),
                    0,
                ));
            }
        }
        for (i, p) in self.prompts.iter().enumerate() {
            let n = p.name.to_ascii_lowercase();
            if n.starts_with(&filter) {
                out.push((
                    CommandItem::UserPrompt(i),
                    Some((0..filter.len()).collect()),
                    0,
                ));
            }
        }
        out
    }
    fn filtered_items(&self) -> Vec<CommandItem> {
        self.filtered().into_iter().map(|(c, _, _)| c).collect()
    }
    pub(crate) fn move_up(&mut self) {
        let len = self.filtered_items().len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }
    pub(crate) fn move_down(&mut self) {
        let len = self.filtered_items().len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }
    pub(crate) fn selected_item(&self) -> Option<CommandItem> {
        let m = self.filtered_items();
        self.state.selected_idx.and_then(|i| m.get(i).copied())
    }
}

impl WidgetRef for CommandPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let matches = self.filtered();
        let rows_all: Vec<GenericDisplayRow> = if matches.is_empty() {
            Vec::new()
        } else {
            matches
                .into_iter()
                .map(|(item, indices, _)| match item {
                    CommandItem::Builtin(cmd) => GenericDisplayRow {
                        name: format!("/{}", cmd.command()),
                        match_indices: indices.map(|v| v.into_iter().map(|i| i + 1).collect()),
                        is_current: false,
                        description: Some(cmd.description().to_string()),
                    },
                    CommandItem::UserPrompt(i) => GenericDisplayRow {
                        name: format!("/{}", self.prompts[i].name),
                        match_indices: indices.map(|v| v.into_iter().map(|i| i + 1).collect()),
                        is_current: false,
                        description: Some("send saved prompt".to_string()),
                    },
                })
                .collect()
        };
        render_rows(area, buf, &rows_all, &self.state, MAX_POPUP_ROWS, false);
    }
}
