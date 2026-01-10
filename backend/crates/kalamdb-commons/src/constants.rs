//! System-wide constants for KalamDB.
//!
//! This module centralizes constant definitions used across all crates, including:
//! - System table names
//! - Column family names
//! - Reserved identifiers
//!
//! ## Example Usage
//!
//! ```rust
//! use kalamdb_commons::constants::SystemTableNames;
//!
//! assert_eq!(SystemTableNames::USERS, "system.users");
//! assert_eq!(SystemTableNames::NAMESPACES, "system.namespaces");
//! ```

/// System table names used throughout KalamDB.
///
/// All system tables are prefixed with "system." to distinguish them from user tables.
pub struct SystemTableNames;

#[allow(non_upper_case_globals)]
impl SystemTableNames {
    /// System users table: `system.users`
    pub const USERS: &'static str = "system.users";

    /// System namespaces table: `system.namespaces`
    pub const NAMESPACES: &'static str = "system.namespaces";

    /// System tables catalog: `system.tables`
    pub const TABLES: &'static str = "system.tables";

    /// Table schema history: `system.table_schemas`
    pub const TABLE_SCHEMAS: &'static str = "system.table_schemas";

    /// Storage locations configuration: `system.storages`
    pub const STORAGES: &'static str = "system.storages";

    /// Active live query subscriptions: `system.live_queries`
    pub const LIVE_QUERIES: &'static str = "system.live_queries";

    /// Background job tracking: `system.jobs`
    pub const JOBS: &'static str = "system.jobs";
}

/// Global instance of system table names.
pub const SYSTEM_TABLES: SystemTableNames = SystemTableNames;

/// RocksDB column family names.
///
/// Provides centralized naming for all column families used in KalamDB storage.
pub struct ColumnFamilyNames;

#[allow(non_upper_case_globals)]
impl ColumnFamilyNames {
    /// Default column family (RocksDB built-in)
    pub const DEFAULT: &'static str = "default";

    /// System users column family
    pub const SYSTEM_USERS: &'static str = "system_users";

    /// System namespaces column family
    pub const SYSTEM_NAMESPACES: &'static str = "system_namespaces";

    /// System tables column family
    pub const SYSTEM_TABLES: &'static str = "system_tables";

    /// System storages column family
    pub const SYSTEM_STORAGES: &'static str = "system_storages";

    /// System live queries column family
    pub const SYSTEM_LIVE_QUERIES: &'static str = "system_live_queries";

    /// System jobs column family
    pub const SYSTEM_JOBS: &'static str = "system_jobs";

    /// Unified information_schema tables (replaces system_table_schemas + system_columns)
    pub const INFORMATION_SCHEMA_TABLES: &'static str = "information_schema_tables";

    /// User table flush counters
    pub const USER_TABLE_COUNTERS: &'static str = "user_table_counters";

    /// Prefix for user table column families (appended with table name)
    pub const USER_TABLE_PREFIX: &'static str = "user_";

    /// Prefix for shared table column families (appended with table name)
    pub const SHARED_TABLE_PREFIX: &'static str = "shared_";

    /// Prefix for stream table column families (appended with table name)
    pub const STREAM_TABLE_PREFIX: &'static str = "stream_";
}

/// Global instance of column family names.
pub const COLUMN_FAMILIES: ColumnFamilyNames = ColumnFamilyNames;

/// System column names added automatically to all tables.
pub struct SystemColumnNames;

#[allow(non_upper_case_globals)]
impl SystemColumnNames {
    // REMOVED: _updated column (timestamp is embedded in _seq Snowflake ID)
    // Use _seq >> 22 to extract timestamp in milliseconds
    // pub const UPDATED: &'static str = "_updated";

    /// Soft delete flag (true = deleted)
    pub const DELETED: &'static str = "_deleted";

    /// Sequence column used for MVCC versioning
    pub const SEQ: &'static str = "_seq";

    /// Check if a column name is a system column
    pub fn is_system_column(column_name: &str) -> bool {
        matches!(column_name, Self::DELETED | Self::SEQ)
    }
}

/// Global instance of system column names.
pub const SYSTEM_COLUMNS: SystemColumnNames = SystemColumnNames;

/// Reserved namespace name for system tables.
pub const SYSTEM_NAMESPACE: &str = "system";

/// Default namespace name for user tables when not specified.
pub const DEFAULT_NAMESPACE: &str = "default";

/// Maximum SQL query length in bytes (1MB)
///
/// Prevents DoS attacks via extremely long SQL strings that could
/// cause excessive memory usage or parsing time.
/// Most legitimate queries are under 10KB.
pub const MAX_SQL_QUERY_LENGTH: usize = 1024 * 1024; // 1MB

/// Authentication-related constants.
pub struct AuthConstants;

#[allow(non_upper_case_globals)]
impl AuthConstants {
    /// Default system user username created on first database initialization
    pub const DEFAULT_SYSTEM_USERNAME: &'static str = "root";

    /// Default system user ID created on first database initialization
    pub const DEFAULT_ROOT_USER_ID: &'static str = "root";
}

/// Global instance of authentication constants.
pub const AUTH: AuthConstants = AuthConstants;

/// Anonymous user ID constant (matches ExecutionContext::anonymous())
pub const ANONYMOUS_USER_ID: &str = "anonymous";

/// Reserved namespace names that cannot be used by users.
///
/// These names are reserved for system use and will be rejected during
/// namespace creation. The check is case-insensitive.
///
/// ## Reserved Names
/// - `system`: System tables namespace
/// - `sys`: Common system alias
/// - `root`: Root/admin namespace
/// - `kalamdb`/`kalam`: KalamDB internal namespaces
/// - `main`/`default`: Default namespace aliases
/// - `sql`/`admin`/`internal`: Reserved for system operations
/// - `information_schema`: SQL standard metadata schema
/// - `pg_catalog`: PostgreSQL compatibility
/// - `datafusion`: DataFusion internal catalog
pub const RESERVED_NAMESPACE_NAMES: &[&str] = &[
    "system",
    "sys",
    "root",
    "kalamdb",
    "kalam",
    "main",
    "default",
    "sql",
    "admin",
    "internal",
    "information_schema",
    "pg_catalog",
    "datafusion",
];

/// System schema versioning constants.
///
/// Used to track the version of system tables schema for future migrations.
/// When new system tables are added or schema changes are required, increment
/// the version number and implement migration logic in `initialize_system_tables()`.
///
/// ## Version History
/// - v1 (2025-01-15): Initial schema with 7 system tables:
///   - system.users, system.namespaces, system.tables, system.storages
///   - system.live_queries, system.jobs, system.audit_logs
///
/// Current system schema version.
pub const SYSTEM_SCHEMA_VERSION: u32 = 1;

/// RocksDB key for storing system schema version in default column family.
pub const SYSTEM_SCHEMA_VERSION_KEY: &str = "system:schema_version";
