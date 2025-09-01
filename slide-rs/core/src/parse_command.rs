#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind { Search, List, Format, Test, Lint, Unknown }

pub fn summarize(cmd: &str) -> CommandKind {
    if cmd.contains("rg ") || cmd.contains("grep ") { CommandKind::Search }
    else if cmd.starts_with("ls") { CommandKind::List }
    else { CommandKind::Unknown }
}

