use thiserror::Error;

#[derive(Error, Debug)]
pub enum CodexError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    
    #[error("Channel send error")]
    ChannelSend,
    
    #[error("Channel receive error")]
    ChannelRecv,
    
    #[error("Generic error: {0}")]
    Generic(String),
}

pub type Result<T> = std::result::Result<T, CodexError>;