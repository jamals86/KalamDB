use thiserror::Error;

/// Errors that can occur in system table operations
#[derive(Error, Debug)]
pub enum SystemError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("DataFusion error: {0}")]
    DataFusion(String),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Other error: {0}")]
    Other(String),
}

/// Result type for system table operations
pub type Result<T> = std::result::Result<T, SystemError>;

// Convert from kalamdb_store::StorageError
impl From<kalamdb_store::StorageError> for SystemError {
    fn from(err: kalamdb_store::StorageError) -> Self {
        SystemError::Storage(err.to_string())
    }
}
