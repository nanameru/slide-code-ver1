// Simplified error handling for slide-code-test

pub type Result<T> = anyhow::Result<T>;

// For compatibility with codex-1
pub use anyhow::Error;