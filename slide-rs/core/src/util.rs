use std::time::Duration;
use rand::Rng;

pub fn truncate_middle(s: &str, max: usize) -> String {
    if s.len() <= max { return s.into(); }
    let keep = (max.saturating_sub(3)) / 2;
    format!("{}...{}", &s[..keep], &s[s.len()-keep..])
}

// codex-1互換のジッター付き指数バックオフ実装
const INITIAL_DELAY_MS: u64 = 200;
const BACKOFF_FACTOR: f64 = 2.0;

pub fn backoff(attempt: u64) -> Duration {
    let exp = BACKOFF_FACTOR.powi(attempt.saturating_sub(1) as i32);
    let base = (INITIAL_DELAY_MS as f64 * exp) as u64;
    let jitter = rand::thread_rng().gen_range(0.9..1.1);
    Duration::from_millis((base as f64 * jitter) as u64)
}

