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

    /// Legacy storage locations table: `system.storage_locations`
    pub const STORAGE_LOCATIONS: &'static str = "system.storage_locations";

    /// Table schema versions: `system.table_schemas`
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

    /// System table schemas column family
    pub const SYSTEM_TABLE_SCHEMAS: &'static str = "system_table_schemas";

    /// Legacy storage locations column family
    pub const SYSTEM_STORAGE_LOCATIONS: &'static str = "system_storage_locations";

    /// System storages column family
    pub const SYSTEM_STORAGES: &'static str = "system_storages";

    /// System live queries column family
    pub const SYSTEM_LIVE_QUERIES: &'static str = "system_live_queries";

    /// System jobs column family
    pub const SYSTEM_JOBS: &'static str = "system_jobs";

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
    /// Timestamp when row was last updated
    pub const UPDATED: &'static str = "_updated";

    /// Soft delete flag (true = deleted)
    pub const DELETED: &'static str = "_deleted";
}

/// Global instance of system column names.
pub const SYSTEM_COLUMNS: SystemColumnNames = SystemColumnNames;

/// Reserved namespace name for system tables.
pub const SYSTEM_NAMESPACE: &'static str = "system";

/// Default namespace name for user tables when not specified.
pub const DEFAULT_NAMESPACE: &'static str = "default";

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_system_table_names() {
//         assert_eq!(SYSTEM_TABLES.USERS, "system.users");
//         assert_eq!(SYSTEM_TABLES.NAMESPACES, "system.namespaces");
//         assert_eq!(SYSTEM_TABLES.TABLES, "system.tables");
//         assert_eq!(SYSTEM_TABLES.TABLE_SCHEMAS, "system.table_schemas");
//         assert_eq!(SYSTEM_TABLES.STORAGES, "system.storages");
//         assert_eq!(SYSTEM_TABLES.LIVE_QUERIES, "system.live_queries");
//         assert_eq!(SYSTEM_TABLES.JOBS, "system.jobs");
//     }

//     #[test]
//     fn test_column_family_names() {
//         assert_eq!(COLUMN_FAMILIES.SYSTEM_USERS, "system_users");
//         assert_eq!(COLUMN_FAMILIES.USER_TABLE_COUNTERS, "user_table_counters");
//         assert_eq!(COLUMN_FAMILIES.USER_TABLE_PREFIX, "user_");
//         assert_eq!(COLUMN_FAMILIES.SHARED_TABLE_PREFIX, "shared_");
//         assert_eq!(COLUMN_FAMILIES.STREAM_TABLE_PREFIX, "stream_");
//     }

//     #[test]
//     fn test_system_columns() {
//         assert_eq!(SYSTEM_COLUMNS.UPDATED, "_updated");
//         assert_eq!(SYSTEM_COLUMNS.DELETED, "_deleted");
//     }

//     #[test]
//     fn test_reserved_namespaces() {
//         assert_eq!(SYSTEM_NAMESPACE, "system");
//         assert_eq!(DEFAULT_NAMESPACE, "default");
//     }
// }
