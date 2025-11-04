//! System table models for KalamDB.
//!
//! **CRITICAL**: This module is the SINGLE SOURCE OF TRUTH for all system table models.
//! DO NOT create duplicate model definitions elsewhere in the codebase.
//!
//! This module contains strongly-typed models for all system tables:
//! - `User`: System users (authentication, authorization)
//! - `Job`: Background jobs (flush, retention, cleanup)
//! - `Namespace`: Database namespaces
//! - `SystemTable`: Table metadata registry
//! - `LiveQuery`: Active WebSocket subscriptions
//! - `InformationSchemaTable`: SQL standard table metadata
//! - `UserTableCounter`: Per-user table flush tracking
//!
//! All system table models are serialized with bincode for performance.
//! These models are re-exported at the crate root for easy access.
//!
//! ## Architecture
//!
//! - **Location**: `kalamdb-commons/src/models/system.rs` (this file)
//! - **Purpose**: Canonical definitions for system table rows
//! - **Serialization**: Bincode (for RocksDB storage) + Serde JSON (for API responses)
//! - **Usage**: Import from `kalamdb_commons::system::*`
//!
//! ## Example
//!
//! ```rust
//! use kalamdb_commons::system::{User, Job, LiveQuery};
//! use kalamdb_commons::{UserId, Role, AuthType, StorageMode, StorageId, JobType, JobStatus, NamespaceId, TableName};
//!
//! let user = User {
//!     id: UserId::new("u_123"),
//!     username: "alice".to_string(),
//!     password_hash: "$2b$12$...".to_string(),
//!     role: Role::User,
//!     email: Some("alice@example.com".to_string()),
//!     auth_type: AuthType::Password,
//!     auth_data: None,
//!     api_key: None,
//!     storage_mode: StorageMode::Table,
//!     storage_id: Some(StorageId::new("storage_1")),
//!     created_at: 1730000000000,
//!     updated_at: 1730000000000,
//!     last_seen: None,
//!     deleted_at: None,
//! };
//! ```
use super::{
    JobId, LiveQueryId, NamespaceId, NodeId, Role, StorageId, TableId, TableName, UserId, UserName,
};
use crate::models::{
    schemas::TableType, AuthType, JobStatus, JobType, StorageMode, TableAccess,
};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
/// - `email`: Optional email address
/// - `auth_type`: Authentication method (Password, OAuth, Internal)
/// - `auth_data`: JSON blob for auth-specific data (e.g., OAuth provider/subject)
/// - `storage_mode`: Preferred storage partitioning mode (Table, Region)
/// - `storage_id`: Optional preferred storage configuration ID
/// - `created_at`: Unix timestamp in milliseconds when user was created
/// - `updated_at`: Unix timestamp in milliseconds when user was last modified
/// - `last_seen`: Optional Unix timestamp in milliseconds of last activity
/// - `deleted_at`: Optional Unix timestamp in milliseconds for soft delete
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::system::User;
/// use kalamdb_commons::{UserId, Role, AuthType, StorageMode, StorageId};
///
/// let user = User {
///     id: UserId::new("u_123456"),
///     username: "alice".to_string(),
///     password_hash: "$2b$12$...".to_string(),
///     role: Role::User,
///     email: Some("alice@example.com".to_string()),
///     auth_type: AuthType::Password,
///     auth_data: None,
///     storage_mode: StorageMode::Table,
///     storage_id: Some(StorageId::new("storage_1")),
///     created_at: 1730000000000,
///     updated_at: 1730000000000,
///     last_seen: None,
///     deleted_at: None,
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct User {
    pub id: UserId,
    pub username: UserName,
    pub password_hash: String,
    pub role: Role,
    pub email: Option<String>,
    pub auth_type: AuthType,
    pub auth_data: Option<String>, // JSON blob for OAuth provider/subject
    pub storage_mode: StorageMode, // Preferred storage partitioning mode
    pub storage_id: Option<StorageId>, // Optional preferred storage configuration
    pub created_at: i64,           // Unix timestamp in milliseconds
    pub updated_at: i64,           // Unix timestamp in milliseconds
    pub last_seen: Option<i64>,    // Unix timestamp in milliseconds (daily granularity)
    pub deleted_at: Option<i64>,   // Unix timestamp in milliseconds for soft delete
}

/// Job entity for system.jobs table.
///
/// Represents a background job (flush, retention, cleanup, etc.).
///
/// ## Fields
/// - `job_id`: Unique job identifier (e.g., "job_123456")
/// - `job_type`: Type of job (Flush, Compact, Cleanup, Backup, Restore)
/// - `namespace_id`: Namespace this job operates on
/// - `table_name`: Optional table name for table-specific jobs
/// - `status`: Job status (Running, Completed, Failed, Cancelled)
/// - `parameters`: Optional JSON array of job parameters
/// - `result`: Optional result message (for completed jobs)
/// - `trace`: Optional stack trace (for failed jobs)
/// - `memory_used`: Optional memory usage in bytes
/// - `cpu_used`: Optional CPU time in microseconds
/// - `created_at`: Unix timestamp in milliseconds when job was created
/// - `started_at`: Optional Unix timestamp in milliseconds when job started
/// - `completed_at`: Optional Unix timestamp in milliseconds when job completed
/// - `node_id`: Node/server that owns this job
/// - `error_message`: Optional error message (for failed jobs)
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::system::Job;
/// use kalamdb_commons::{NamespaceId, TableName, JobType, JobStatus};
///
/// let job = Job {
///     job_id: "job_123456".to_string(),
///     job_type: JobType::Flush,
///     namespace_id: NamespaceId::new("default"),
///     table_name: Some(TableName::new("events")),
///     status: JobStatus::Running,
///     parameters: None,
///     result: None,
///     trace: None,
///     memory_used: None,
///     cpu_used: None,
///     created_at: 1730000000000,
///     started_at: Some(1730000000000),
///     completed_at: None,
///     node_id: "server-01".to_string(),
///     error_message: None,
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct Job {
    pub job_id: JobId,
    pub job_type: JobType,
    pub namespace_id: NamespaceId,
    pub table_name: Option<TableName>,
    pub status: JobStatus,
    pub parameters: Option<String>, // JSON object (migrated from array)
    pub message: Option<String>,    // Unified field replacing result/error_message
    pub exception_trace: Option<String>, // Full stack trace on failures
    pub idempotency_key: Option<String>, // For preventing duplicate jobs
    pub retry_count: u8,            // Number of retries attempted (default 0)
    pub max_retries: u8,            // Maximum retries allowed (default 3)
    pub memory_used: Option<i64>,   // bytes
    pub cpu_used: Option<i64>,      // microseconds
    pub created_at: i64,            // Unix timestamp in milliseconds
    pub updated_at: i64,            // Unix timestamp in milliseconds
    pub started_at: Option<i64>,    // Unix timestamp in milliseconds
    pub finished_at: Option<i64>,   // Unix timestamp in milliseconds (renamed from completed_at)
    #[bincode(with_serde)]
    pub node_id: NodeId,
    pub queue: Option<String>,      // Queue name (future use)
    pub priority: Option<i32>,      // Priority value (future use)
    
    // Legacy fields (deprecated, kept for backward compatibility)
    #[deprecated(since = "0.9.0", note = "Use message field instead")]
    pub result: Option<String>,
    #[deprecated(since = "0.9.0", note = "Use message field instead")]
    pub error_message: Option<String>,
    #[deprecated(since = "0.9.0", note = "Use exception_trace field instead")]
    pub trace: Option<String>,
}

impl Job {
    /// Create a new job with New status
    #[allow(deprecated)]
    pub fn new(
        job_id: JobId,
        job_type: JobType,
        namespace_id: NamespaceId,
        node_id: NodeId,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            job_id,
            job_type,
            namespace_id,
            table_name: None,
            status: JobStatus::New,
            parameters: None,
            message: None,
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            finished_at: None,
            node_id,
            queue: None,
            priority: None,
            // Legacy fields
            result: None,
            error_message: None,
            trace: None,
        }
    }

    /// Mark job as completed with result message
    #[allow(deprecated)]
    pub fn complete(mut self, message: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Completed;
        self.updated_at = now;
        self.finished_at = Some(now);
        if self.started_at.is_none() {
            self.started_at = Some(self.created_at);
        }
        self.message = message;
        self.exception_trace = None; // Clear any previous error trace
        // Legacy compatibility
        self.result = self.message.clone();
        self
    }

    /// Mark job as failed with error message and optional stack trace
    #[allow(deprecated)]
    pub fn fail(mut self, error_message: String, exception_trace: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Failed;
        self.updated_at = now;
        self.finished_at = Some(now);
        if self.started_at.is_none() {
            self.started_at = Some(self.created_at);
        }
        self.message = Some(error_message.clone());
        self.exception_trace = exception_trace.clone();
        // Legacy compatibility
        self.error_message = Some(error_message);
        self.trace = exception_trace;
        self
    }

    /// Mark job as cancelled
    pub fn cancel(mut self) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Cancelled;
        self.updated_at = now;
        self.finished_at = Some(now);
        self
    }

    /// Queue the job (transition from New to Queued)
    pub fn queue(mut self) -> Self {
        self.status = JobStatus::Queued;
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self
    }

    /// Start the job (transition to Running)
    pub fn start(mut self) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Running;
        self.updated_at = now;
        self.started_at = Some(now);
        self
    }

    /// Mark job for retry (increment retry_count, set status to Retrying)
    #[allow(deprecated)]
    pub fn retry(mut self, error_message: String, exception_trace: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.retry_count += 1;
        self.status = JobStatus::Retrying;
        self.updated_at = now;
        self.message = Some(error_message.clone());
        self.exception_trace = exception_trace.clone();
        // Legacy compatibility
        self.error_message = Some(error_message);
        self.trace = exception_trace;
        self
    }

    /// Check if job can be retried
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Set table name
    pub fn with_table_name(mut self, table_name: TableName) -> Self {
        self.table_name = Some(table_name);
        self
    }

    /// Set parameters (JSON object)
    pub fn with_parameters(mut self, parameters: String) -> Self {
        self.parameters = Some(parameters);
        self
    }

    /// Set idempotency key for duplicate prevention
    pub fn with_idempotency_key(mut self, key: String) -> Self {
        self.idempotency_key = Some(key);
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u8) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set queue name
    pub fn with_queue(mut self, queue: String) -> Self {
        self.queue = Some(queue);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Set resource metrics (memory and CPU usage)
    pub fn with_metrics(mut self, memory_used: Option<i64>, cpu_used: Option<i64>) -> Self {
        self.memory_used = memory_used;
        self.cpu_used = cpu_used;
        self
    }

    /// Set trace information (deprecated - use exception_trace)
    #[deprecated(since = "0.9.0", note = "Use with_exception_trace instead")]
    #[allow(deprecated)]
    pub fn with_trace(mut self, trace: String) -> Self {
        self.exception_trace = Some(trace.clone());
        self.trace = Some(trace);
        self
    }

    /// Set exception trace
    #[allow(deprecated)]
    pub fn with_exception_trace(mut self, trace: String) -> Self {
        self.exception_trace = Some(trace.clone());
        self.trace = Some(trace);
        self
    }
}

/// Options for job creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOptions {
    /// Maximum number of retries (default: 3)
    pub max_retries: Option<u8>,
    /// Queue name for job routing (future use)
    pub queue: Option<String>,
    /// Priority value (higher = more priority, future use)
    pub priority: Option<i32>,
    /// Idempotency key to prevent duplicate job creation
    pub idempotency_key: Option<String>,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            max_retries: Some(3),
            queue: None,
            priority: None,
            idempotency_key: None,
        }
    }
}

/// Filter criteria for job queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobFilter {
    /// Filter by job type
    pub job_type: Option<JobType>,
    /// Filter by job status
    pub status: Option<JobStatus>,
    /// Filter by namespace
    pub namespace_id: Option<NamespaceId>,
    /// Filter by table name
    pub table_name: Option<TableName>,
    /// Filter by idempotency key
    pub idempotency_key: Option<String>,
    /// Limit number of results
    pub limit: Option<usize>,
    /// Start from created_at timestamp (inclusive)
    pub created_after: Option<i64>,
    /// End at created_at timestamp (exclusive)
    pub created_before: Option<i64>,
}

impl Default for JobFilter {
    fn default() -> Self {
        Self {
            job_type: None,
            status: None,
            namespace_id: None,
            table_name: None,
            idempotency_key: None,
            limit: Some(100),
            created_after: None,
            created_before: None,
        }
    }
}

/// Audit log entry for administrative actions.
///
/// Captures who performed an action, what they targeted, and contextual metadata.
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct AuditLogEntry {
    pub audit_id: crate::models::AuditLogId,
    pub timestamp: i64,
    pub actor_user_id: UserId,
    pub actor_username: UserName,
    pub action: String,
    pub target: String,
    pub details: Option<String>,    // JSON blob for additional context
    pub ip_address: Option<String>, // Connection source (if available)
}

/// Namespace entity for system.namespaces table.
///
/// Represents a database namespace for data isolation.
///
/// ## Fields
/// - `namespace_id`: Unique namespace identifier
/// - `name`: Namespace name (e.g., "default", "production")
/// - `created_at`: Unix timestamp in milliseconds when namespace was created
/// - `options`: Optional JSON configuration
/// - `table_count`: Number of tables in this namespace
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::system::Namespace;
/// use kalamdb_commons::{NamespaceId, UserId};
///
/// let namespace = Namespace {
///     namespace_id: NamespaceId::new("default"),
///     name: "default".to_string(),
///     created_at: 1730000000000,
///     options: Some("{}".to_string()),
///     table_count: 0,
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct Namespace {
    pub namespace_id: NamespaceId,
    pub name: String,
    pub created_at: i64,         // Unix timestamp in milliseconds
    pub options: Option<String>, // JSON configuration
    pub table_count: i32,
}

impl Namespace {
    /// Create a new namespace with default values
    ///
    /// # Arguments
    /// * `name` - Namespace identifier
    ///
    /// # Example
    /// ```
    /// use kalamdb_commons::{system::Namespace, NamespaceId};
    ///
    /// let namespace = Namespace::new("app");
    /// assert_eq!(namespace.name, "app");
    /// assert_eq!(namespace.table_count, 0);
    /// ```
    pub fn new(name: impl Into<String>) -> Self {
        let name_str = name.into();
        Self {
            namespace_id: NamespaceId::new(&name_str),
            name: name_str,
            created_at: chrono::Utc::now().timestamp_millis(),
            options: Some("{}".to_string()),
            table_count: 0,
        }
    }

    /// Validate namespace name format
    ///
    /// Name must match regex: ^[a-z][a-z0-9_]*$ (lowercase, start with letter)
    /// Name cannot be "system" (reserved)
    ///
    /// # Example
    /// ```
    /// use kalamdb_commons::system::Namespace;
    ///
    /// assert!(Namespace::validate_name("app").is_ok());
    /// assert!(Namespace::validate_name("analytics_db").is_ok());
    /// assert!(Namespace::validate_name("system").is_err());
    /// assert!(Namespace::validate_name("Invalid").is_err());
    /// ```
    pub fn validate_name(name: &str) -> Result<(), String> {
        if name == "system" {
            return Err("Namespace name 'system' is reserved".to_string());
        }

        if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
            return Err("Namespace name must start with a lowercase letter".to_string());
        }

        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(
                "Namespace name can only contain lowercase letters, digits, and underscores"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Check if this namespace can be deleted (has no tables)
    pub fn can_delete(&self) -> bool {
        self.table_count == 0
    }

    /// Increment the table count
    pub fn increment_table_count(&mut self) {
        self.table_count += 1;
    }

    /// Decrement the table count
    pub fn decrement_table_count(&mut self) {
        if self.table_count > 0 {
            self.table_count -= 1;
        }
    }
}

/// Table metadata entity for system.tables.
///
/// Represents metadata for user, shared, stream, and system tables.
///
/// ## Fields
/// - `table_id`: Unique table identifier
/// - `table_name`: Full table name (e.g., "user_table:app:events")
/// - `namespace`: Namespace ID
/// - `table_type`: Type of table (USER, SHARED, STREAM, SYSTEM)
/// - `created_at`: Unix timestamp in milliseconds when table was created
/// - `storage_id`: Optional storage configuration ID (references system.storages)
/// - `use_user_storage`: Whether to use user-specific storage
/// - `flush_policy`: Flush policy configuration (JSON)
/// - `schema_version`: Current schema version
/// - `deleted_retention_hours`: Hours to retain deleted rows
/// - `access_level`: Access level for shared tables (PUBLIC, PRIVATE, RESTRICTED)
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::system::SystemTable;
/// use kalamdb_commons::{NamespaceId, TableName, TableType, StorageId};
///
/// let table = SystemTable {
///     table_id: "tbl_123".to_string(),
///     table_name: TableName::new("events"),
///     namespace: NamespaceId::new("default"),
///     table_type: TableType::User,
///     created_at: 1730000000000,
///     storage_id: Some(StorageId::new("storage_1")),
///     use_user_storage: false,
///     flush_policy: "{}".to_string(),
///     schema_version: 1,
///     deleted_retention_hours: 24,
///     access_level: None,
/// };
/// ```
///
/// Note:
/// - SystemTable is the registry metadata for a table (IDs, namespace, storage, access, lifecycle).
/// - `schemas::TableDefinition` defines the logical columnar schema and table options (columns, PKs, TTL, etc.).
/// - They are complementary, not interchangeable. SystemTable references the active `schema_version` of the logical TableDefinition.
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct SystemTable {
    pub table_id: TableId,
    pub table_name: TableName,
    pub namespace: NamespaceId,
    pub table_type: TableType,
    pub created_at: i64, // Unix timestamp in milliseconds
    pub storage_id: Option<StorageId>,
    pub use_user_storage: bool,
    pub flush_policy: String, // JSON TODO: Why not use a struct here?
    pub schema_version: i32,
    pub deleted_retention_hours: i32,
    /// Access level for SHARED tables (public, private, restricted)
    /// NULL for USER and SYSTEM tables (they have different access control)
    pub access_level: Option<TableAccess>,
}

/// Alias for clarity when referring to registry metadata
pub type TableMetadata = SystemTable;

/// Storage configuration in system_storages table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Storage {
    pub storage_id: StorageId, // PK
    pub storage_name: String,
    pub description: Option<String>,
    pub storage_type: String, // "filesystem" or "s3"
    pub base_directory: String,
    #[serde(default)]
    pub credentials: Option<String>,
    pub shared_tables_template: String,
    pub user_tables_template: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Live query subscription entity for system.live_queries.
///
/// Represents an active live query subscription (WebSocket connection).
///
/// ## Fields
/// - `live_id`: Unique live query ID (format: {user_id}-{conn_id}-{table_name}-{query_id})
/// - `connection_id`: WebSocket connection identifier
/// - `namespace_id`: Namespace ID
/// - `table_name`: Table being queried
/// - `query_id`: Query identifier
/// - `user_id`: User who created the subscription
/// - `query`: SQL query text
/// - `options`: Optional JSON configuration
/// - `created_at`: Unix timestamp in milliseconds when subscription was created
/// - `last_update`: Unix timestamp in milliseconds of last update notification
/// - `changes`: Number of changes sent
/// - `node`: Node/server handling this subscription
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::system::LiveQuery;
/// use kalamdb_commons::{UserId, NamespaceId, TableName};
///
/// let live_query = LiveQuery {
///     live_id: "u_123-conn_456-events-q_789".to_string(),
///     connection_id: "conn_456".to_string(),
///     namespace_id: NamespaceId::new("default"),
///     table_name: TableName::new("events"),
///     query_id: "q_789".to_string(),
///     user_id: UserId::new("u_123"),
///     query: "SELECT * FROM events WHERE type = 'click'".to_string(),
///     options: Some(r#"{"include_initial": true}"#.to_string()),
///     created_at: 1730000000000,
///     last_update: 1730000300000,
///     changes: 42,
///     node: "server-01".to_string(),
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct LiveQuery {
    pub live_id: LiveQueryId, // Format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
    pub connection_id: String,
    pub namespace_id: NamespaceId,
    pub table_name: TableName,
    pub query_id: String,
    pub user_id: UserId,
    pub query: String,
    pub options: Option<String>, // JSON
    pub created_at: i64,         // Unix timestamp in milliseconds
    pub last_update: i64,        // Unix timestamp in milliseconds
    pub changes: i64,
    pub node: String,
}

/// Table definition for information_schema.tables.
///
/// Represents table metadata in SQL standard information_schema format.
///
/// ## Fields
/// - `table_catalog`: Catalog name (usually "def")
/// - `table_schema`: Schema/namespace ID
/// - `table_name`: Table name
/// - `table_type`: SQL standard type (BASE TABLE, SYSTEM VIEW, etc.)
/// - `table_id`: KalamDB-specific table ID
/// - `created_at`: Unix timestamp in milliseconds when table was created
/// - `updated_at`: Unix timestamp in milliseconds when table was last updated
/// - `schema_version`: Schema version number
/// - `storage_id`: Storage configuration ID
/// - `use_user_storage`: User storage flag
/// - `deleted_retention_hours`: Retention for deleted rows (optional)
/// - `ttl_seconds`: Time-to-live for stream tables (optional)
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::system::InformationSchemaTable;
/// use kalamdb_commons::{NamespaceId, TableName, StorageId};
///
/// let table = InformationSchemaTable {
///     table_catalog: "def".to_string(),
///     table_schema: NamespaceId::new("default"),
///     table_name: TableName::new("events"),
///     table_type: "BASE TABLE".to_string(),
///     table_id: "tbl_123".to_string(),
///     created_at: 1730000000000,
///     updated_at: 1730000000000,
///     schema_version: 1,
///     storage_id: StorageId::new("storage_1"),
///     use_user_storage: false,
///     deleted_retention_hours: Some(24),
///     ttl_seconds: None,
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct InformationSchemaTable {
    pub table_catalog: String,
    pub table_schema: NamespaceId,
    pub table_name: TableName,
    pub table_type: String, // BASE TABLE, SYSTEM VIEW, STREAM TABLE
    pub table_id: TableId,
    pub created_at: i64, // Unix timestamp in milliseconds
    pub updated_at: i64, // Unix timestamp in milliseconds
    pub schema_version: u32,
    pub storage_id: StorageId,
    pub use_user_storage: bool,
    pub deleted_retention_hours: Option<u32>,
    pub ttl_seconds: Option<u64>,
}

/// User table counter for flush tracking.
///
/// Tracks row counts per user table for flush decisions.
///
/// ## Fields
/// - `key`: Composite key (format: "{user_id}:{table_name}")
/// - `user_id`: User who owns the table
/// - `table_name`: Table name
/// - `row_count`: Current row count in RocksDB (unflushed)
/// - `last_flushed_at`: Optional Unix timestamp in milliseconds of last flush
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::system::UserTableCounter;
/// use kalamdb_commons::{UserId, TableName};
///
/// let counter = UserTableCounter {
///     key: "u_123:events".to_string(),
///     user_id: UserId::new("u_123"),
///     table_name: TableName::new("events"),
///     row_count: 1500,
///     last_flushed_at: Some(1730000000000),
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct UserTableCounter {
    pub key: String, // "{user_id}:{table_name}"
    pub user_id: UserId,
    pub table_name: TableName,
    pub row_count: u64,
    pub last_flushed_at: Option<i64>, // Unix timestamp in milliseconds
}

/// Table schema version in system_table_schemas table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableSchema {
    pub schema_id: String, // PK (composite: table_id + version)
    pub table_id: TableId,
    pub version: i32,
    pub arrow_schema: String, // Arrow schema as JSON
    pub created_at: i64,
    pub changes: String, // JSON array of schema changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_serialization() {
        let user = User {
            id: UserId::new("u_123"),
            username: "alice".into(),
            password_hash: "$2b$12$hash".to_string(),
            role: Role::User,
            email: Some("test@example.com".to_string()),
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: Some(StorageId::new("storage_1")),
            created_at: 1730000000000,
            updated_at: 1730000000000,
            last_seen: None,
            deleted_at: None,
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&user, config).unwrap();
        let (deserialized, _): (User, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(user, deserialized);
    }

    #[test]
    #[allow(deprecated)]
    fn test_job_serialization() {
        let job = Job {
            job_id: "job_123".into(),
            job_type: JobType::Flush,
            namespace_id: NamespaceId::new("default"),
            table_name: Some(TableName::new("events")),
            status: JobStatus::Completed,
            parameters: None,
            message: Some("Job completed successfully".to_string()),
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: 1730000000000,
            updated_at: 1730000300000,
            started_at: Some(1730000000000),
            finished_at: Some(1730000300000),
            node_id: NodeId::from("server-01"),
            queue: None,
            priority: None,
            // Legacy fields
            result: None,
            error_message: None,
            trace: None,
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&job, config).unwrap();
        let (deserialized, _): (Job, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(job, deserialized);
    }

    #[test]
    fn test_namespace_serialization() {
        let namespace = Namespace {
            namespace_id: NamespaceId::new("default"),
            name: "default".to_string(),
            created_at: 1730000000000,
            options: Some("{}".to_string()),
            table_count: 0,
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&namespace, config).unwrap();
        let (deserialized, _): (Namespace, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(namespace, deserialized);
    }

    #[test]
    fn test_system_table_serialization() {
        let table = SystemTable {
            table_id: TableId::from_strings("default", "events"),
            table_name: TableName::new("events"),
            namespace: NamespaceId::new("default"),
            table_type: TableType::User,
            created_at: 1730000000000,
            storage_id: Some(StorageId::new("storage_1")),
            use_user_storage: false,
            flush_policy: "{}".to_string(),
            schema_version: 1,
            deleted_retention_hours: 24,
            access_level: None, // USER table - no access_level
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&table, config).unwrap();
        let (deserialized, _): (SystemTable, _) =
            bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(table, deserialized);
    }

    #[test]
    fn test_live_query_serialization() {
        let live_query = LiveQuery {
            live_id: "u_123-conn_456-events-q_789".into(),
            connection_id: "conn_456".to_string(),
            namespace_id: NamespaceId::new("default"),
            table_name: TableName::new("events"),
            query_id: "q_789".to_string(),
            user_id: UserId::new("u_123"),
            query: "SELECT * FROM events".to_string(),
            options: Some(r#"{"include_initial": true}"#.to_string()),
            created_at: 1730000000000,
            last_update: 1730000300000,
            changes: 42,
            node: "server-01".to_string(),
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&live_query, config).unwrap();
        let (deserialized, _): (LiveQuery, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(live_query, deserialized);
    }

    #[test]
    fn test_information_schema_table_serialization() {
        let table = InformationSchemaTable {
            table_catalog: "def".to_string(),
            table_schema: NamespaceId::new("default"),
            table_name: TableName::new("events"),
            table_type: "BASE TABLE".to_string(),
            table_id: TableId::from_strings("default", "events"),
            created_at: 1730000000000,
            updated_at: 1730000000000,
            schema_version: 1,
            storage_id: StorageId::new("storage_1"),
            use_user_storage: false,
            deleted_retention_hours: Some(24),
            ttl_seconds: None,
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&table, config).unwrap();
        let (deserialized, _): (InformationSchemaTable, _) =
            bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(table, deserialized);
    }

    #[test]
    fn test_user_table_counter_serialization() {
        let counter = UserTableCounter {
            key: "u_123:events".to_string(),
            user_id: UserId::new("u_123"),
            table_name: TableName::new("events"),
            row_count: 1500,
            last_flushed_at: Some(1730000000000),
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&counter, config).unwrap();
        let (deserialized, _): (UserTableCounter, _) =
            bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(counter, deserialized);
    }

    #[test]
    fn test_job_builder_pattern() {
        let job = Job::new(
            JobId::new("job_123"),
            JobType::Flush,
            NamespaceId::new("default"),
              NodeId::from("server-01"),
        )
        .with_table_name(TableName::new("events"))
        .with_parameters(r#"["param1", "param2"]"#.to_string());

        assert_eq!(job.job_id, JobId::new("job_123"));
        assert_eq!(job.table_name, Some(TableName::new("events")));
        assert_eq!(job.status, JobStatus::Running);
    }

    #[test]
    fn test_job_state_transitions() {
        let job = Job::new(
            JobId::new("job_123"),
            JobType::Flush,
            NamespaceId::new("default"),
            NodeId::from("server-01"),
        );

        assert_eq!(job.status, JobStatus::New);

        let queued_job = job.queue();
        assert_eq!(queued_job.status, JobStatus::Queued);

        let running_job = queued_job.start();
        assert_eq!(running_job.status, JobStatus::Running);
        assert!(running_job.started_at.is_some());

        let completed_job = running_job.complete(Some("Success".to_string()));
        assert_eq!(completed_job.status, JobStatus::Completed);
        assert_eq!(completed_job.message, Some("Success".to_string()));
        assert!(completed_job.finished_at.is_some());
    }
}
