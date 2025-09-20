use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::{Read, Write};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Clone, Debug)]
pub(crate) struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn default() -> Self {
        let mut path = home_dir();
        path.push(".slide");
        path.push("history.jsonl");
        Self { path }
    }

    /// Ensure parent directory exists.
    fn ensure_parent_dir(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    /// Append a text entry as a single JSON line. Best-effort; errors are returned.
    pub fn append(&self, text: &str) -> std::io::Result<()> {
        self.ensure_parent_dir()?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut line = format!("{{\"ts\":{},\"text\":{}}}\n", ts, json_escape(text));
        // Open append-only; set 0600 on Unix
        let mut opts = OpenOptions::new();
        opts.append(true).create(true).read(true);
        #[cfg(unix)]
        {
            opts.mode(0o600);
        }
        let mut f = opts.open(&self.path)?;
        f.write_all(line.as_bytes())?;
        f.flush()
    }

    /// Return (identifier, entry_count). Identifier is inode on Unix, 0 elsewhere.
    pub fn metadata(&self) -> (u64, usize) {
        let mut id = 0u64;
        let mut count = 0usize;
        match std::fs::metadata(&self.path) {
            Ok(meta) => {
                #[cfg(unix)]
                {
                    id = meta.ino();
                }
                #[cfg(not(unix))]
                {
                    let _ = meta;
                }
            }
            Err(_) => return (id, 0),
        }
        if let Ok(mut f) = std::fs::File::open(&self.path) {
            let mut buf = [0u8; 8192];
            loop {
                match f.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => count += buf[..n].iter().filter(|&&b| b == b'\n').count(),
                    Err(_) => break,
                }
            }
        }
        (id, count)
    }

    /// Lookup the `offset`-th entry by counting lines; validate `log_id` on Unix.
    pub fn lookup(&self, log_id: u64, offset: usize) -> Option<String> {
        // Validate id on Unix
        #[cfg(unix)]
        {
            if let Ok(meta) = std::fs::metadata(&self.path) {
                if meta.ino() != log_id {
                    return None;
                }
            } else {
                return None;
            }
        }
        let f = OpenOptions::new().read(true).open(&self.path).ok()?;
        let reader = std::io::BufReader::new(f);
        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res.ok()?;
            if idx == offset {
                // Parse minimal {"text":...}
                if let Some(txt) = extract_text_field(&line) {
                    return Some(txt);
                } else {
                    return None;
                }
            }
        }
        None
    }
}

fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn extract_text_field(line: &str) -> Option<String> {
    // Extremely small extractor to avoid JSON deps: find "text":"..."
    let key = "\"text\":";
    let idx = line.find(key)? + key.len();
    let rest = line[idx..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    for ch in rest[1..].chars() {
        // skip opening quote
        if escaped {
            match ch {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => { /* skip simplistic */ }
                other => out.push(other),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}
