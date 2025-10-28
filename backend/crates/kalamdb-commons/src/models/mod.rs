//! Type-safe wrapper types for KalamDB identifiers and enums.
//!
//! This module provides newtype wrappers around String to enforce type safety
//! at compile time, preventing accidental mixing of user IDs, namespace IDs,
//! and table names.
//!
//! ## System Table Models
//!
//! The `system` submodule contains the SINGLE SOURCE OF TRUTH for all system table models.
//! Import from `kalamdb_commons::system::*` to use these models.
//!
//! ## Examples
//!
//! ```rust
//! use kalamdb_commons::models::{UserId, NamespaceId, TableName};
//! use kalamdb_commons::system::{User, Job, LiveQuery};
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

mod namespace_id;
mod storage_id;
pub mod system;
mod table_name;
mod user_id;

pub use namespace_id::NamespaceId;
pub use storage_id::StorageId;
pub use table_name::TableName;
pub use user_id::UserId;

// Re-export everything else from the old models.rs file
// TODO: Split these into separate files as well
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Enum representing the type of storage backend in KalamDB.
///
/// - **Filesystem**: Local or network filesystem storage
/// - **S3**: Amazon S3 or S3-compatible object storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StorageType {
    /// Local or network filesystem storage
    Filesystem,
    /// Amazon S3 or S3-compatible object storage
    S3,
}

impl fmt::Display for StorageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageType::Filesystem => write!(f, "filesystem"),
            StorageType::S3 => write!(f, "s3"),
        }
    }
}

impl From<&str> for StorageType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "s3" => StorageType::S3,
            _ => StorageType::Filesystem,
        }
    }
}

impl From<String> for StorageType {
    fn from(s: String) -> Self {
        StorageType::from(s.as_str())
    }
}

/// Enum representing the type of table in KalamDB.
///
/// - **User**: Per-user tables with user-specific partitioning (e.g., `user_123/conversations`)
/// - **Shared**: Shared tables accessible across all users (e.g., `categories`)
/// - **Stream**: Event stream tables with TTL-based eviction (e.g., `chat_events`)
/// - **System**: Internal system metadata tables (e.g., `information_schema.tables`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum TableType {
    /// Per-user tables with user-specific partitioning
    User,
    /// Shared tables accessible across all users
    Shared,
    /// Event stream tables with TTL-based eviction
    Stream,
    /// Internal system metadata tables
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

impl From<&str> for TableType {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "USER" => TableType::User,
            "SHARED" => TableType::Shared,
            "STREAM" => TableType::Stream,
            "SYSTEM" => TableType::System,
            _ => TableType::User, // Default to User
        }
    }
}

impl From<String> for TableType {
    fn from(s: String) -> Self {
        TableType::from(s.as_str())
    }
}

/// Enum representing job execution status.
///
/// - **Running**: Job is currently executing
/// - **Completed**: Job finished successfully
/// - **Failed**: Job encountered an error
/// - **Cancelled**: Job was cancelled by user or system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum JobStatus {
    /// Job is currently executing
    Running,
    /// Job finished successfully
    Completed,
    /// Job encountered an error
    Failed,
    /// Job was cancelled
    Cancelled,
}

impl JobStatus {
    /// Returns the job status as a lowercase string.
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    /// Parse a job status from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "running" => Some(JobStatus::Running),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            "cancelled" => Some(JobStatus::Cancelled),
            _ => None,
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for JobStatus {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Failed, // Default to failed for safety
        }
    }
}

impl From<String> for JobStatus {
    fn from(s: String) -> Self {
        JobStatus::from(s.as_str())
    }
}

/// Enum representing job types.
///
/// - **Flush**: Flush table data from RocksDB to Parquet
/// - **Compact**: Compact Parquet files
/// - **Cleanup**: Clean up old data
/// - **Backup**: Backup namespace data
/// - **Restore**: Restore namespace from backup
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum JobType {
    /// Flush table data from RocksDB to Parquet
    Flush,
    /// Compact Parquet files
    Compact,
    /// Clean up old data
    Cleanup,
    /// Backup namespace data
    Backup,
    /// Restore namespace from backup
    Restore,
}

impl JobType {
    /// Returns the job type as a lowercase string.
    pub fn as_str(&self) -> &'static str {
        match self {
            JobType::Flush => "flush",
            JobType::Compact => "compact",
            JobType::Cleanup => "cleanup",
            JobType::Backup => "backup",
            JobType::Restore => "restore",
        }
    }

    /// Parse a job type from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "flush" => Some(JobType::Flush),
            "compact" => Some(JobType::Compact),
            "cleanup" => Some(JobType::Cleanup),
            "backup" => Some(JobType::Backup),
            "restore" => Some(JobType::Restore),
            _ => None,
        }
    }
}

impl fmt::Display for JobType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for JobType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "flush" => JobType::Flush,
            "compact" => JobType::Compact,
            "cleanup" => JobType::Cleanup,
            "backup" => JobType::Backup,
            "restore" => JobType::Restore,
            _ => JobType::Flush, // Default to flush
        }
    }
}

impl From<String> for JobType {
    fn from(s: String) -> Self {
        JobType::from(s.as_str())
    }
}

/// Enum representing user roles in KalamDB.
///
/// - **User**: Regular user with standard permissions
/// - **Service**: Service account for automated tasks
/// - **Dba**: Database administrator with elevated privileges
/// - **System**: Internal system user (highest privileges)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum Role {
    /// Regular user with standard permissions
    User,
    /// Service account for automated tasks
    Service,
    /// Database administrator with elevated privileges
    Dba,
    /// Internal system user (highest privileges)
    System,
}

impl Role {
    /// Returns the role as a lowercase string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Service => "service",
            Role::Dba => "dba",
            Role::System => "system",
        }
    }

    /// Parse a role from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "user" => Some(Role::User),
            "service" => Some(Role::Service),
            "dba" => Some(Role::Dba),
            "system" => Some(Role::System),
            _ => None,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "user" => Role::User,
            "service" => Role::Service,
            "dba" => Role::Dba,
            "system" => Role::System,
            _ => Role::User, // Default to user for safety
        }
    }
}

impl From<String> for Role {
    fn from(s: String) -> Self {
        Role::from(s.as_str())
    }
}

/// Enum representing authentication types in KalamDB.
///
/// - **Password**: Traditional username/password authentication
/// - **OAuth**: OAuth 2.0 authentication
/// - **Internal**: Internal system authentication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum AuthType {
    /// Traditional username/password authentication
    Password,
    /// OAuth 2.0 authentication
    OAuth,
    /// Internal system authentication
    Internal,
}

impl AuthType {
    /// Returns the auth type as a lowercase string.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthType::Password => "password",
            AuthType::OAuth => "oauth",
            AuthType::Internal => "internal",
        }
    }

    /// Parse an auth type from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "password" => Some(AuthType::Password),
            "oauth" => Some(AuthType::OAuth),
            "internal" => Some(AuthType::Internal),
            _ => None,
        }
    }
}

impl fmt::Display for AuthType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for AuthType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "password" => AuthType::Password,
            "oauth" => AuthType::OAuth,
            "internal" => AuthType::Internal,
            _ => AuthType::Password, // Default to password for safety
        }
    }
}

impl From<String> for AuthType {
    fn from(s: String) -> Self {
        AuthType::from(s.as_str())
    }
}

/// Enum representing storage mode preferences for users.
///
/// - **Table**: User prefers table-based storage partitioning
/// - **Region**: User prefers region-based storage partitioning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum StorageMode {
    /// Table-based storage partitioning
    Table,
    /// Region-based storage partitioning
    Region,
}

impl StorageMode {
    /// Returns the storage mode as a lowercase string.
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageMode::Table => "table",
            StorageMode::Region => "region",
        }
    }

    /// Parse a storage mode from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "table" => Some(StorageMode::Table),
            "region" => Some(StorageMode::Region),
            _ => None,
        }
    }
}

impl fmt::Display for StorageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for StorageMode {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "table" => StorageMode::Table,
            "region" => StorageMode::Region,
            _ => StorageMode::Table, // Default to table for safety
        }
    }
}

impl From<String> for StorageMode {
    fn from(s: String) -> Self {
        StorageMode::from(s.as_str())
    }
}

/// Enum representing table access control in KalamDB.
///
/// - **Public**: Table accessible by all authenticated users
/// - **Private**: Table accessible only by owner
/// - **Restricted**: Table accessible by specific users/roles (requires permissions table)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum TableAccess {
    /// Table accessible by all authenticated users
    Public,
    /// Table accessible only by owner
    Private,
    /// Table accessible by specific users/roles (requires permissions table)
    Restricted,
}

impl TableAccess {
    /// Returns the table access as a lowercase string.
    pub fn as_str(&self) -> &'static str {
        match self {
            TableAccess::Public => "public",
            TableAccess::Private => "private",
            TableAccess::Restricted => "restricted",
        }
    }

    /// Parse a table access from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "public" => Some(TableAccess::Public),
            "private" => Some(TableAccess::Private),
            "restricted" => Some(TableAccess::Restricted),
            _ => None,
        }
    }
}

impl fmt::Display for TableAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for TableAccess {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "public" => TableAccess::Public,
            "private" => TableAccess::Private,
            "restricted" => TableAccess::Restricted,
            _ => TableAccess::Private, // Default to private for safety
        }
    }
}

impl From<String> for TableAccess {
    fn from(s: String) -> Self {
        TableAccess::from(s.as_str())
    }
}

/// Storage configuration for a KalamDB storage backend.
///
/// Defines the connection details and path templates for storing table data.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StorageConfig {
    /// Unique storage identifier (e.g., "local", "s3-prod")
    pub storage_id: StorageId,
    /// Human-readable name (e.g., "Local Filesystem")
    pub storage_name: String,
    /// Optional description
    pub description: Option<String>,
    /// Type of storage (Filesystem or S3)
    pub storage_type: StorageType,
    /// Base directory or S3 bucket path
    pub base_directory: String,
    /// Optional credentials (e.g., AWS access key JSON)
    pub credentials: Option<String>,
    /// Path template for shared tables: `{base_directory}/{shared_tables_template}/{namespace}/{table_name}`
    pub shared_tables_template: String,
    /// Path template for user tables: `{base_directory}/{user_tables_template}/{namespace}/{table_name}/{user_id}`
    pub user_tables_template: String,
    /// Creation timestamp (Unix milliseconds)
    pub created_at: i64,
    /// Last update timestamp (Unix milliseconds)
    pub updated_at: i64,
}

impl StorageConfig {
    /// Create a new storage configuration
    pub fn new(
        storage_id: impl Into<StorageId>,
        storage_type: StorageType,
        base_directory: impl Into<String>,
        shared_tables_template: impl Into<String>,
        user_tables_template: impl Into<String>,
        credentials: Option<String>,
    ) -> Self {
        Self {
            storage_id: storage_id.into(),
            storage_name: String::new(),
            description: None,
            storage_type,
            base_directory: base_directory.into(),
            credentials,
            shared_tables_template: shared_tables_template.into(),
            user_tables_template: user_tables_template.into(),
            created_at: 0,
            updated_at: 0,
        }
    }

    /// Get storage ID
    pub fn storage_id(&self) -> &StorageId {
        &self.storage_id
    }

    /// Get storage type
    pub fn storage_type(&self) -> StorageType {
        self.storage_type
    }

    /// Get base directory
    pub fn base_directory(&self) -> &str {
        &self.base_directory
    }

    /// Get shared tables template
    pub fn shared_tables_template(&self) -> &str {
        &self.shared_tables_template
    }

    /// Get user tables template
    pub fn user_tables_template(&self) -> &str {
        &self.user_tables_template
    }

    /// Get credentials
    pub fn credentials(&self) -> Option<&str> {
        self.credentials.as_deref()
    }

    /// Check if storage is S3
    pub fn is_s3(&self) -> bool {
        matches!(self.storage_type, StorageType::S3)
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_id: StorageId::local(),
            storage_name: "Local Filesystem".to_string(),
            description: Some("Default local filesystem storage".to_string()),
            storage_type: StorageType::Filesystem,
            base_directory: "./data".to_string(),
            credentials: None,
            shared_tables_template: "shared".to_string(),
            user_tables_template: "users".to_string(),
            created_at: 0,
            updated_at: 0,
        }
    }
}

/// Unique identifier for WebSocket connections.
///
/// Used to track live query subscriptions and route notifications.
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

/// Unique identifier for live query subscriptions.
///
/// Used to track and manage individual live query subscriptions.
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

/// Default column value specification.
///
/// Used in CREATE TABLE statements to specify default values for columns.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ColumnDefault {
    /// No default value
    None,
    /// Function call (e.g., "NOW", "UUID_V7", "SNOWFLAKE_ID")
    FunctionCall(String),
    /// Literal value (e.g., "0", "'hello'", "true")
    Literal(String),
}

impl fmt::Display for ColumnDefault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColumnDefault::None => write!(f, "NONE"),
            ColumnDefault::FunctionCall(func) => write!(f, "{}()", func),
            ColumnDefault::Literal(lit) => write!(f, "{}", lit),
        }
    }
}

/// Complete table definition including schema, metadata, and history.
///
/// This is the canonical source of truth for table structure in KalamDB.
/// Stored in `information_schema.tables` table.
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

/// Flush policy definition for table write-ahead buffer.
///
/// Controls when data is flushed from RocksDB to Parquet.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FlushPolicyDef {
    /// Row threshold for flush trigger
    pub row_threshold: Option<u64>,
    /// Time interval in seconds for flush trigger
    pub interval_seconds: Option<u64>,
}

/// Column definition in a table schema.
///
/// Defines the structure and constraints of a single column.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ColumnDefinition {
    /// Column name
    pub column_name: String,
    /// Ordinal position (1-indexed, preserved across schema changes)
    pub ordinal_position: u32,
    /// Data type (e.g., "Int64", "Utf8", "Boolean")
    pub data_type: String,
    /// Whether column allows NULL values
    pub is_nullable: bool,
    /// Default value specification
    pub column_default: Option<String>,
    /// Whether column is the primary key
    pub is_primary_key: bool,
}

/// Schema version entry in table history.
///
/// Tracks schema evolution over time for migration and rollback.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SchemaVersion {
    /// Schema version number (incremented on ALTER TABLE)
    pub version: u32,
    /// Creation timestamp (Unix milliseconds)
    pub created_at: i64,
    /// Human-readable description of changes
    pub changes: String,
    /// Arrow schema as JSON string
    pub arrow_schema_json: String,
}

impl TableDefinition {
    /// Extract column definitions from an Arrow schema.
    ///
    /// # Arguments
    /// * `schema` - Arrow schema to extract columns from
    /// * `column_defaults` - Map of column names to default values
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
    fn test_storage_type_conversion() {
        assert_eq!(StorageType::from("filesystem"), StorageType::Filesystem);
        assert_eq!(StorageType::from("s3"), StorageType::S3);
        assert_eq!(StorageType::from("S3"), StorageType::S3);
        assert_eq!(StorageType::from("unknown"), StorageType::Filesystem);
    }

    #[test]
    fn test_table_type_conversion() {
        assert_eq!(TableType::from("user"), TableType::User);
        assert_eq!(TableType::from("USER"), TableType::User);
        assert_eq!(TableType::from("shared"), TableType::Shared);
        assert_eq!(TableType::from("stream"), TableType::Stream);
        assert_eq!(TableType::from("system"), TableType::System);
    }

    #[test]
    fn test_storage_id_default() {
        let id = StorageId::default();
        assert_eq!(id.as_str(), "local");
    }
}

