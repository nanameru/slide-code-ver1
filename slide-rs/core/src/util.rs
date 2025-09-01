pub fn truncate_middle(s: &str, max: usize) -> String {
    if s.len() <= max { return s.into(); }
    let keep = (max.saturating_sub(3)) / 2;
    format!("{}...{}", &s[..keep], &s[s.len()-keep..])
}

