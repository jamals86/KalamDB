use thiserror::Error;

/// Shared error type used across the PostgreSQL extension workspace.
#[derive(Debug, Error)]
pub enum KalamPgError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("execution error: {0}")]
    Execution(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

impl From<datafusion::error::DataFusionError> for KalamPgError {
    fn from(value: datafusion::error::DataFusionError) -> Self {
        Self::Execution(value.to_string())
    }
}
