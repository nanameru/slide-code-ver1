use std::time::Duration;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, CodexErr>;

#[derive(Error, Debug)]
pub enum SandboxErr {
    /// Error from sandbox execution
    #[error("sandbox denied exec error, exit code: {0}, stdout: {1}, stderr: {2}")]
    Denied(i32, String, String),
    /// Command timed out
    #[error("command timed out")]
    Timeout,
    /// Command was killed by a signal
    #[error("command was killed by a signal")]
    Signal(i32),
    /// Error from linux landlock
    #[error("Landlock was not able to fully enforce all sandbox rules")]
    LandlockRestrict,
}

#[derive(Error, Debug)]
pub enum CodexErr {
    /// Stream disconnected before completion
    #[error("stream disconnected before completion: {0}")]
    Stream(String, Option<Duration>),
    /// Conversation not found
    #[error("no conversation with id: {0}")]
    ConversationNotFound(String),
    /// Session configuration error
    #[error("session configured event was not the first event in the stream")]
    SessionConfiguredNotFirstEvent,
    /// Command timeout
    #[error("timeout waiting for child process to exit")]
    Timeout,
    /// Spawn error
    #[error("spawn failed: child stdout/stderr not captured")]
    Spawn,
    /// User interrupted
    #[error("interrupted (Ctrl-C)")]
    Interrupted,
    /// HTTP status error
    #[error("unexpected status {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
    /// Usage limit reached
    #[error("usage limit reached")]
    UsageLimitReached,
    /// Usage not included
    #[error("To use Codex with your ChatGPT plan, upgrade to Plus")]
    UsageNotIncluded,
    /// Internal server error
    #[error("We're currently experiencing high demand")]
    InternalServerError,
    /// Retry limit exceeded
    #[error("exceeded retry limit, last status: {0}")]
    RetryLimit(u16),
    /// Agent loop died
    #[error("internal error; agent loop died unexpectedly")]
    InternalAgentDied,
    /// Sandbox error
    #[error("sandbox error: {0}")]
    Sandbox(#[from] SandboxErr),
    /// Linux sandbox required but not provided
    #[error("codex-linux-sandbox was required but not provided")]
    LinuxSandboxRequired,
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Generic error
    #[error("error: {0}")]
    Generic(String),
}

pub fn get_error_message_ui(err: &CodexErr) -> String {
    match err {
        CodexErr::UsageLimitReached => "Usage limit reached. Please try again later.".to_string(),
        CodexErr::UsageNotIncluded => "To use Codex with your ChatGPT plan, upgrade to Plus.".to_string(),
        CodexErr::InternalServerError => "We're currently experiencing high demand, which may cause temporary errors.".to_string(),
        _ => err.to_string(),
    }
}

pub type Error = anyhow::Error;

