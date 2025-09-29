use std::path::{Path, PathBuf};

/// Resolve a potentially messy user-provided path against a cwd, applying
/// light sanitization similar to codex-1's approach.
pub(crate) fn resolve_path_with_cwd(cwd: &Path, raw: &str) -> PathBuf {
    let mut s = raw.trim();

    // Strip surrounding quotes (' " `)
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
        || (s.starts_with('`') && s.ends_with('`'))
    {
        if s.len() >= 2 {
            s = &s[1..s.len() - 1];
        }
    }

    // Fix missing slash after /Users (e.g., "/Userskim" -> "/Users/kim")
    const USERS: &str = "/Users";
    if s.starts_with(USERS) && !s.starts_with("/Users/") {
        let after = &s[USERS.len()..];
        if !after.starts_with('/') {
            let fixed = format!("{}/{}", USERS, after);
            return PathBuf::from(fixed);
        }
    }
    // Handle leading "Users" without slash
    if s.starts_with("Users") && !s.starts_with("Users/") {
        let after = &s["Users".len()..];
        let fixed = format!("/Users/{}", after);
        return PathBuf::from(fixed);
    }

    // Already absolute
    if s.starts_with('/') {
        return PathBuf::from(s);
    }

    // Common top-levels: add leading slash
    const TOPS: &[&str] = &[
        "Users/", "Volumes/", "System/", "Applications/", "Library/", "usr/", "bin/",
        "sbin/", "etc/", "var/", "opt/", "private/", "tmp/", "home/",
    ];
    for prefix in TOPS {
        if s.starts_with(prefix) {
            let fixed = format!("/{}", s);
            return PathBuf::from(fixed);
        }
    }

    // Relative: resolve against cwd
    cwd.join(s)
}
