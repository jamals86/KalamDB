//! RocksDB initialization
//!
//! This module handles opening the RocksDB database and creating default column families.

use crate::error::KalamDbError;
use crate::storage::RocksDbConfig;
use rocksdb::{ColumnFamilyDescriptor, DB};
use std::path::Path;
use std::sync::Arc;

/// RocksDB initializer
pub struct RocksDbInit {
    db_path: String,
}

impl RocksDbInit {
    /// Create a new RocksDB initializer
    pub fn new(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    /// Open or create the RocksDB database
    ///
    /// This will:
    /// 1. Create the database if it doesn't exist
    /// 2. Open with existing column families if it does exist
    /// 3. Create default system table column families if needed
    pub fn open(&self) -> Result<Arc<DB>, KalamDbError> {
        let path = Path::new(&self.db_path);
        let config = RocksDbConfig::default();

        // Try to list existing column families
        let mut existing_cfs = match DB::list_cf(&config.db_options, path) {
            Ok(cfs) => cfs,
            Err(_) => {
                // Database doesn't exist yet, will be created with default CF
                vec!["default".to_string()]
            }
        };

        // Ensure all required system table CFs are in the list
        let system_tables = vec!["users", "live_queries", "storage_locations", "jobs"];
        for table_name in system_tables {
            let cf_name = format!("system_table:{}", table_name);
            if !existing_cfs.contains(&cf_name) {
                existing_cfs.push(cf_name);
            }
        }

        // Create column family descriptors for all existing CFs
        let mut cf_descriptors: Vec<ColumnFamilyDescriptor> = Vec::new();

        for cf_name in &existing_cfs {
            let opts = if cf_name == "default" {
                RocksDbConfig::default().db_options
            } else if cf_name.starts_with("system_table:") {
                RocksDbConfig::system_table_cf_options()
            } else if cf_name.starts_with("user_table:") {
                RocksDbConfig::user_table_cf_options()
            } else if cf_name.starts_with("shared_table:") {
                RocksDbConfig::shared_table_cf_options()
            } else if cf_name.starts_with("stream_table:") {
                RocksDbConfig::stream_table_cf_options()
            } else {
                RocksDbConfig::default().db_options
            };

            cf_descriptors.push(ColumnFamilyDescriptor::new(cf_name, opts));
        }

        // Open the database with all column families
        let db = DB::open_cf_descriptors(&config.db_options, path, cf_descriptors)
            .map_err(|e| {
                KalamDbError::Storage(crate::error::StorageError::Other(format!(
                    "Failed to open RocksDB at {}: {}",
                    self.db_path, e
                )))
            })?;

        Ok(Arc::new(db))
    }

    /// Create default system table column families
    ///
    /// Creates column families for:
    /// - system_table:users
    /// - system_table:live_queries
    /// - system_table:storage_locations
    /// - system_table:jobs
    fn create_default_system_tables(&self, db: &Arc<DB>) -> Result<(), KalamDbError> {
        let system_tables = vec!["users", "live_queries", "storage_locations", "jobs"];

        for table_name in system_tables {
            let cf_name = format!("system_table:{}", table_name);
            
            // Check if CF exists
            if db.cf_handle(&cf_name).is_none() {
                // CF doesn't exist, it should have been created during open
                // If we reach here, something went wrong during initialization
                return Err(KalamDbError::CatalogError(format!(
                    "System table column family not found: {}",
                    cf_name
                )));
            }
        }

        Ok(())
    }

    /// Close the database (handled automatically by Arc::drop)
    pub fn close(db: Arc<DB>) {
        drop(db);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    #[test]
    fn test_rocksdb_init() {
        let temp_dir = env::temp_dir().join("kalamdb_rocksdb_init_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let init = RocksDbInit::new(temp_dir.to_str().unwrap());

        // Open database
        let db = init.open().unwrap();

        // Verify system tables exist by checking CF handles
        assert!(db.cf_handle("system_table:users").is_some());
        assert!(db.cf_handle("system_table:live_queries").is_some());
        assert!(db.cf_handle("system_table:storage_locations").is_some());
        assert!(db.cf_handle("system_table:jobs").is_some());

        // Close database
        RocksDbInit::close(db);

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_reopen_existing_database() {
        let temp_dir = env::temp_dir().join("kalamdb_rocksdb_reopen_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let init = RocksDbInit::new(temp_dir.to_str().unwrap());

        // Open database first time
        let db1 = init.open().unwrap();
        RocksDbInit::close(db1);

        // Reopen database
        let db2 = init.open().unwrap();

        // Verify system tables still exist
        assert!(db2.cf_handle("system_table:users").is_some());

        RocksDbInit::close(db2);

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }
}
