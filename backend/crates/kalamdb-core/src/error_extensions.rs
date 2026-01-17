//! Error conversion trait extensions for reducing boilerplate.
//!
//! This module provides convenient trait methods to convert errors from external
//! crates (like Arrow, Parquet, RocksDB, serde_json) into KalamDB error types without repetitive
//! .map_err() calls.
//!
//! # Examples
//!
//! ```rust,ignore
//! // Serialization errors
//! let json = serde_json::to_string(&data)
//!     .into_serialization_error("Failed to serialize config")?;
//!
//! // Arrow/DataFusion errors
//! let batch = RecordBatch::try_new(schema, columns)
//!     .into_arrow_error_ctx("Failed to create record batch")?;
//!
//! // Generic errors with context
//! file_operation()
//!     .into_kalamdb_error("File operation failed")?;
//!
//! // Invalid operation errors
//! validate_input()
//!     .into_invalid_operation("Input validation failed")?;
//!
//! // Not found errors
//! find_resource()
//!     .into_not_found("Resource not found")?;
//! ```

use crate::error::KalamDbError;

/// Extension trait for Result types to simplify error conversions.
///
/// Provides convenience methods to convert any error type into specific
/// KalamDbError variants without verbose .map_err() closures.
pub trait KalamDbResultExt<T> {
    /// Convert any error into KalamDbError::Other with context message.
    ///
    /// # Example
    /// ```rust,ignore
    /// file.read_to_string()
    ///     .into_kalamdb_error("Failed to read config file")?;
    /// ```
    fn into_kalamdb_error(self, context: &str) -> Result<T, KalamDbError>;

    /// Convert Arrow errors into KalamDbError with generic message.
    ///
    /// # Example
    /// ```rust,ignore
    /// RecordBatch::try_new(schema, columns)
    ///     .into_arrow_error()?;
    /// ```
    fn into_arrow_error(self) -> Result<T, KalamDbError>;

    /// Convert Arrow errors into KalamDbError with context.
    ///
    /// # Example
    /// ```rust,ignore
    /// cast(&array, &new_type)
    ///     .into_arrow_error_ctx("Schema evolution cast failed")?;
    /// ```
    fn into_arrow_error_ctx(self, context: &str) -> Result<T, KalamDbError>;

    /// Convert errors into KalamDbError::ExecutionError with context.
    ///
    /// # Example
    /// ```rust,ignore
    /// execute_query()
    ///     .into_execution_error("Query execution failed")?;
    /// ```
    fn into_execution_error(self, context: &str) -> Result<T, KalamDbError>;

    /// Convert errors into KalamDbError::SerializationError.
    ///
    /// # Example
    /// ```rust,ignore
    /// serde_json::to_string(&config)
    ///     .into_serialization_error("Config serialization failed")?;
    /// ```
    fn into_serialization_error(self, context: &str) -> Result<T, KalamDbError>;

    /// Convert errors into KalamDbError::SchemaError.
    ///
    /// # Example
    /// ```rust,ignore
    /// validate_schema_change()
    ///     .into_schema_error("Invalid schema evolution")?;
    /// ```
    fn into_schema_error(self, context: &str) -> Result<T, KalamDbError>;

    /// Convert errors into KalamDbError::InvalidOperation.
    ///
    /// # Example
    /// ```rust,ignore
    /// validate_table_options()
    ///     .into_invalid_operation("Invalid table configuration")?;
    /// ```
    fn into_invalid_operation(self, context: &str) -> Result<T, KalamDbError>;

    /// Convert errors into KalamDbError::NotFound.
    ///
    /// # Example
    /// ```rust,ignore
    /// find_table(&table_id)
    ///     .into_not_found("Table not found")?;
    /// ```
    fn into_not_found(self, context: &str) -> Result<T, KalamDbError>;

    /// Convert errors into KalamDbError::AlreadyExists.
    ///
    /// # Example
    /// ```rust,ignore
    /// check_table_exists(&table_name)
    ///     .into_already_exists("Table already exists")?;
    /// ```
    fn into_already_exists(self, context: &str) -> Result<T, KalamDbError>;

    /// Convert errors into KalamDbError::ConfigError.
    ///
    /// # Example
    /// ```rust,ignore
    /// parse_config_value()
    ///     .into_config_error("Invalid configuration")?;
    /// ```
    fn into_config_error(self, context: &str) -> Result<T, KalamDbError>;
}

impl<T, E: std::fmt::Display> KalamDbResultExt<T> for Result<T, E> {
    #[inline]
    fn into_kalamdb_error(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::Other(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_arrow_error(self) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::Other(format!("Arrow error: {}", e)))
    }

    #[inline]
    fn into_arrow_error_ctx(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::Other(format!("Arrow error - {}: {}", context, e)))
    }

    #[inline]
    fn into_execution_error(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::ExecutionError(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_serialization_error(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::SerializationError(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_schema_error(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::SchemaError(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_invalid_operation(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::InvalidOperation(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_not_found(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::NotFound(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_already_exists(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::AlreadyExists(format!("{}: {}", context, e)))
    }

    #[inline]
    fn into_config_error(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::ConfigError(format!("{}: {}", context, e)))
    }
}

/// Specialized extension methods for commonly-used types.
///
/// These provide even more ergonomic conversions for specific error types
/// like serde_json and tokio JoinError.
pub trait SerdeJsonResultExt<T> {
    /// Convert serde_json errors into KalamDbError::SerializationError.
    ///
    /// # Example
    /// ```rust,ignore
    /// let json = serde_json::to_string(&data)
    ///     .into_serde_error("Config serialization")?;
    /// ```
    fn into_serde_error(self, context: &str) -> Result<T, KalamDbError>;
}

impl<T> SerdeJsonResultExt<T> for Result<T, serde_json::Error> {
    #[inline]
    fn into_serde_error(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| KalamDbError::SerializationError(format!("{}: {}", context, e)))
    }
}

/// Extension for tokio JoinError (for spawn_blocking, spawn, etc.)
pub trait TokioJoinResultExt<T> {
    /// Convert tokio JoinError into KalamDbError.
    ///
    /// # Example
    /// ```rust,ignore
    /// tokio::task::spawn_blocking(move || {
    ///     // heavy computation
    /// })
    /// .await
    /// .into_join_error("Background task failed")?;
    /// ```
    fn into_join_error(self, context: &str) -> Result<T, KalamDbError>;
}

impl<T> TokioJoinResultExt<T> for Result<T, tokio::task::JoinError> {
    #[inline]
    fn into_join_error(self, context: &str) -> Result<T, KalamDbError> {
        self.map_err(|e| {
            if e.is_cancelled() {
                KalamDbError::Other(format!("{}: task was cancelled", context))
            } else if e.is_panic() {
                KalamDbError::Other(format!("{}: task panicked", context))
            } else {
                KalamDbError::Other(format!("{}: {}", context, e))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_into_kalamdb_error() {
        let result: Result<(), &str> = Err("something failed");
        let err = result.into_kalamdb_error("Test operation").unwrap_err();

        match err {
            KalamDbError::Other(msg) => {
                assert!(msg.contains("Test operation"));
                assert!(msg.contains("something failed"));
            },
            _ => panic!("Expected KalamDbError::Other"),
        }
    }

    #[test]
    fn test_into_arrow_error() {
        let result: Result<(), &str> = Err("arrow failed");
        let err = result.into_arrow_error().unwrap_err();

        match err {
            KalamDbError::Other(msg) => assert!(msg.contains("arrow failed")),
            _ => panic!("Expected KalamDbError::Other"),
        }
    }

    #[test]
    fn test_into_execution_error() {
        let result: Result<(), &str> = Err("execution issue");
        let err = result.into_execution_error("Query execution").unwrap_err();

        match err {
            KalamDbError::ExecutionError(msg) => {
                assert!(msg.contains("Query execution"));
                assert!(msg.contains("execution issue"));
            },
            _ => panic!("Expected KalamDbError::ExecutionError"),
        }
    }

    #[test]
    fn test_into_serialization_error() {
        let result: Result<(), &str> = Err("json parse error");
        let err = result.into_serialization_error("Parsing config").unwrap_err();

        match err {
            KalamDbError::SerializationError(msg) => {
                assert!(msg.contains("Parsing config"));
                assert!(msg.contains("json parse error"));
            },
            _ => panic!("Expected KalamDbError::SerializationError"),
        }
    }

    #[test]
    fn test_into_schema_error() {
        let result: Result<(), &str> = Err("incompatible types");
        let err = result.into_schema_error("Schema validation").unwrap_err();

        match err {
            KalamDbError::SchemaError(msg) => {
                assert!(msg.contains("Schema validation"));
                assert!(msg.contains("incompatible types"));
            },
            _ => panic!("Expected KalamDbError::SchemaError"),
        }
    }

    #[test]
    fn test_into_invalid_operation() {
        let result: Result<(), &str> = Err("operation not allowed");
        let err = result.into_invalid_operation("Validation").unwrap_err();

        match err {
            KalamDbError::InvalidOperation(msg) => {
                assert!(msg.contains("Validation"));
                assert!(msg.contains("operation not allowed"));
            },
            _ => panic!("Expected KalamDbError::InvalidOperation"),
        }
    }

    #[test]
    fn test_into_not_found() {
        let result: Result<(), &str> = Err("resource missing");
        let err = result.into_not_found("Resource lookup").unwrap_err();

        match err {
            KalamDbError::NotFound(msg) => {
                assert!(msg.contains("Resource lookup"));
                assert!(msg.contains("resource missing"));
            },
            _ => panic!("Expected KalamDbError::NotFound"),
        }
    }

    #[test]
    fn test_into_already_exists() {
        let result: Result<(), &str> = Err("duplicate entry");
        let err = result.into_already_exists("Create table").unwrap_err();

        match err {
            KalamDbError::AlreadyExists(msg) => {
                assert!(msg.contains("Create table"));
                assert!(msg.contains("duplicate entry"));
            },
            _ => panic!("Expected KalamDbError::AlreadyExists"),
        }
    }

    #[test]
    fn test_into_config_error() {
        let result: Result<(), &str> = Err("invalid config value");
        let err = result.into_config_error("Config parsing").unwrap_err();

        match err {
            KalamDbError::ConfigError(msg) => {
                assert!(msg.contains("Config parsing"));
                assert!(msg.contains("invalid config value"));
            },
            _ => panic!("Expected KalamDbError::ConfigError"),
        }
    }

    #[test]
    fn test_serde_json_error() {
        // Test with actual serde_json error
        let invalid_json = "{invalid json}";
        let result: Result<serde_json::Value, _> = serde_json::from_str(invalid_json);
        let err = result.into_serde_error("JSON parsing").unwrap_err();

        match err {
            KalamDbError::SerializationError(msg) => {
                assert!(msg.contains("JSON parsing"));
            },
            _ => panic!("Expected KalamDbError::SerializationError"),
        }
    }
}
