//! Test utilities for kalamdb-store.
//!
//! Provides helpers for setting up test databases with minimal boilerplate.

use anyhow::Result;
use rocksdb::{Options, DB};
use std::sync::Arc;
use tempfile::TempDir;

/// Test database wrapper that automatically cleans up on drop.
pub struct TestDb {
    /// RocksDB instance
    pub db: Arc<DB>,
    /// Temporary directory (kept alive for the duration of the test)
    #[allow(dead_code)]
    temp_dir: TempDir,
}

impl TestDb {
    /// Create a new test database with the specified column families.
    ///
    /// # Arguments
    ///
    /// * `cf_names` - List of column family names to create
    ///
    /// # Example
    ///
    /// ```no_run
    /// use kalamdb_store::test_utils::TestDb;
    ///
    /// let test_db = TestDb::new(&["user_table:app:messages"]).unwrap();
    /// // Use test_db.db for testing...
    /// ```
    pub fn new(cf_names: &[&str]) -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open_cf(&opts, temp_dir.path(), cf_names)?;

        Ok(Self {
            db: Arc::new(db),
            temp_dir,
        })
    }

    /// Create a test database with common user table column families.
    ///
    /// Creates column families for:
    /// - `user_table:app:messages`
    /// - `user_table:app:events`
    ///
    /// # Example
    ///
    /// ```no_run
    /// use kalamdb_store::test_utils::TestDb;
    ///
    /// let test_db = TestDb::with_user_tables().unwrap();
    /// // Use test_db.db for testing...
    /// ```
    pub fn with_user_tables() -> Result<Self> {
        Self::new(&["user_table:app:messages", "user_table:app:events"])
    }

    /// Create a test database with a single column family.
    ///
    /// # Arguments
    ///
    /// * `cf_name` - Column family name to create
    ///
    /// # Example
    ///
    /// ```no_run
    /// use kalamdb_store::test_utils::TestDb;
    ///
    /// let test_db = TestDb::single_cf("user_table:app:messages").unwrap();
    /// // Use test_db.db for testing...
    /// ```
    pub fn single_cf(cf_name: &str) -> Result<Self> {
        Self::new(&[cf_name])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_db() {
        let test_db = TestDb::new(&["user_table:app:messages"]).unwrap();

        // Verify DB is accessible
        let cf = test_db.db.cf_handle("user_table:app:messages");
        assert!(cf.is_some());
    }

    #[test]
    fn test_with_user_tables() {
        let test_db = TestDb::with_user_tables().unwrap();

        // Verify both CFs exist
        assert!(test_db.db.cf_handle("user_table:app:messages").is_some());
        assert!(test_db.db.cf_handle("user_table:app:events").is_some());
    }

    #[test]
    fn test_single_cf() {
        let test_db = TestDb::single_cf("user_table:app:messages").unwrap();

        // Verify CF exists
        assert!(test_db.db.cf_handle("user_table:app:messages").is_some());
    }
}
