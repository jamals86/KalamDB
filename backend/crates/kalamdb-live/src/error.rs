//! Error types for kalamdb-live

use thiserror::Error;

/// Errors that can occur in live query operations
#[derive(Error, Debug)]
pub enum LiveError {
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("System error: {0}")]
    System(String),

    #[error("Other error: {0}")]
    Other(String),
}

/// Result type for live query operations
pub type Result<T> = std::result::Result<T, LiveError>;

/// For backward compatibility with code expecting KalamDbError
pub type KalamDbError = LiveError;
