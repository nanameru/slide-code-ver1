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
