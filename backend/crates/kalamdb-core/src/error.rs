// Error types module
use thiserror::Error;

/// Main error type for KalamDB
#[derive(Error, Debug)]
pub enum KalamDbError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("API error: {0}")]
    Api(#[from] ApiError),

    #[error("Configuration file error: {0}")]
    ConfigError(String),

    #[error("Schema error: {0}")]
    SchemaError(String),

    #[error("Catalog error: {0}")]
    CatalogError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Invalid SQL: {0}")]
    InvalidSql(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("{0}")]
    Other(String),
}

/// Storage-related errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Message not found: {0}")]
    MessageNotFound(i64),

    #[error("Invalid key format: {0}")]
    InvalidKey(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database is closed")]
    DatabaseClosed,

    #[error("Storage error: {0}")]
    Other(String),
}

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    ParseError(String),

    #[error("Invalid configuration: {0}")]
    ValidationError(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid value for {field}: {value}")]
    InvalidValue { field: String, value: String },
}

/// API-related errors
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Message too large: {size} bytes (max: {max} bytes)")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Query limit exceeded: {requested} (max: {max})")]
    QueryLimitExceeded { requested: usize, max: usize },

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl StorageError {
    /// Create a validation error
    pub fn validation<S: Into<String>>(msg: S) -> Self {
        StorageError::Validation(msg.into())
    }

    /// Create a generic error
    pub fn other<S: Into<String>>(msg: S) -> Self {
        StorageError::Other(msg.into())
    }
}

impl ConfigError {
    /// Create a parse error
    pub fn parse<S: Into<String>>(msg: S) -> Self {
        ConfigError::ParseError(msg.into())
    }

    /// Create a validation error
    pub fn validation<S: Into<String>>(msg: S) -> Self {
        ConfigError::ValidationError(msg.into())
    }
}

impl ApiError {
    /// Create an invalid request error
    pub fn invalid_request<S: Into<String>>(msg: S) -> Self {
        ApiError::InvalidRequest(msg.into())
    }

    /// Create an internal error
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        ApiError::Internal(msg.into())
    }
}

// Conversion from StorageError validation to string
impl From<String> for StorageError {
    fn from(msg: String) -> Self {
        StorageError::Validation(msg)
    }
}

// Conversion from String to KalamDbError
impl From<String> for KalamDbError {
    fn from(msg: String) -> Self {
        KalamDbError::Other(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_error_display() {
        let err = StorageError::MessageNotFound(12345);
        assert_eq!(err.to_string(), "Message not found: 12345");
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::MissingField("port".to_string());
        assert_eq!(err.to_string(), "Missing required field: port");
    }

    #[test]
    fn test_api_error_display() {
        let err = ApiError::MessageTooLarge {
            size: 2_000_000,
            max: 1_048_576,
        };
        assert_eq!(
            err.to_string(),
            "Message too large: 2000000 bytes (max: 1048576 bytes)"
        );
    }

    #[test]
    fn test_storage_error_validation() {
        let err = StorageError::validation("invalid data");
        assert!(matches!(err, StorageError::Validation(_)));
    }

    #[test]
    fn test_config_error_parse() {
        let err = ConfigError::parse("invalid TOML");
        assert!(matches!(err, ConfigError::ParseError(_)));
    }
}

