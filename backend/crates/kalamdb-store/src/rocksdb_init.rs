//! RocksDB initialization utilities for KalamDB.
//!
//! Provides a thin helper to open a RocksDB instance with required
//! system column families present.

use anyhow::Result;
use kalamdb_commons::{StoragePartition, SystemTable};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use std::path::Path;
use std::sync::Arc;

/// RocksDB initializer for creating/opening a database with system CFs.
pub struct RocksDbInit {
    db_path: String,
}

impl RocksDbInit {
    /// Create a new initializer for the given path.
    pub fn new(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    /// Open or create the RocksDB database and ensure system CFs exist.
    pub fn open(&self) -> Result<Arc<DB>> {
        let path = Path::new(&self.db_path);

        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        // Determine existing CFs (or default if DB missing)
        let mut existing = match DB::list_cf(&db_opts, path) {
            Ok(cfs) if !cfs.is_empty() => cfs,
            _ => vec!["default".to_string()],
        };

        // Ensure system CFs using single source of truth (SystemTable + StoragePartition)
        // 1) All system tables' CFs
        for table in SystemTable::all().iter() {
            let name = table.column_family_name();
            if !existing.iter().any(|n| n == name) {
                existing.push(name.to_string());
            }
        }

        // 2) Additional named partitions used by the system
        let extra_partitions = [
            StoragePartition::InformationSchemaTables.name(),
            StoragePartition::SystemColumns.name(),
            StoragePartition::UserTableCounters.name(),
            StoragePartition::SystemUsersUsernameIdx.name(),
            StoragePartition::SystemUsersRoleIdx.name(),
            StoragePartition::SystemUsersDeletedAtIdx.name(),
        ];

        for name in extra_partitions.iter() {
            if !existing.iter().any(|n| n == name) {
                existing.push((*name).to_string());
            }
        }

        // Build CF descriptors
        let cf_descriptors: Vec<_> = existing
            .iter()
            .map(|name| ColumnFamilyDescriptor::new(name, Options::default()))
            .collect();

        let db = DB::open_cf_descriptors(&db_opts, path, cf_descriptors)?;
        Ok(Arc::new(db))
    }

    /// Close database handle (drop Arc)
    pub fn close(_db: Arc<DB>) {}
}
