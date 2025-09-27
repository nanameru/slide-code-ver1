// Simplified error handling for slide-code-test

use std::time::Duration;

pub type Result<T> = anyhow::Result<T>;

// For compatibility with codex-1
pub use anyhow::Error;

// codex-1互換のエラー型（run_turn実装に必要な分類）
#[derive(Debug, thiserror::Error)]
pub enum CodexErr {
    /// ストリーム中断エラー（リトライ対象）
    /// オプションでリトライ遅延時間を指定可能
    #[error("stream disconnected before completion: {0}")]
    Stream(String, Option<Duration>),
    
    /// Ctrl-C割り込み（致命的エラー、リトライしない）
    #[error("interrupted (Ctrl-C)")]
    Interrupted,
    
    /// 使用量制限エラー（致命的エラー、リトライしない）
    #[error("usage limit reached: {0}")]
    UsageLimitReached(String),
    
    /// プラン不適合エラー（致命的エラー、リトライしない）
    #[error("To use Codex with your ChatGPT plan, upgrade to Plus")]
    UsageNotIncluded,
    
    /// 環境変数不足エラー（致命的エラー、リトライしない）
    #[error("Missing environment variable: {0}")]
    EnvVar(String),
    
    /// 内部エージェント死亡
    #[error("Internal agent died")]
    InternalAgentDied,
    
    /// その他のエラー（リトライ対象）
    #[error("Other error: {0}")]
    Other(String),
}

pub type CodexResult<T> = std::result::Result<T, CodexErr>;