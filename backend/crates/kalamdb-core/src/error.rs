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

    #[error("Table not found: {0}")]
    TableNotFound(String),

    #[error("Namespace not found: {0}")]
    NamespaceNotFound(String),

    #[error("Schema version not found: table={table}, version={version}")]
    SchemaVersionNotFound { table: String, version: i32 },

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Invalid schema evolution: {0}")]
    InvalidSchemaEvolution(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Column family error: {0}")]
    ColumnFamily(#[from] ColumnFamilyError),

    #[error("Flush error: {0}")]
    Flush(#[from] FlushError),

    #[error("Backup error: {0}")]
    Backup(#[from] BackupError),

    #[error("{0}")]
    Other(String),
}

/// Storage-related errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Backend error: {0}")]
    Backend(String),

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

/// Column family operation errors
#[derive(Error, Debug)]
pub enum ColumnFamilyError {
    #[error("Column family not found: {0}")]
    NotFound(String),

    #[error("Failed to create column family: {0}")]
    CreateFailed(String),

    #[error("Failed to drop column family: {0}")]
    DropFailed(String),

    #[error("Column family already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid column family name: {0}")]
    InvalidName(String),

    #[error("RocksDB error: {0}")]
    RocksDb(String),
}

/// Flush operation errors
#[derive(Error, Debug)]
pub enum FlushError {
    #[error("Failed to read from RocksDB buffer: {0}")]
    ReadFailed(String),

    #[error("Failed to write Parquet file: {0}")]
    WriteFailed(String),

    #[error("No data to flush for table: {0}")]
    NoData(String),

    #[error("Flush policy not configured for table: {0}")]
    NoPolicyConfigured(String),

    #[error("Failed to update flush metadata: {0}")]
    MetadataUpdateFailed(String),

    #[error("IO error during flush: {0}")]
    Io(String),

    #[error("Serialization error during flush: {0}")]
    Serialization(String),
}

/// Backup/restore operation errors
#[derive(Error, Debug)]
pub enum BackupError {
    #[error("Backup not found: {0}")]
    NotFound(String),

    #[error("Failed to create backup: {0}")]
    CreateFailed(String),

    #[error("Failed to restore backup: {0}")]
    RestoreFailed(String),

    #[error("Backup manifest is corrupt: {0}")]
    CorruptManifest(String),

    #[error("Backup validation failed: {0}")]
    ValidationFailed(String),

    #[error("Failed to copy Parquet files: {0}")]
    FileCopyFailed(String),

    #[error("Checksum mismatch: expected={expected}, actual={actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("IO error during backup/restore: {0}")]
    Io(String),
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

impl ColumnFamilyError {
    /// Create a not found error
    pub fn not_found<S: Into<String>>(name: S) -> Self {
        ColumnFamilyError::NotFound(name.into())
    }

    /// Create a create failed error
    pub fn create_failed<S: Into<String>>(msg: S) -> Self {
        ColumnFamilyError::CreateFailed(msg.into())
    }

    /// Create a drop failed error
    pub fn drop_failed<S: Into<String>>(msg: S) -> Self {
        ColumnFamilyError::DropFailed(msg.into())
    }
}

impl FlushError {
    /// Create a read failed error
    pub fn read_failed<S: Into<String>>(msg: S) -> Self {
        FlushError::ReadFailed(msg.into())
    }

    /// Create a write failed error
    pub fn write_failed<S: Into<String>>(msg: S) -> Self {
        FlushError::WriteFailed(msg.into())
    }

    /// Create a no data error
    pub fn no_data<S: Into<String>>(table: S) -> Self {
        FlushError::NoData(table.into())
    }
}

impl BackupError {
    /// Create a not found error
    pub fn not_found<S: Into<String>>(backup: S) -> Self {
        BackupError::NotFound(backup.into())
    }

    /// Create a create failed error
    pub fn create_failed<S: Into<String>>(msg: S) -> Self {
        BackupError::CreateFailed(msg.into())
    }

    /// Create a restore failed error
    pub fn restore_failed<S: Into<String>>(msg: S) -> Self {
        BackupError::RestoreFailed(msg.into())
    }

    /// Create a validation failed error
    pub fn validation_failed<S: Into<String>>(msg: S) -> Self {
        BackupError::ValidationFailed(msg.into())
    }
}

impl KalamDbError {
    /// Create a table not found error
    pub fn table_not_found<S: Into<String>>(table: S) -> Self {
        KalamDbError::TableNotFound(table.into())
    }

    /// Create a namespace not found error
    pub fn namespace_not_found<S: Into<String>>(namespace: S) -> Self {
        KalamDbError::NamespaceNotFound(namespace.into())
    }

    /// Create a schema version not found error
    pub fn schema_version_not_found<S: Into<String>>(table: S, version: i32) -> Self {
        KalamDbError::SchemaVersionNotFound {
            table: table.into(),
            version,
        }
    }

    /// Create an invalid schema evolution error
    pub fn invalid_schema_evolution<S: Into<String>>(msg: S) -> Self {
        KalamDbError::InvalidSchemaEvolution(msg.into())
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

// Conversion from anyhow::Error to KalamDbError
impl From<anyhow::Error> for KalamDbError {
    fn from(err: anyhow::Error) -> Self {
        KalamDbError::Other(err.to_string())
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

    #[test]
    fn test_table_not_found_error() {
        let err = KalamDbError::table_not_found("messages");
        assert_eq!(err.to_string(), "Table not found: messages");
    }

    #[test]
    fn test_namespace_not_found_error() {
        let err = KalamDbError::namespace_not_found("default");
        assert_eq!(err.to_string(), "Namespace not found: default");
    }

    #[test]
    fn test_schema_version_not_found_error() {
        let err = KalamDbError::schema_version_not_found("messages", 3);
        assert_eq!(
            err.to_string(),
            "Schema version not found: table=messages, version=3"
        );
    }

    #[test]
    fn test_invalid_schema_evolution_error() {
        let err = KalamDbError::invalid_schema_evolution("cannot drop required column");
        assert_eq!(
            err.to_string(),
            "Invalid schema evolution: cannot drop required column"
        );
    }

    #[test]
    fn test_column_family_not_found_error() {
        let err = ColumnFamilyError::not_found("user_table:messages");
        assert_eq!(
            err.to_string(),
            "Column family not found: user_table:messages"
        );
    }

    #[test]
    fn test_flush_error_no_data() {
        let err = FlushError::no_data("messages");
        assert_eq!(err.to_string(), "No data to flush for table: messages");
    }

    #[test]
    fn test_backup_checksum_mismatch() {
        let err = BackupError::ChecksumMismatch {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Checksum mismatch: expected=abc123, actual=def456"
        );
    }
}
