//! Storage backend abstraction to isolate RocksDB types
//!
//! This module provides a trait-based abstraction over storage backends,
//! allowing kalamdb-core to minimize direct RocksDB coupling.

use anyhow::{Context, Result};
use rocksdb::{ColumnFamily, DB};
use std::sync::Arc;

/// Storage backend interface that hides RocksDB implementation details.
///
/// This trait provides a minimal interface for accessing storage primitives
/// while keeping RocksDB types contained within the storage layer.
pub trait StorageBackend: Send + Sync {
    /// Get a reference to the underlying database.
    ///
    /// **Note**: This method exposes RocksDB types and should only be used
    /// within the storage layer. Higher-level code should use kalamdb-store
    /// abstractions instead.
    fn db(&self) -> &Arc<DB>;

    /// Check if a column family exists.
    fn has_cf(&self, name: &str) -> bool;

    /// Get column family handle by name.
    ///
    /// Returns an error if the column family doesn't exist.
    fn get_cf(&self, name: &str) -> Result<&ColumnFamily>;
}

/// RocksDB implementation of StorageBackend.
///
/// This is the primary storage backend implementation that wraps a RocksDB
/// database instance.
#[derive(Clone)]
pub struct RocksDbBackend {
    db: Arc<DB>,
}

impl RocksDbBackend {
    /// Create a new RocksDB backend from a database instance.
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// Get the underlying database Arc (for initialization/setup code).
    pub fn into_inner(self) -> Arc<DB> {
        self.db
    }
}

impl StorageBackend for RocksDbBackend {
    fn db(&self) -> &Arc<DB> {
        &self.db
    }

    fn has_cf(&self, name: &str) -> bool {
        self.db.cf_handle(name).is_some()
    }

    fn get_cf(&self, name: &str) -> Result<&ColumnFamily> {
        self.db
            .cf_handle(name)
            .with_context(|| format!("Column family not found: {}", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use std::env;
    use std::fs;

    #[test]
    fn test_rocksdb_backend_creation() {
        let temp_dir = env::temp_dir().join("kalamdb_backend_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let init = RocksDbInit::new(temp_dir.to_str().unwrap());
        let db = init.open().unwrap();

        let backend = RocksDbBackend::new(db.clone());

        // Test has_cf (should have default CF)
        assert!(backend.has_cf("default"));
        assert!(!backend.has_cf("nonexistent"));

        // Test get_cf
        let cf_result = backend.get_cf("default");
        assert!(cf_result.is_ok());

        let missing_cf_result = backend.get_cf("nonexistent");
        assert!(missing_cf_result.is_err());

        RocksDbInit::close(db);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_backend_clone() {
        let temp_dir = env::temp_dir().join("kalamdb_backend_clone_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let init = RocksDbInit::new(temp_dir.to_str().unwrap());
        let db = init.open().unwrap();

        let backend1 = RocksDbBackend::new(db.clone());
        let backend2 = backend1.clone();

        // Both backends should point to the same DB
        assert!(Arc::ptr_eq(backend1.db(), backend2.db()));

        RocksDbInit::close(db);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_into_inner() {
        let temp_dir = env::temp_dir().join("kalamdb_backend_into_inner_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let init = RocksDbInit::new(temp_dir.to_str().unwrap());
        let db = init.open().unwrap();
        let db_clone = db.clone();

        let backend = RocksDbBackend::new(db.clone());
        let extracted_db = backend.into_inner();

        // Should be the same Arc
        assert!(Arc::ptr_eq(&extracted_db, &db_clone));

        RocksDbInit::close(db);
        let _ = fs::remove_dir_all(temp_dir);
    }
}
