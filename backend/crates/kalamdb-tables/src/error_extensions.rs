//! Error conversion trait extensions for reducing boilerplate in kalamdb-tables.

use crate::error::TableError;

/// Extension trait for Result types to simplify error conversions.
///
/// Provides convenience methods to convert any error type into specific
/// TableError variants without verbose .map_err() closures.
pub trait KalamDbResultExt<T> {
    /// Convert any error into TableError::Other with context message.
    fn into_kalamdb_error(self, context: &str) -> Result<T, TableError>;

    /// Convert errors into TableError::Other with Arrow context.
    fn into_arrow_error(self) -> Result<T, TableError>;

    /// Convert errors into TableError::Other with Arrow context.
    fn into_arrow_error_ctx(self, context: &str) -> Result<T, TableError>;

    /// Convert errors into TableError::Other with execution context.
    fn into_execution_error(self, context: &str) -> Result<T, TableError>;

    /// Convert errors into TableError::Serialization with context.
    fn into_serialization_error(self, context: &str) -> Result<T, TableError>;

    /// Convert errors into TableError::SchemaError with context.
    fn into_schema_error(self, context: &str) -> Result<T, TableError>;

    /// Convert errors into TableError::InvalidOperation with context.
    fn into_invalid_operation(self, context: &str) -> Result<T, TableError>;

    /// Convert errors into TableError::NotFound with context.
    fn into_not_found(self, context: &str) -> Result<T, TableError>;

    /// Convert errors into TableError::AlreadyExists with context.
    fn into_already_exists(self, context: &str) -> Result<T, TableError>;

    /// Convert errors into TableError::Other with config context.
    fn into_config_error(self, context: &str) -> Result<T, TableError>;
}

impl<T, E: std::fmt::Display> KalamDbResultExt<T> for Result<T, E> {
    #[inline]
    fn into_kalamdb_error(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::Other(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_arrow_error(self) -> Result<T, TableError> {
        self.map_err(|e| TableError::Other(format!("Arrow error: {}", e)))
    }

    #[inline]
    fn into_arrow_error_ctx(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::Other(format!("Arrow error - {}: {}", context, e)))
    }

    #[inline]
    fn into_execution_error(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::Other(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_serialization_error(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::Serialization(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_schema_error(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::SchemaError(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_invalid_operation(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::InvalidOperation(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_not_found(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::NotFound(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_already_exists(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::AlreadyExists(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_config_error(self, context: &str) -> Result<T, TableError> {
        self.map_err(|e| TableError::Other(format!("{}: {}", context, e)))
    }
}
