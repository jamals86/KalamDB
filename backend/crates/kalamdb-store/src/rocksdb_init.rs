//! RocksDB initialization utilities for KalamDB.
//!
//! Provides a thin helper to open a RocksDB instance with required
//! system column families present.

use anyhow::Result;
use kalamdb_commons::{config::RocksDbSettings, StoragePartition, SystemTable};
use rocksdb::{BlockBasedOptions, Cache, ColumnFamilyDescriptor, Options, DB};
use std::path::Path;
use std::sync::Arc;

/// RocksDB initializer for creating/opening a database with system CFs.
pub struct RocksDbInit {
    db_path: String,
    settings: RocksDbSettings,
}

impl RocksDbInit {
    /// Create a new initializer for the given path with custom settings.
    pub fn new(db_path: impl Into<String>, settings: RocksDbSettings) -> Self {
        Self {
            db_path: db_path.into(),
            settings,
        }
    }

    /// Create a new initializer with default settings.
    pub fn with_defaults(db_path: impl Into<String>) -> Self {
        Self::new(db_path, RocksDbSettings::default())
    }

    /// Open or create the RocksDB database and ensure system CFs exist.
    pub fn open(&self) -> Result<Arc<DB>> {
        let path = Path::new(&self.db_path);

        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        
        // Memory optimization: Use configured settings instead of hardcoded values
        db_opts.set_write_buffer_size(self.settings.write_buffer_size);
        db_opts.set_max_write_buffer_number(self.settings.max_write_buffers);
        db_opts.set_max_background_jobs(self.settings.max_background_jobs);
        
        // Block cache: Shared across all CFs to control total memory
        let cache = Cache::new_lru_cache(self.settings.block_cache_size);
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_block_cache(&cache);
        db_opts.set_block_based_table_factory(&block_opts);

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

        // Build CF descriptors with memory-optimized options
        let cf_descriptors: Vec<_> = existing
            .iter()
            .map(|name| {
                let mut cf_opts = Options::default();
                // Use configured settings for each CF
                cf_opts.set_write_buffer_size(self.settings.write_buffer_size);
                cf_opts.set_max_write_buffer_number(self.settings.max_write_buffers);
                ColumnFamilyDescriptor::new(name, cf_opts)
            })
            .collect();

        let db = DB::open_cf_descriptors(&db_opts, path, cf_descriptors)?;
        Ok(Arc::new(db))
    }

    /// Close database handle (drop Arc)
    pub fn close(_db: Arc<DB>) {}
}
