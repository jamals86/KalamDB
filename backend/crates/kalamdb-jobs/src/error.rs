use thiserror::Error;

/// Errors that can occur in job operations
#[derive(Error, Debug)]
pub enum JobError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Job already running: {0}")]
    AlreadyRunning(String),

    #[error("Job execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Filestore error: {0}")]
    Filestore(String),

    #[error("Other error: {0}")]
    Other(String),
}

/// Result type for job operations
pub type Result<T> = std::result::Result<T, JobError>;

// Convert from kalamdb_store::StorageError
impl From<kalamdb_store::StorageError> for JobError {
    fn from(err: kalamdb_store::StorageError) -> Self {
        JobError::Storage(err.to_string())
    }
}

// Convert from kalamdb_filestore::FilestoreError
impl From<kalamdb_filestore::FilestoreError> for JobError {
    fn from(err: kalamdb_filestore::FilestoreError) -> Self {
        JobError::Filestore(err.to_string())
    }
}
