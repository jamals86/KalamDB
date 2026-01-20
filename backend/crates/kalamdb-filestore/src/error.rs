use thiserror::Error;

/// Errors that can occur in filestore operations
#[derive(Error, Debug)]
pub enum FilestoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Parquet error: {0}")]
    Parquet(String),

    #[error("Path error: {0}")]
    Path(String),

    #[error("Path traversal attempt: {0}")]
    PathTraversal(String),

    #[error("Batch not found: {0}")]
    BatchNotFound(String),

    #[error("Invalid batch file: {0}")]
    InvalidBatchFile(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("ObjectStore error: {0}")]
    ObjectStore(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Other error: {0}")]
    Other(String),
}

/// Result type for filestore operations
pub type Result<T> = std::result::Result<T, FilestoreError>;
