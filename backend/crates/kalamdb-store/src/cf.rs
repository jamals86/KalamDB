use crate::storage_trait::Partition;

/// Column families used across KalamDB system storage.
/// Centralizes names and provides `Partition` constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnFamily {
    SystemUsers,
    SystemNamespaces,
    SystemTables,
    SystemStorages,
    SystemLiveQueries,
    SystemJobs,
    InformationSchemaTables,
    /// Legacy system columns metadata (kept for backward compatibility)
    SystemColumns,
}

impl ColumnFamily {
    /// Returns the canonical column family name.
    pub fn name(&self) -> &'static str {
        match self {
            ColumnFamily::SystemUsers => kalamdb_commons::constants::ColumnFamilyNames::SYSTEM_USERS,
            ColumnFamily::SystemNamespaces => kalamdb_commons::constants::ColumnFamilyNames::SYSTEM_NAMESPACES,
            ColumnFamily::SystemTables => kalamdb_commons::constants::ColumnFamilyNames::SYSTEM_TABLES,
            ColumnFamily::SystemStorages => kalamdb_commons::constants::ColumnFamilyNames::SYSTEM_STORAGES,
            ColumnFamily::SystemLiveQueries => kalamdb_commons::constants::ColumnFamilyNames::SYSTEM_LIVE_QUERIES,
            ColumnFamily::SystemJobs => kalamdb_commons::constants::ColumnFamilyNames::SYSTEM_JOBS,
            ColumnFamily::InformationSchemaTables => kalamdb_commons::constants::ColumnFamilyNames::INFORMATION_SCHEMA_TABLES,
            ColumnFamily::SystemColumns => "system_columns",
        }
    }

    /// Returns a Partition referencing this column family.
    pub fn partition(&self) -> Partition {
        Partition::new(self.name())
    }
}

