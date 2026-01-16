use thiserror::Error;

/// Result type for stream log operations.
pub type Result<T> = std::result::Result<T, StreamLogError>;

/// Errors for stream log operations.
#[derive(Debug, Error)]
pub enum StreamLogError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}
