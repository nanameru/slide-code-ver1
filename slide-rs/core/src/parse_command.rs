use serde::Deserialize;
use serde::Serialize;
use shlex::split as shlex_split;
use shlex::try_join as shlex_try_join;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum ParsedCommand {
    Read {
        cmd: String,
        name: String,
    },
    ListFiles {
        cmd: String,
        path: Option<String>,
    },
    Search {
        cmd: String,
        query: Option<String>,
        path: Option<String>,
    },
    Format {
        cmd: String,
        tool: Option<String>,
        targets: Option<Vec<String>>,
    },
    Test {
        cmd: String,
    },
    Lint {
        cmd: String,
        tool: Option<String>,
        targets: Option<Vec<String>>,
    },
    Noop {
        cmd: String,
    },
    Unknown {
        cmd: String,
    },
}

fn shlex_join(tokens: &[String]) -> String {
    shlex_try_join(tokens.iter().map(|s| s.as_str()))
        .unwrap_or_else(|_| "<command included NUL byte>".to_string())
}

/// Parses metadata out of an arbitrary command.
/// These commands are model driven and could include just about anything.
/// The parsing is slightly lossy due to the ~infinite expressiveness of an arbitrary command.
/// The goal of the parsed metadata is to be able to provide the user with a human readable gist
/// of what it is doing.
pub fn parse_command(command: &[String]) -> Vec<ParsedCommand> {
    // Parse and then collapse consecutive duplicate commands to avoid redundant summaries.
    let parsed = parse_command_impl(command);
    let mut deduped: Vec<ParsedCommand> = Vec::with_capacity(parsed.len());

    for cmd in parsed {
        if let Some(last) = deduped.last() {
            if std::mem::discriminant(last) == std::mem::discriminant(&cmd) {
                continue; // Skip duplicate command types
            }
        }
        deduped.push(cmd);
    }

    deduped
}

fn parse_command_impl(command: &[String]) -> Vec<ParsedCommand> {
    if command.is_empty() {
        return vec![ParsedCommand::Noop {
            cmd: "".to_string(),
        }];
    }

    let joined = shlex_join(command);

    // Check for `bash -lc "..."` pattern without using unstable slice patterns.
    if command.len() == 3 && command[0] == "bash" && command[1] == "-lc" {
        if let Some(inner_commands) = shlex_split(&command[2]) {
            return parse_command_impl(&inner_commands);
        }
    }

    let first_arg = &command[0];
    let cmd = joined.clone();

    match first_arg.as_str() {
        // File reading commands
        "cat" | "head" | "tail" | "less" | "more" => {
            if command.len() > 1 {
                ParsedCommand::Read {
                    cmd,
                    name: command[1].clone(),
                }
            } else {
                ParsedCommand::Unknown { cmd }
            }
        }

        // File listing commands
        "ls" | "find" | "tree" => {
            let path = if command.len() > 1 {
                Some(command[1].clone())
            } else {
                None
            };
            ParsedCommand::ListFiles { cmd, path }
        }

        // Search commands
        "grep" | "rg" | "ag" | "ack" => {
            let query = if command.len() > 1 {
                Some(command[1].clone())
            } else {
                None
            };
            let path = if command.len() > 2 {
                Some(command[2].clone())
            } else {
                None
            };
            ParsedCommand::Search { cmd, query, path }
        }

        // Format commands
        "rustfmt" | "black" | "prettier" | "gofmt" => {
            let tool = Some(first_arg.clone());
            let targets = if command.len() > 1 {
                Some(command[1..].to_vec())
            } else {
                None
            };
            ParsedCommand::Format { cmd, tool, targets }
        }

        // Test commands
        "cargo" if command.len() > 1 && command[1] == "test" => ParsedCommand::Test { cmd },
        "npm" | "yarn" if command.len() > 1 && command[1] == "test" => ParsedCommand::Test { cmd },
        "pytest" | "jest" => ParsedCommand::Test { cmd },

        // Lint commands
        "clippy" | "eslint" | "pylint" | "flake8" => {
            let tool = Some(first_arg.clone());
            let targets = if command.len() > 1 {
                Some(command[1..].to_vec())
            } else {
                None
            };
            ParsedCommand::Lint { cmd, tool, targets }
        }
        "cargo" if command.len() > 1 && command[1] == "clippy" => {
            let tool = Some("clippy".to_string());
            let targets = if command.len() > 2 {
                Some(command[2..].to_vec())
            } else {
                None
            };
            ParsedCommand::Lint { cmd, tool, targets }
        }

        // No-op commands
        "true" | ":" => ParsedCommand::Noop { cmd },

        // Everything else
        _ => ParsedCommand::Unknown { cmd },
    }
    .into()
}

impl From<ParsedCommand> for Vec<ParsedCommand> {
    fn from(cmd: ParsedCommand) -> Self {
        vec![cmd]
    }
}

/// Legacy function - シンプルなコマンド文字列パーサー
/// スペース区切りでコマンドを分割しますが、引用符内のスペースは保持します
pub fn parse_command_string(cmd: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = cmd.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            '\\' if in_quotes => {
                // エスケープ文字の処理
                if let Some(next_ch) = chars.next() {
                    match next_ch {
                        '"' => current.push('"'),
                        '\\' => current.push('\\'),
                        'n' => current.push('\n'),
                        't' => current.push('\t'),
                        _ => {
                            current.push('\\');
                            current.push(next_ch);
                        }
                    }
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Search,
    List,
    Format,
    Test,
    Lint,
    Unknown,
}

pub fn summarize(cmd: &str) -> CommandKind {
    if cmd.contains("rg ") || cmd.contains("grep ") {
        CommandKind::Search
    } else if cmd.starts_with("ls") {
        CommandKind::List
    } else {
        CommandKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cat_command() {
        let command = vec!["cat".to_string(), "file.txt".to_string()];
        let result = parse_command(&command);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ParsedCommand::Read { cmd: _, name } => {
                assert_eq!(name, "file.txt");
            }
            _ => panic!("Expected Read command"),
        }
    }

    #[test]
    fn test_parse_ls_command() {
        let command = vec!["ls".to_string(), "/tmp".to_string()];
        let result = parse_command(&command);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ParsedCommand::ListFiles { cmd: _, path } => {
                assert_eq!(path, &Some("/tmp".to_string()));
            }
            _ => panic!("Expected ListFiles command"),
        }
    }

    #[test]
    fn test_parse_grep_command() {
        let command = vec![
            "grep".to_string(),
            "pattern".to_string(),
            "file.txt".to_string(),
        ];
        let result = parse_command(&command);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ParsedCommand::Search {
                cmd: _,
                query,
                path,
            } => {
                assert_eq!(query, &Some("pattern".to_string()));
                assert_eq!(path, &Some("file.txt".to_string()));
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_parse_bash_lc_command() {
        let command = vec!["bash".to_string(), "-lc".to_string(), "ls /tmp".to_string()];
        let result = parse_command(&command);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ParsedCommand::ListFiles { cmd: _, path } => {
                assert_eq!(path, &Some("/tmp".to_string()));
            }
            _ => panic!("Expected ListFiles command from bash -lc"),
        }
    }

    #[test]
    fn test_parse_test_command() {
        let command = vec!["cargo".to_string(), "test".to_string()];
        let result = parse_command(&command);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ParsedCommand::Test { cmd: _ } => {}
            _ => panic!("Expected Test command"),
        }
    }

    #[test]
    fn test_parse_unknown_command() {
        let command = vec!["unknown_command".to_string()];
        let result = parse_command(&command);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ParsedCommand::Unknown { cmd: _ } => {}
            _ => panic!("Expected Unknown command"),
        }
    }

    // Legacy function tests
    #[test]
    fn test_simple_command_string() {
        assert_eq!(parse_command_string("ls -la"), vec!["ls", "-la"]);
    }

    #[test]
    fn test_quoted_arguments() {
        assert_eq!(
            parse_command_string("echo \"hello world\""),
            vec!["echo", "hello world"]
        );
    }

    #[test]
    fn test_mixed_quotes() {
        assert_eq!(
            parse_command_string("grep \"search term\" file.txt"),
            vec!["grep", "search term", "file.txt"]
        );
    }

    #[test]
    fn test_escaped_quotes() {
        assert_eq!(
            parse_command_string("echo \"He said \\\"hello\\\"\""),
            vec!["echo", "He said \"hello\""]
        );
    }

    #[test]
    fn test_empty_command_string() {
        assert_eq!(parse_command_string(""), Vec::<String>::new());
    }

    #[test]
    fn test_extra_spaces() {
        assert_eq!(parse_command_string("  ls   -la  "), vec!["ls", "-la"]);
    }
}
