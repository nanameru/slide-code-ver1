use std::time::Duration;

pub fn truncate_middle(s: &str, max: usize) -> String {
    if s.len() <= max { return s.into(); }
    let keep = (max.saturating_sub(3)) / 2;
    format!("{}...{}", &s[..keep], &s[s.len()-keep..])
}

pub fn backoff(attempt: u32) -> Duration {
    let base_delay = Duration::from_millis(100);
    let max_delay = Duration::from_secs(30);
    
    let exponential_delay = base_delay * 2u32.pow(attempt);
    
    if exponential_delay > max_delay {
        max_delay
    } else {
        exponential_delay
    }
}

