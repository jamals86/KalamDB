//! System table enumeration
//!
//! Defines all system tables available in KalamDB.

/// System table enumeration
///
/// All system tables in KalamDB. This enum ensures type-safe table registration
/// and prevents typos in table names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemTable {
    /// system.users - User accounts
    Users,
    /// system.namespaces - Database namespaces
    Namespaces,
    /// system.tables - User and shared tables metadata
    Tables,
    /// system.table_schemas - Table schema versions
    TableSchemas,
    /// system.storages - Storage configurations
    Storages,
    /// system.live_queries - Active live query subscriptions
    LiveQueries,
    /// system.jobs - Background job tracking
    Jobs,
}

impl SystemTable {
    /// Get the table name as used in SQL (e.g., "users", "tables")
    pub fn table_name(&self) -> &'static str {
        match self {
            SystemTable::Users => "users",
            SystemTable::Namespaces => "namespaces",
            SystemTable::Tables => "tables",
            SystemTable::TableSchemas => "table_schemas",
            SystemTable::Storages => "storages",
            SystemTable::LiveQueries => "live_queries",
            SystemTable::Jobs => "jobs",
        }
    }

    /// Get the column family name in RocksDB (e.g., "system_users")
    pub fn column_family_name(&self) -> &'static str {
        match self {
            SystemTable::Users => "system_users",
            SystemTable::Namespaces => "system_namespaces",
            SystemTable::Tables => "system_tables",
            SystemTable::TableSchemas => "system_table_schemas",
            SystemTable::Storages => "system_storages",
            SystemTable::LiveQueries => "system_live_queries",
            SystemTable::Jobs => "system_jobs",
        }
    }

    /// Parse from table name (with or without "system." prefix)
    pub fn from_name(name: &str) -> Result<Self, String> {
        // Remove "system." prefix if present
        let name = name.strip_prefix("system.").unwrap_or(name);

        match name {
            "users" | "system_users" => Ok(SystemTable::Users),
            "namespaces" | "system_namespaces" => Ok(SystemTable::Namespaces),
            "tables" | "system_tables" => Ok(SystemTable::Tables),
            "table_schemas" | "system_table_schemas" => Ok(SystemTable::TableSchemas),
            "storages" | "system_storages" => Ok(SystemTable::Storages),
            "live_queries" | "system_live_queries" => Ok(SystemTable::LiveQueries),
            "jobs" | "system_jobs" => Ok(SystemTable::Jobs),
            _ => Err(format!("Unknown system table: {}", name)),
        }
    }

    /// Get all system tables
    pub fn all() -> &'static [SystemTable] {
        &[
            SystemTable::Users,
            SystemTable::Namespaces,
            SystemTable::Tables,
            SystemTable::TableSchemas,
            SystemTable::Storages,
            SystemTable::LiveQueries,
            SystemTable::Jobs,
        ]
    }

    /// Check if a table name is a system table
    pub fn is_system_table(name: &str) -> bool {
        Self::from_name(name).is_ok()
    }

    /// Returns a shared Partition for this system table's column family.
    ///
    /// Allocates each Partition once and returns a reference,
    /// avoiding repeated String allocations across the codebase.
    pub fn partition(&self) -> &'static crate::storage::Partition {
        use crate::storage::Partition;
        use once_cell::sync::Lazy;

        static USERS: Lazy<Partition> =
            Lazy::new(|| Partition::new(SystemTable::Users.column_family_name()));
        static NAMESPACES: Lazy<Partition> =
            Lazy::new(|| Partition::new(SystemTable::Namespaces.column_family_name()));
        static TABLES: Lazy<Partition> =
            Lazy::new(|| Partition::new(SystemTable::Tables.column_family_name()));
        static TABLE_SCHEMAS: Lazy<Partition> =
            Lazy::new(|| Partition::new(SystemTable::TableSchemas.column_family_name()));
        static STORAGES: Lazy<Partition> =
            Lazy::new(|| Partition::new(SystemTable::Storages.column_family_name()));
        static LIVE_QUERIES: Lazy<Partition> =
            Lazy::new(|| Partition::new(SystemTable::LiveQueries.column_family_name()));
        static JOBS: Lazy<Partition> =
            Lazy::new(|| Partition::new(SystemTable::Jobs.column_family_name()));

        match self {
            SystemTable::Users => &USERS,
            SystemTable::Namespaces => &NAMESPACES,
            SystemTable::Tables => &TABLES,
            SystemTable::TableSchemas => &TABLE_SCHEMAS,
            SystemTable::Storages => &STORAGES,
            SystemTable::LiveQueries => &LIVE_QUERIES,
            SystemTable::Jobs => &JOBS,
        }
    }
}

/// Additional named partitions that are not SystemTable rows
/// but still stored as dedicated column families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoragePartition {
    /// Unified information_schema.tables storage
    InformationSchemaTables,
    /// Legacy system columns metadata (kept for compatibility)
    SystemColumns,
    /// User table flush counters
    UserTableCounters,
}

impl StoragePartition {
    /// Returns the partition (column family) name
    pub fn name(&self) -> &'static str {
        match self {
            StoragePartition::InformationSchemaTables => "information_schema_tables",
            StoragePartition::SystemColumns => "system_columns",
            StoragePartition::UserTableCounters => "user_table_counters",
        }
    }

    /// Returns a shared Partition reference for this named partition.
    pub fn partition(&self) -> &'static crate::storage::Partition {
        use crate::storage::Partition;
        use once_cell::sync::Lazy;

        static INFO: Lazy<Partition> =
            Lazy::new(|| Partition::new(StoragePartition::InformationSchemaTables.name()));
        static COLUMNS: Lazy<Partition> =
            Lazy::new(|| Partition::new(StoragePartition::SystemColumns.name()));
        static COUNTERS: Lazy<Partition> =
            Lazy::new(|| Partition::new(StoragePartition::UserTableCounters.name()));

        match self {
            StoragePartition::InformationSchemaTables => &INFO,
            StoragePartition::SystemColumns => &COLUMNS,
            StoragePartition::UserTableCounters => &COUNTERS,
        }
    }
}

impl std::fmt::Display for SystemTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "system.{}", self.table_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_name() {
        assert_eq!(SystemTable::Users.table_name(), "users");
        assert_eq!(SystemTable::Namespaces.table_name(), "namespaces");
        assert_eq!(SystemTable::Jobs.table_name(), "jobs");
    }

    #[test]
    fn test_column_family_name() {
        assert_eq!(SystemTable::Users.column_family_name(), "system_users");
        assert_eq!(
            SystemTable::Namespaces.column_family_name(),
            "system_namespaces"
        );
        assert_eq!(SystemTable::Jobs.column_family_name(), "system_jobs");
    }

    #[test]
    fn test_from_name() {
        assert_eq!(SystemTable::from_name("users").unwrap(), SystemTable::Users);
        assert_eq!(
            SystemTable::from_name("system.users").unwrap(),
            SystemTable::Users
        );
        assert_eq!(
            SystemTable::from_name("system_users").unwrap(),
            SystemTable::Users
        );
        assert_eq!(
            SystemTable::from_name("storages").unwrap(),
            SystemTable::Storages
        );
        assert!(SystemTable::from_name("invalid_table").is_err());
    }

    #[test]
    fn test_is_system_table() {
        assert!(SystemTable::is_system_table("users"));
        assert!(SystemTable::is_system_table("system.users"));
        assert!(SystemTable::is_system_table("storages"));
        assert!(!SystemTable::is_system_table("my_custom_table"));
    }

    #[test]
    fn test_display() {
        assert_eq!(SystemTable::Users.to_string(), "system.users");
        assert_eq!(SystemTable::Jobs.to_string(), "system.jobs");
    }

    #[test]
    fn test_all() {
        let all = SystemTable::all();
        assert_eq!(all.len(), 7);
        assert!(all.contains(&SystemTable::Users));
        assert!(all.contains(&SystemTable::Storages));
    }
}
