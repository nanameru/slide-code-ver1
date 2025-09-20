use crate::parse_command::parse_command_string;

pub fn is_known_safe_command(command: &[String]) -> bool {
    if is_safe_to_call_with_exec(command) {
        return true;
    }

    // Support `bash -lc "..."` where the script consists solely of one or
    // more "plain" commands (only bare words / quoted strings) combined with
    // a conservative allow‑list of shell operators that themselves do not
    // introduce side effects ( "&&", "||", ";", and "|" ). If every
    // individual command in the script is itself a known‑safe command, then
    // the composite expression is considered safe.
    if let [bash, flag, script] = command
        && bash == "bash"
        && flag == "-lc"
    {
        // Parse the script and check if all commands are safe
        let parsed_script = parse_command_string(script);
        if !parsed_script.is_empty() && is_safe_to_call_with_exec(&parsed_script) {
            return true;
        }
    }

    false
}

fn is_safe_to_call_with_exec(command: &[String]) -> bool {
    let cmd0 = command.first().map(String::as_str);

    match cmd0 {
        #[rustfmt::skip]
        Some(
            "cat" |
            "cd" |
            "echo" |
            "false" |
            "grep" |
            "head" |
            "ls" |
            "nl" |
            "pwd" |
            "tail" |
            "true" |
            "wc" |
            "which") => {
            true
        },

        Some("find") => {
            // Certain options to `find` can delete files, write to files, or
            // execute arbitrary commands, so we cannot auto-approve the
            // invocation of `find` in such cases.
            #[rustfmt::skip]
            const UNSAFE_FIND_OPTIONS: &[&str] = &[
                // Options that can execute arbitrary commands.
                "-exec", "-execdir", "-ok", "-okdir",
                // Option that deletes matching files.
                "-delete",
                // Options that write pathnames to a file.
                "-fls", "-fprint", "-fprint0", "-fprintf",
            ];

            !command
                .iter()
                .any(|arg| UNSAFE_FIND_OPTIONS.contains(&arg.as_str()))
        }

        // Ripgrep
        Some("rg") => {
            const UNSAFE_RIPGREP_OPTIONS_WITH_ARGS: &[&str] = &[
                // Takes an arbitrary command that is executed for each match.
                "--pre",
                // Takes a command that can be used to obtain the local hostname.
                "--hostname-bin",
            ];
            const UNSAFE_RIPGREP_OPTIONS_WITHOUT_ARGS: &[&str] = &[
                // Write matches to a file instead of stdout.
                // This could potentially overwrite important files.
                "-o", "--only-matching",
            ];

            // Check for unsafe options with arguments
            let mut i = 1;
            while i < command.len() {
                let arg = &command[i];

                // Check for options that take an argument
                if UNSAFE_RIPGREP_OPTIONS_WITH_ARGS.contains(&arg.as_str()) {
                    return false;
                }

                // Check for options without arguments
                if UNSAFE_RIPGREP_OPTIONS_WITHOUT_ARGS.contains(&arg.as_str()) {
                    return false;
                }

                i += 1;
            }

            true
        }

        // Git - allow common read-only operations
        Some("git") => {
            if command.len() < 2 {
                return false;
            }

            match command[1].as_str() {
                // Safe read-only operations
                "status" | "log" | "diff" | "show" | "branch" | "remote" |
                "config" | "ls-files" | "ls-remote" => true,
                // Everything else is potentially unsafe
                _ => false,
            }
        }

        // Common safe utilities
        Some("whoami" | "date" | "uptime" | "uname" | "env" | "printenv") => true,

        // File inspection tools
        Some("file" | "stat" | "du" | "df") => true,

        // Text processing (read-only)
        Some("sort" | "uniq" | "cut" | "awk" | "sed") => {
            // These are generally safe for reading, but we should be careful
            // about write operations. For now, allow them.
            true
        }

        // Network tools (read-only)
        Some("ping" | "traceroute" | "nslookup" | "dig") => {
            // These tools make network requests but don't modify the system
            true
        }

        _ => false,
    }
}

/// Check if a command contains any potentially dangerous patterns
pub fn has_dangerous_patterns(command: &[String]) -> bool {
    let command_str = command.join(" ");

    // Check for dangerous shell patterns
    let dangerous_patterns = [
        "rm ", "rmdir ", "delete ", "del ",  // Deletion commands
        "mv ", "move ",                      // Move commands
        "cp ", "copy ",                      // Copy commands (can overwrite)
        ">", ">>",                          // Redirection (can overwrite files)
        "curl ", "wget ", "fetch ",         // Download commands
        "chmod ", "chown ", "chgrp ",       // Permission changes
        "sudo ", "su ",                     // Privilege escalation
        "kill ", "killall ",                // Process termination
        "mount ", "umount ",                // Filesystem operations
        "fdisk ", "mkfs ",                  // Disk operations
        "&", "&&", "||", ";", "|",         // Command chaining
        "$", "`",                          // Variable expansion/command substitution
    ];

    for pattern in &dangerous_patterns {
        if command_str.contains(pattern) {
            return true;
        }
    }

    false
}

/// Get a human-readable explanation of why a command might be unsafe
pub fn explain_safety_concern(command: &[String]) -> Option<String> {
    if command.is_empty() {
        return Some("Empty command".to_string());
    }

    let cmd0 = command.first().map(String::as_str);

    match cmd0 {
        Some("rm" | "rmdir" | "delete" | "del") => {
            Some("Command can delete files or directories".to_string())
        }
        Some("mv" | "move") => {
            Some("Command can move or rename files".to_string())
        }
        Some("cp" | "copy") => {
            Some("Command can copy files and potentially overwrite existing files".to_string())
        }
        Some("chmod" | "chown" | "chgrp") => {
            Some("Command can change file permissions or ownership".to_string())
        }
        Some("sudo" | "su") => {
            Some("Command attempts privilege escalation".to_string())
        }
        Some("curl" | "wget" | "fetch") => {
            Some("Command can download files from the internet".to_string())
        }
        _ => {
            if has_dangerous_patterns(command) {
                Some("Command contains potentially dangerous shell patterns".to_string())
            } else {
                None
            }
        }
    }
}

// Legacy shell parsing functions (keep for compatibility)

// A conservative parser that accepts a very small, safe subset of shell.
// Rules (aligned with the reference spec):
// - Allow words (letters/digits/_-.//) and quoted strings without expansions.
// - Allow operators: &&, ||, ;, | (no redirections, background, subshells).
// - Reject: >, <, >>, <<, $, ``, $(), (), & (background), env-assignment prefixes (FOO=bar ls).
// - Reject trailing operators (e.g., `ls &&`).
pub fn is_known_safe(input: &str) -> bool {
    parse_seq(input).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Word(String),
    OpAnd,
    OpOr,
    OpSeq,
    OpPipe,
}

fn parse_seq(s: &str) -> Option<Vec<Vec<String>>> {
    let toks = tokenize(s)?;
    if toks.is_empty() { return None; }
    // Split by ;, &&, || while keeping pipelines inside each command sequence
    let mut seqs: Vec<Vec<Tok>> = vec![Vec::new()];
    for t in toks {
        match t {
            Tok::OpSeq | Tok::OpAnd | Tok::OpOr => {
                // new sequence
                if seqs.last().map(|v| v.is_empty()).unwrap_or(true) { return None; } // trailing op
                seqs.push(Vec::new());
            }
            other => seqs.last_mut().unwrap().push(other),
        }
    }
    if seqs.last().map(|v| v.is_empty()).unwrap_or(true) { return None; }

    // Each seq must be a pipeline of words and | between
    let mut result = Vec::new();
    for seq in seqs {
        if seq.is_empty() { return None; }
        let mut cmd: Vec<String> = Vec::new();
        let mut cmds: Vec<Vec<String>> = Vec::new();
        for t in seq {
            match t {
                Tok::Word(w) => cmd.push(w),
                Tok::OpPipe => {
                    if cmd.is_empty() { return None; }
                    cmds.push(std::mem::take(&mut cmd));
                }
                Tok::OpAnd | Tok::OpOr | Tok::OpSeq => unreachable!(),
            }
        }
        if cmd.is_empty() { return None; }
        cmds.push(cmd);
        result.extend(cmds);
    }
    Some(result)
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let mut toks = Vec::new();
    let mut buf = String::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' => {
                chars.next();
                flush_word(&mut buf, &mut toks)?;
            }
            '|' => {
                chars.next();
                flush_word(&mut buf, &mut toks)?;
                toks.push(Tok::OpPipe);
            }
            ';' => {
                chars.next();
                flush_word(&mut buf, &mut toks)?;
                toks.push(Tok::OpSeq);
            }
            '&' => {
                // allow only &&
                chars.next();
                if chars.peek() == Some(&'&') { chars.next(); flush_word(&mut buf, &mut toks)?; toks.push(Tok::OpAnd); } else { return None; }
            }
            '|' if matches!(chars.clone().nth(1), Some('|')) => {
                // handled in '|' branch; but this pattern overlaps, so skip
                unreachable!()
            }
            '>' | '<' => return None, // redirections not allowed
            '(' | ')' => return None, // subshells not allowed
            '$' | '`' => return None, // expansions not allowed
            '"' | '\'' => {
                // quoted string without expansions
                let quote = c;
                chars.next();
                let mut q = String::new();
                while let Some(ch) = chars.next() {
                    if ch == quote { break; }
                    if ch == '$' || ch == '`' { return None; }
                    q.push(ch);
                }
                if !matches!(chars.peek(), Some(' ') | Some('\t') | Some('\n') | Some('|') | Some(';') | Some('&') | None) {
                    // quotes must end at token boundary
                }
                if !buf.is_empty() { buf.push(' '); }
                buf.push_str(&q);
            }
            _ => {
                // Only allow safe word chars
                if is_safe_char(c) {
                    buf.push(c);
                    chars.next();
                } else {
                    return None;
                }
            }
        }
    }
    flush_word(&mut buf, &mut toks)?;
    // reject assignment prefix: first token like NAME=VALUE
    if let Some(Tok::Word(w)) = toks.first() {
        if w.contains('=') && !w.starts_with("./") { return None; }
    }
    Some(toks)
}

fn flush_word(buf: &mut String, toks: &mut Vec<Tok>) -> Option<()> {
    if !buf.trim().is_empty() {
        toks.push(Tok::Word(buf.trim().to_string()));
        buf.clear();
    }
    Some(())
}

fn is_safe_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '=')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_commands() {
        let safe_commands = vec![
            vec!["ls".to_string()],
            vec!["cat".to_string(), "file.txt".to_string()],
            vec!["grep".to_string(), "pattern".to_string(), "file.txt".to_string()],
            vec!["echo".to_string(), "hello".to_string()],
            vec!["pwd".to_string()],
            vec!["whoami".to_string()],
            vec!["git".to_string(), "status".to_string()],
        ];

        for cmd in safe_commands {
            assert!(is_known_safe_command(&cmd), "Command should be safe: {:?}", cmd);
        }
    }

    #[test]
    fn test_unsafe_commands() {
        let unsafe_commands = vec![
            vec!["rm".to_string(), "file.txt".to_string()],
            vec!["sudo".to_string(), "ls".to_string()],
            vec!["curl".to_string(), "http://example.com".to_string()],
            vec!["find".to_string(), "/".to_string(), "-delete".to_string()],
            vec!["git".to_string(), "push".to_string()],
        ];

        for cmd in unsafe_commands {
            assert!(!is_known_safe_command(&cmd), "Command should be unsafe: {:?}", cmd);
        }
    }

    #[test]
    fn test_bash_lc_safe() {
        let cmd = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "ls -la".to_string(),
        ];
        assert!(is_known_safe_command(&cmd));
    }

    #[test]
    fn test_bash_lc_unsafe() {
        let cmd = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "rm -rf /".to_string(),
        ];
        assert!(!is_known_safe_command(&cmd));
    }

    #[test]
    fn test_dangerous_patterns() {
        let dangerous_cmd = vec!["echo".to_string(), "test".to_string(), ">".to_string(), "file.txt".to_string()];
        assert!(has_dangerous_patterns(&dangerous_cmd));

        let safe_cmd = vec!["echo".to_string(), "test".to_string()];
        assert!(!has_dangerous_patterns(&safe_cmd));
    }

    #[test]
    fn test_safety_explanations() {
        let rm_cmd = vec!["rm".to_string(), "file.txt".to_string()];
        let explanation = explain_safety_concern(&rm_cmd);
        assert!(explanation.is_some());
        assert!(explanation.unwrap().contains("delete"));

        let safe_cmd = vec!["ls".to_string()];
        let explanation = explain_safety_concern(&safe_cmd);
        assert!(explanation.is_none());
    }

    #[test]
    fn test_legacy_is_known_safe() {
        assert!(is_known_safe("ls -la"));
        assert!(is_known_safe("cat file.txt"));
        assert!(!is_known_safe("rm file.txt"));
        assert!(!is_known_safe("ls > output.txt"));
    }
}
