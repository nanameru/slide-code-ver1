// Simplified error handling for slide-code-test

pub type Result<T> = anyhow::Result<T>;

// For compatibility with codex-1
pub use anyhow::Error;

// codex-1互換のエラー型
#[derive(Debug, thiserror::Error)]
pub enum CodexErr {
    #[error("Internal agent died")]
    InternalAgentDied,
    #[error("Stream error: {0}")]
    Stream(String, Option<Box<dyn std::error::Error + Send + Sync>>),
    #[error("Other error: {0}")]
    Other(String),
}

pub type CodexResult<T> = std::result::Result<T, CodexErr>;