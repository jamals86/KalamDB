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
    SerializationError(String),

    #[error("Invalid SQL: {0}")]
    InvalidSql(String),

    #[error("System error: {0}")]
    System(String),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<serde_json::Error> for LiveError {
    fn from(e: serde_json::Error) -> Self {
        LiveError::SerializationError(e.to_string())
    }
}

impl From<datafusion::error::DataFusionError> for LiveError {
    fn from(e: datafusion::error::DataFusionError) -> Self {
        LiveError::InvalidSql(e.to_string())
    }
}

impl From<kalamdb_tables::TableError> for LiveError {
    fn from(e: kalamdb_tables::TableError) -> Self {
        LiveError::Storage(e.to_string())
    }
}

impl From<kalamdb_system::SystemError> for LiveError {
    fn from(e: kalamdb_system::SystemError) -> Self {
        LiveError::System(e.to_string())
    }
}

impl From<crate::schema_registry::error::RegistryError> for LiveError {
    fn from(e: crate::schema_registry::error::RegistryError) -> Self {
        LiveError::Storage(e.to_string())
    }
}

/// Result type for live query operations
pub type Result<T> = std::result::Result<T, LiveError>;

/// For backward compatibility with code expecting KalamDbError
pub type KalamDbError = LiveError;
