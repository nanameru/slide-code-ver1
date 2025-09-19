/// シンプルなコマンド文字列パーサー
/// スペース区切りでコマンドを分割しますが、引用符内のスペースは保持します
pub fn parse_command(cmd: &str) -> Vec<String> {
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
pub enum CommandKind { Search, List, Format, Test, Lint, Unknown }

pub fn summarize(cmd: &str) -> CommandKind {
    if cmd.contains("rg ") || cmd.contains("grep ") { CommandKind::Search }
    else if cmd.starts_with("ls") { CommandKind::List }
    else { CommandKind::Unknown }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        assert_eq!(parse_command("ls -la"), vec!["ls", "-la"]);
    }

    #[test]
    fn test_quoted_arguments() {
        assert_eq!(
            parse_command("echo \"hello world\""),
            vec!["echo", "hello world"]
        );
    }

    #[test]
    fn test_mixed_quotes() {
        assert_eq!(
            parse_command("grep \"search term\" file.txt"),
            vec!["grep", "search term", "file.txt"]
        );
    }

    #[test]
    fn test_escaped_quotes() {
        assert_eq!(
            parse_command("echo \"He said \\\"hello\\\"\""),
            vec!["echo", "He said \"hello\""]
        );
    }

    #[test]
    fn test_empty_command() {
        assert_eq!(parse_command(""), Vec::<String>::new());
    }

    #[test]
    fn test_extra_spaces() {
        assert_eq!(
            parse_command("  ls   -la  "),
            vec!["ls", "-la"]
        );
    }
}

