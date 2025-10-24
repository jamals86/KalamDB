//! Type-safe wrapper types for KalamDB identifiers and enums.
//!
//! This module provides newtype wrappers around String to enforce type safety
//! at compile time, preventing accidental mixing of user IDs, namespace IDs,
//! and table names.
//!
//! ## Examples
//!
//! ```rust
//! use kalamdb_commons::models::{UserId, NamespaceId, TableName};
//!
//! let user_id = UserId::new("user_123");
//! let namespace_id = NamespaceId::new("default");
//! let table_name = TableName::new("conversations");
//!
//! // Type safety prevents mixing
//! // let wrong: UserId = namespace_id; // Compile error!
//!
//! // Conversion to string
//! let id_str: &str = user_id.as_str();
//! let owned: String = user_id.into_string();
//! ```

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Type-safe wrapper for user identifiers.
///
/// Ensures user IDs cannot be accidentally used where namespace IDs or table names
/// are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct UserId(String);

impl UserId {
    /// Creates a new UserId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the user ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for UserId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for UserId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for UserId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Type-safe wrapper for namespace identifiers.
///
/// Ensures namespace IDs cannot be accidentally used where user IDs or table names
/// are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NamespaceId(String);

impl NamespaceId {
    /// Creates a new NamespaceId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the namespace ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NamespaceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NamespaceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for NamespaceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Type-safe wrapper for storage identifiers.
///
/// Ensures storage IDs cannot be accidentally used where user IDs, namespace IDs,
/// or table names are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StorageId(String);

impl StorageId {
    /// Creates a new StorageId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the storage ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }

    /// Creates a default 'local' storage ID.
    pub fn local() -> Self {
        Self("local".to_string())
    }
}

impl fmt::Display for StorageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for StorageId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for StorageId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for StorageId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Default for StorageId {
    fn default() -> Self {
        Self::local()
    }
}

/// Type-safe wrapper for table names.
///
/// Ensures table names cannot be accidentally used where user IDs or namespace IDs
/// are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TableName(String);

impl TableName {
    /// Creates a new TableName from a string.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the table name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TableName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TableName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for TableName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Enum representing the type of storage backend in KalamDB.
///
/// - **Filesystem**: Local or network filesystem storage
/// - **S3**: Amazon S3 or S3-compatible object storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageType {
    /// Local or network filesystem storage
    Filesystem,
    /// Amazon S3 or S3-compatible object storage
    S3,
}

impl StorageType {
    /// Returns the storage type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageType::Filesystem => "filesystem",
            StorageType::S3 => "s3",
        }
    }
}

impl fmt::Display for StorageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<&str> for StorageType {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "filesystem" => Ok(StorageType::Filesystem),
            "s3" => Ok(StorageType::S3),
            _ => Err(format!("Invalid storage type: {}", s)),
        }
    }
}

/// Enum representing the type of table in KalamDB.
///
/// - **USER**: Per-user table with user_id-based partitioning (key format: `{user_id}:{row_id}`)
/// - **SHARED**: Shared table without user partitioning (key format: `{row_id}`)
/// - **STREAM**: Ephemeral stream table with time-based keys (key format: `{timestamp}:{row_id}`)
/// - **SYSTEM**: Internal KalamDB system tables (e.g., system.namespaces, system.tables)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TableType {
    /// Per-user table with user_id partitioning
    User,
    /// Shared table without user partitioning
    Shared,
    /// Ephemeral stream table with time-based eviction
    Stream,
    /// Internal system table
    System,
}

impl TableType {
    /// Returns the table type as a string (lowercase for column family names).
    pub fn as_str(&self) -> &'static str {
        match self {
            TableType::User => "user",
            TableType::Shared => "shared",
            TableType::Stream => "stream",
            TableType::System => "system",
        }
    }

    /// Parse a table type from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "USER" => Some(TableType::User),
            "SHARED" => Some(TableType::Shared),
            "STREAM" => Some(TableType::Stream),
            "SYSTEM" => Some(TableType::System),
            _ => None,
        }
    }
}

impl fmt::Display for TableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<&str> for TableType {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_uppercase().as_str() {
            "USER" => Ok(TableType::User),
            "SHARED" => Ok(TableType::Shared),
            "STREAM" => Ok(TableType::Stream),
            "SYSTEM" => Ok(TableType::System),
            _ => Err(format!("Invalid table type: {}", s)),
        }
    }
}

/// Storage configuration resolved from system.storages
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConfig {
    storage_id: String,
    storage_type: StorageType,
    base_directory: String,
    shared_tables_template: String,
    user_tables_template: String,
    credentials: Option<String>,
}

impl StorageConfig {
    /// Create a new storage configuration model
    pub fn new(
        storage_id: impl Into<String>,
        storage_type: StorageType,
        base_directory: impl Into<String>,
        shared_tables_template: impl Into<String>,
        user_tables_template: impl Into<String>,
        credentials: Option<String>,
    ) -> Self {
        Self {
            storage_id: storage_id.into(),
            storage_type,
            base_directory: base_directory.into(),
            shared_tables_template: shared_tables_template.into(),
            user_tables_template: user_tables_template.into(),
            credentials,
        }
    }

    pub fn storage_id(&self) -> &str {
        &self.storage_id
    }

    pub fn storage_type(&self) -> StorageType {
        self.storage_type
    }

    pub fn base_directory(&self) -> &str {
        &self.base_directory
    }

    pub fn shared_tables_template(&self) -> &str {
        &self.shared_tables_template
    }

    pub fn user_tables_template(&self) -> &str {
        &self.user_tables_template
    }

    pub fn credentials(&self) -> Option<&str> {
        self.credentials.as_deref()
    }

    pub fn is_s3(&self) -> bool {
        matches!(self.storage_type, StorageType::S3)
    }
}

/// Connection identifier: {user_id}-{unique_conn_id}
///
/// Represents a WebSocket connection from a user to the server.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ConnectionId {
    pub user_id: String,
    pub unique_conn_id: String,
}

impl ConnectionId {
    /// Create a new connection ID
    pub fn new(user_id: String, unique_conn_id: String) -> Self {
        Self {
            user_id,
            unique_conn_id,
        }
    }

    /// Parse from string format: {user_id}-{unique_conn_id}
    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid connection_id format: {}. Expected: {{user_id}}-{{unique_conn_id}}",
                s
            ));
        }
        Ok(Self {
            user_id: parts[0].to_string(),
            unique_conn_id: parts[1].to_string(),
        })
    }

    /// Get user_id component
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get unique_conn_id component
    pub fn unique_conn_id(&self) -> &str {
        &self.unique_conn_id
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.user_id, self.unique_conn_id)
    }
}

/// Live query identifier: {connection_id}-{table_name}-{query_id}
///
/// Represents a live query subscription for a specific table and query.
/// Format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LiveId {
    pub connection_id: ConnectionId,
    pub table_name: String,
    pub query_id: String,
}

impl LiveId {
    /// Create a new live query ID
    pub fn new(connection_id: ConnectionId, table_name: String, query_id: String) -> Self {
        Self {
            connection_id,
            table_name,
            query_id,
        }
    }

    /// Parse from string format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.splitn(4, '-').collect();
        if parts.len() != 4 {
            return Err(format!(
                "Invalid live_id format: {}. Expected: {{user_id}}-{{unique_conn_id}}-{{table_name}}-{{query_id}}",
                s
            ));
        }
        Ok(Self {
            connection_id: ConnectionId {
                user_id: parts[0].to_string(),
                unique_conn_id: parts[1].to_string(),
            },
            table_name: parts[2].to_string(),
            query_id: parts[3].to_string(),
        })
    }

    /// Get connection_id component
    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }

    /// Get table_name component
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Get query_id component
    pub fn query_id(&self) -> &str {
        &self.query_id
    }

    /// Get user_id from connection_id
    pub fn user_id(&self) -> &str {
        &self.connection_id.user_id
    }
}

impl fmt::Display for LiveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            self.connection_id.user_id,
            self.connection_id.unique_conn_id,
            self.table_name,
            self.query_id
        )
    }
}

/// Column default value specification for CREATE TABLE.
///
/// Represents the default value strategy for a column when no value is provided
/// during INSERT operations.
///
/// ## Examples
///
/// ```rust
/// use kalamdb_commons::models::ColumnDefault;
///
/// // No default - column must be specified or NULL
/// let no_default = ColumnDefault::None;
///
/// // Call a function to generate default
/// let now_default = ColumnDefault::FunctionCall("NOW".to_string());
/// let id_default = ColumnDefault::FunctionCall("SNOWFLAKE_ID".to_string());
///
/// // Literal value
/// let literal_default = ColumnDefault::Literal("default_value".to_string());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum ColumnDefault {
    /// No default value specified
    None,
    /// Call a SQL function to generate the default value
    /// Function name should be uppercase (e.g., "NOW", "SNOWFLAKE_ID", "UUID_V7", "ULID", "CURRENT_USER")
    FunctionCall(String),
    /// Use a literal value as default
    /// Value is stored as string and will be cast to the column's data type
    Literal(String),
}

impl ColumnDefault {
    /// Check if a default value is specified
    pub fn is_none(&self) -> bool {
        matches!(self, ColumnDefault::None)
    }

    /// Check if this is a function call
    pub fn is_function(&self) -> bool {
        matches!(self, ColumnDefault::FunctionCall(_))
    }

    /// Check if this is a literal value
    pub fn is_literal(&self) -> bool {
        matches!(self, ColumnDefault::Literal(_))
    }

    /// Get the function name if this is a function call
    pub fn function_name(&self) -> Option<&str> {
        match self {
            ColumnDefault::FunctionCall(name) => Some(name.as_str()),
            _ => None,
        }
    }

    /// Get the literal value if this is a literal
    pub fn literal_value(&self) -> Option<&str> {
        match self {
            ColumnDefault::Literal(value) => Some(value.as_str()),
            _ => None,
        }
    }
}

impl Default for ColumnDefault {
    fn default() -> Self {
        ColumnDefault::None
    }
}

// ===================================
// information_schema.tables Support
// ===================================

/// Complete table definition stored in information_schema_tables CF.
/// Single source of truth for all table metadata following MySQL/PostgreSQL
/// information_schema pattern.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TableDefinition {
    /// Unique table identifier: "{namespace_id}:{table_name}"
    pub table_id: String,
    /// Table name (e.g., "users")
    pub table_name: TableName,
    /// Namespace ID (e.g., "app")
    pub namespace_id: NamespaceId,
    /// Table type (USER, SHARED, STREAM, SYSTEM)
    pub table_type: TableType,
    /// Creation timestamp (Unix milliseconds)
    pub created_at: i64,
    /// Last update timestamp (Unix milliseconds)
    pub updated_at: i64,
    /// Current schema version (incremented on ALTER TABLE)
    pub schema_version: u32,
    /// Storage ID reference (e.g., "local", "s3-prod")
    pub storage_id: StorageId,
    /// Whether to use user-specific storage partitioning
    pub use_user_storage: bool,
    /// Flush policy for write-ahead buffer
    pub flush_policy: Option<FlushPolicyDef>,
    /// Soft-delete retention in hours (for USER tables)
    pub deleted_retention_hours: Option<u32>,
    /// TTL in seconds (for STREAM tables)
    pub ttl_seconds: Option<u64>,
    /// Array of column definitions with ordinal positions
    pub columns: Vec<ColumnDefinition>,
    /// Schema change history for migrations
    pub schema_history: Vec<SchemaVersion>,
}

/// Serializable flush policy definition for TableDefinition JSON storage.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FlushPolicyDef {
    /// Row threshold for flush trigger
    pub row_threshold: Option<u64>,
    /// Time interval in seconds for flush trigger
    pub interval_seconds: Option<u64>,
}

/// Column metadata with ordinal position for SELECT * ordering.
/// Stored within TableDefinition.columns array.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ColumnDefinition {
    /// Column name
    pub column_name: String,
    /// 1-indexed position in CREATE TABLE (preserved for SELECT *)
    pub ordinal_position: u32,
    /// Arrow DataType as string (e.g., "Int64", "Utf8", "Timestamp(Millisecond, None)")
    pub data_type: String,
    /// Whether column allows NULL values
    pub is_nullable: bool,
    /// DEFAULT expression (e.g., "NOW()", "SNOWFLAKE_ID()", "'literal'")
    pub column_default: Option<String>,
    /// Whether this column is the PRIMARY KEY
    pub is_primary_key: bool,
}

/// Schema version record for change tracking and migrations.
/// Stored within TableDefinition.schema_history array.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SchemaVersion {
    /// Schema version number
    pub version: u32,
    /// When this version was created (Unix milliseconds)
    pub created_at: i64,
    /// Human-readable description of changes (e.g., "Initial schema", "Added email column")
    pub changes: String,
    /// Full Arrow schema as JSON for this version
    pub arrow_schema_json: String,
}

impl TableDefinition {
    /// Extract column definitions from Arrow Schema and column_defaults map.
    /// Assigns ordinal_position based on field order (1-indexed).
    ///
    /// # Arguments
    /// * `schema` - Arrow schema with field definitions
    /// * `column_defaults` - Map of column_name -> ColumnDefault
    /// * `primary_key_column` - Optional primary key column name
    ///
    /// # Returns
    /// Vector of ColumnDefinition with ordinal positions assigned
    #[cfg(feature = "serde")]
    pub fn extract_columns_from_schema(
        schema: &arrow_schema::Schema,
        column_defaults: &std::collections::HashMap<String, ColumnDefault>,
        primary_key_column: Option<&str>,
    ) -> Vec<ColumnDefinition> {
        schema
            .fields()
            .iter()
            .enumerate()
            .map(|(idx, field)| {
                let column_name = field.name().to_string();
                let is_primary_key = primary_key_column
                    .map(|pk| pk == column_name.as_str())
                    .unwrap_or(false);

                // Get default value if specified
                let column_default = column_defaults.get(&column_name).and_then(|def| match def {
                    ColumnDefault::None => None,
                    ColumnDefault::FunctionCall(func) => Some(format!("{}()", func)),
                    ColumnDefault::Literal(lit) => Some(lit.clone()),
                });

                ColumnDefinition {
                    column_name,
                    ordinal_position: (idx + 1) as u32, // 1-indexed
                    data_type: format!("{:?}", field.data_type()), // e.g., "Int64", "Utf8"
                    is_nullable: field.is_nullable(),
                    column_default,
                    is_primary_key,
                }
            })
            .collect()
    }

    /// Serialize Arrow Schema to JSON string for schema_history.
    ///
    /// # Arguments
    /// * `schema` - Arrow schema to serialize
    ///
    /// # Returns
    /// JSON string representation of the schema
    #[cfg(feature = "serde")]
    pub fn serialize_arrow_schema(
        schema: &arrow_schema::Schema,
    ) -> Result<String, serde_json::Error> {
        // Convert schema to JSON-compatible structure
        #[cfg_attr(feature = "serde", derive(Serialize))]
        struct SchemaJson {
            fields: Vec<FieldJson>,
        }

        #[cfg_attr(feature = "serde", derive(Serialize))]
        struct FieldJson {
            name: String,
            data_type: String,
            nullable: bool,
        }

        let schema_json = SchemaJson {
            fields: schema
                .fields()
                .iter()
                .map(|f| FieldJson {
                    name: f.name().to_string(),
                    data_type: format!("{:?}", f.data_type()),
                    nullable: f.is_nullable(),
                })
                .collect(),
        };

        serde_json::to_string(&schema_json)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_user_id() {
        let user_id = UserId::new("user_123");
        assert_eq!(user_id.as_str(), "user_123");
        assert_eq!(user_id.to_string(), "user_123");
    }

    #[test]
    fn test_namespace_id() {
        let namespace_id = NamespaceId::new("default");
        assert_eq!(namespace_id.as_str(), "default");
        assert_eq!(namespace_id.to_string(), "default");
    }

    #[test]
    fn test_table_name() {
        let table_name = TableName::new("conversations");
        assert_eq!(table_name.as_str(), "conversations");
        assert_eq!(table_name.to_string(), "conversations");
    }

    #[test]
    fn test_table_type() {
        assert_eq!(TableType::User.as_str(), "user");
        assert_eq!(TableType::Shared.as_str(), "shared");
        assert_eq!(TableType::Stream.as_str(), "stream");

        assert_eq!(TableType::try_from("USER").unwrap(), TableType::User);
        assert_eq!(TableType::try_from("user").unwrap(), TableType::User);
        assert_eq!(TableType::try_from("SHARED").unwrap(), TableType::Shared);
        assert_eq!(TableType::try_from("STREAM").unwrap(), TableType::Stream);
        assert!(TableType::try_from("INVALID").is_err());
    }

    #[test]
    fn test_storage_config_accessors() {
        let config = StorageConfig::new(
            "s3-prod",
            StorageType::S3,
            "s3://bucket",
            "{namespace}/{table}",
            "{namespace}/{table}/{userId}",
            Some("{\"access_key\":\"A\",\"secret_key\":\"B\"}".to_string()),
        );

        assert_eq!(config.storage_id(), "s3-prod");
        assert!(config.is_s3());
        assert_eq!(
            config.credentials(),
            Some("{\"access_key\":\"A\",\"secret_key\":\"B\"}")
        );
    }

    #[test]
    fn test_connection_id() {
        let conn_id = ConnectionId::new("user123".to_string(), "conn_abc".to_string());
        assert_eq!(conn_id.user_id(), "user123");
        assert_eq!(conn_id.unique_conn_id(), "conn_abc");
        assert_eq!(conn_id.to_string(), "user123-conn_abc");
    }

    #[test]
    fn test_connection_id_from_string() {
        let conn_id = ConnectionId::from_string("user456-conn_xyz").unwrap();
        assert_eq!(conn_id.user_id(), "user456");
        assert_eq!(conn_id.unique_conn_id(), "conn_xyz");
    }

    #[test]
    fn test_connection_id_from_string_invalid() {
        assert!(ConnectionId::from_string("invalid").is_err());
        assert!(ConnectionId::from_string("").is_err());
    }

    #[test]
    fn test_live_id() {
        let conn_id = ConnectionId::new("user123".to_string(), "conn_abc".to_string());
        let live_id = LiveId::new(conn_id, "messages".to_string(), "q1".to_string());

        assert_eq!(live_id.user_id(), "user123");
        assert_eq!(live_id.connection_id().unique_conn_id(), "conn_abc");
        assert_eq!(live_id.table_name(), "messages");
        assert_eq!(live_id.query_id(), "q1");
        assert_eq!(live_id.to_string(), "user123-conn_abc-messages-q1");
    }

    #[test]
    fn test_live_id_from_string() {
        let live_id = LiveId::from_string("user123-conn_abc-messages-q1").unwrap();
        assert_eq!(live_id.user_id(), "user123");
        assert_eq!(live_id.table_name(), "messages");
        assert_eq!(live_id.query_id(), "q1");
    }

    #[test]
    fn test_live_id_from_string_invalid() {
        assert!(LiveId::from_string("invalid-format").is_err());
        assert!(LiveId::from_string("user-conn-table").is_err());
        assert!(LiveId::from_string("").is_err());
    }

    #[test]
    fn test_column_default_none() {
        let default = ColumnDefault::None;
        assert!(default.is_none());
        assert!(!default.is_function());
        assert!(!default.is_literal());
        assert_eq!(default.function_name(), None);
        assert_eq!(default.literal_value(), None);
    }

    #[test]
    fn test_column_default_function() {
        let default = ColumnDefault::FunctionCall("NOW".to_string());
        assert!(!default.is_none());
        assert!(default.is_function());
        assert!(!default.is_literal());
        assert_eq!(default.function_name(), Some("NOW"));
        assert_eq!(default.literal_value(), None);
    }

    #[test]
    fn test_column_default_literal() {
        let default = ColumnDefault::Literal("42".to_string());
        assert!(!default.is_none());
        assert!(!default.is_function());
        assert!(default.is_literal());
        assert_eq!(default.function_name(), None);
        assert_eq!(default.literal_value(), Some("42"));
    }

    #[test]
    fn test_column_default_default_trait() {
        let default = ColumnDefault::default();
        assert!(default.is_none());
    }
}
