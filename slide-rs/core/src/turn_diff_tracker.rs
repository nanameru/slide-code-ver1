use std::collections::HashMap;
use std::path::{Path, PathBuf};
use protocol::protocol::FileChange;

#[derive(Debug, Clone)]
pub struct TurnDiffTracker {
    cwd: PathBuf,
    changes: HashMap<PathBuf, FileChange>,
}

impl TurnDiffTracker {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd, changes: HashMap::new() }
    }

    pub fn is_empty(&self) -> bool { self.changes.is_empty() }

    pub fn record_write_file(&mut self, path: &Path, before: Option<String>, after: &str) {
        let p = self.normalize(path);
        match before {
            None => {
                // New file
                self.changes.insert(p, FileChange::Add { content: after.to_string() });
            }
            Some(prev) => {
                if prev == after { return; }
                let hunk = generate_update_hunk(&prev, after);
                self.changes.insert(p, FileChange::Update { unified_diff: hunk, move_path: None });
            }
        }
    }

    pub fn record_delete_file(&mut self, path: &Path) {
        let p = self.normalize(path);
        self.changes.insert(p, FileChange::Delete);
    }

    pub fn record_move_file(&mut self, from: &Path, to: &Path) {
        let p = self.normalize(from);
        let to_rel = self.normalize(to);
        self.changes.insert(p, FileChange::Update { unified_diff: String::new(), move_path: Some(to_rel) });
    }

    pub fn record_apply_patch_content(&mut self, patch_content: &str) {
        // Parse a simplified apply_patch grammar to extract per-file changes
        let mut lines = patch_content.lines().peekable();
        let mut current_file: Option<PathBuf> = None;
        let mut current_update: Option<(PathBuf, Option<PathBuf>, Vec<String>)> = None;

        while let Some(line) = lines.next() {
            if let Some(path) = line.strip_prefix("*** Add File: ") {
                flush_update(&mut self.changes, current_update.take());
                current_file = Some(PathBuf::from(path.trim()));
                // Collect '+' lines as content
                let mut content = String::new();
                while let Some(ln) = lines.peek() {
                    if ln.starts_with("*** ") { break; }
                    let ln = lines.next().unwrap();
                    if let Some(rest) = ln.strip_prefix('+') { content.push_str(rest); content.push('\n'); }
                }
                self.changes.insert(self.normalize_pathbuf(&current_file.clone().unwrap()), FileChange::Add { content });
                current_file = None;
                continue;
            }
            if let Some(path) = line.strip_prefix("*** Delete File: ") {
                flush_update(&mut self.changes, current_update.take());
                self.changes.insert(self.normalize(&PathBuf::from(path.trim())), FileChange::Delete);
                current_file = None;
                continue;
            }
            if let Some(path) = line.strip_prefix("*** Update File: ") {
                flush_update(&mut self.changes, current_update.take());
                current_file = Some(PathBuf::from(path.trim()));
                current_update = Some((self.normalize_pathbuf(&current_file.clone().unwrap()), None, Vec::new()));
                continue;
            }
            if let Some(newp) = line.strip_prefix("*** Move to: ") {
                if let Some((_p, mv, _hunk)) = current_update.as_mut() {
                    *mv = Some(self.normalize(&PathBuf::from(newp.trim())));
                }
                continue;
            }
            if let Some((_p, _mv, hunk)) = current_update.as_mut() {
                // Collect raw hunk lines (@@, ' ', '+', '-')
                if line.starts_with("@@") || line.starts_with(' ') || line.starts_with('+') || line.starts_with('-') || line.is_empty() {
                    hunk.push(line.to_string());
                }
            }
        }
        flush_update(&mut self.changes, current_update.take());
    }

    pub fn emit_unified_patch(&self) -> Option<String> {
        if self.changes.is_empty() { return None; }
        let mut out = String::new();
        out.push_str("*** Begin Patch\n");
        let mut paths: Vec<_> = self.changes.keys().cloned().collect();
        paths.sort();
        for p in paths {
            match &self.changes[&p] {
                FileChange::Add { content } => {
                    out.push_str(&format!("*** Add File: {}\n", p.display()));
                    for ln in content.split('\n') {
                        if ln.is_empty() { continue; }
                        out.push('+'); out.push_str(ln); out.push('\n');
                    }
                }
                FileChange::Delete => {
                    out.push_str(&format!("*** Delete File: {}\n", p.display()));
                }
                FileChange::Update { unified_diff, move_path } => {
                    out.push_str(&format!("*** Update File: {}\n", p.display()));
                    if let Some(mp) = move_path { out.push_str(&format!("*** Move to: {}\n", mp.display())); }
                    let hunk = strip_wrappers(unified_diff);
                    if !hunk.is_empty() { out.push_str(&hunk); if !hunk.ends_with('\n') { out.push('\n'); } }
                }
            }
        }
        out.push_str("*** End Patch\n");
        Some(out)
    }

    fn normalize(&self, path: &Path) -> PathBuf { self.normalize_pathbuf(path) }
    fn normalize_pathbuf(&self, path: &Path) -> PathBuf {
        let p = if path.is_absolute() { path.to_path_buf() } else { self.cwd.join(path) };
        // produce path relative to cwd for display
        match p.strip_prefix(&self.cwd) { Ok(rel) => rel.to_path_buf(), Err(_) => p }
    }
}

fn flush_update(map: &mut HashMap<PathBuf, FileChange>, upd: Option<(PathBuf, Option<PathBuf>, Vec<String>)>) {
    if let Some((p, mv, hunk_lines)) = upd {
        let unified = if hunk_lines.is_empty() { String::new() } else { hunk_lines.join("\n") + "\n" };
        map.insert(p, FileChange::Update { unified_diff: unified, move_path: mv });
    }
}

fn generate_update_hunk(before: &str, after: &str) -> String {
    // Naive line-wise hunk: remove all before, add all after
    let mut out = String::new();
    out.push_str("@@\n");
    for ln in before.split('\n') { if !ln.is_empty() { out.push('-'); out.push_str(ln); out.push('\n'); } }
    for ln in after.split('\n') { if !ln.is_empty() { out.push('+'); out.push_str(ln); out.push('\n'); } }
    out
}

fn strip_wrappers(input: &str) -> String {
    // If input already contains apply_patch envelope, extract only the hunk-like lines
    if input.contains("*** Begin Patch") {
        let mut out = String::new();
        for line in input.lines() {
            if line.starts_with("@@") || line.starts_with(' ') || line.starts_with('+') || line.starts_with('-') { out.push_str(line); out.push('\n'); }
        }
        return out;
    }
    input.to_string()
}
