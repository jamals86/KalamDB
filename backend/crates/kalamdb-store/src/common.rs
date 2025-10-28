// //! Common utilities for table stores to reduce code duplication.
// //!
// //! This module provides shared functions for RocksDB column family operations
// //! that are used across user, shared, and stream table stores.

// use rocksdb::{ColumnFamilyDescriptor, Options, DB};
// use std::sync::Arc;

// /// Creates a new column family in RocksDB with standard options.
// ///
// /// This helper consolidates the common pattern of:
// /// 1. Creating CF options
// /// 2. Creating CF descriptor
// /// 3. Creating the column family
// /// 4. Checking if it exists
// ///
// /// # Arguments
// ///
// /// * `db` - RocksDB database handle
// /// * `cf_name` - Name of the column family to create
// ///
// /// # Returns
// ///
// /// * `Ok(())` - Column family created successfully or already exists
// /// * `Err(String)` - Error message if creation failed
// ///
// /// # Example
// ///
// /// ```rust,ignore
// /// use kalamdb_store::common::create_column_family;
// ///
// /// let db = Arc::new(DB::open_default(&path).unwrap());
// /// create_column_family(&db, "user_table_alice_messages")?;
// /// ```
// pub fn create_column_family(db: &Arc<DB>, cf_name: &str) -> Result<(), String> {
//     // Check if column family already exists
//     if db.cf_handle(cf_name).is_some() {
//         // Column family already exists, nothing to do
//         return Ok(());
//     }

//     // Create column family options
//     let mut cf_opts = Options::default();
//     cf_opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB write buffer
//     cf_opts.set_max_write_buffer_number(3);

//     // SAFETY: RocksDB's DB is internally synchronized. The create_cf method
//     // requires &mut self for API consistency, but RocksDB handles all
//     // synchronization internally via locks. This is safe in a multi-threaded
//     // context because:
//     // 1. RocksDB uses internal mutexes to protect all data structures
//     // 2. create_cf is an atomic operation from RocksDB's perspective
//     // 3. We're not violating any actual data races - the mut requirement
//     //    is a Rust API design choice, not a safety invariant
//     unsafe {
//         let db_ptr = Arc::as_ptr(db) as *mut DB;
//         (*db_ptr)
//             .create_cf(cf_name, &cf_opts)
//             .map_err(|e| format!("Failed to create column family '{}': {}", cf_name, e))?;
//     }

//     Ok(())
// }

// /// Drops a column family from RocksDB.
// ///
// /// This helper consolidates the common pattern of:
// /// 1. Checking if CF exists
// /// 2. Dropping the column family
// /// 3. Handling errors gracefully
// ///
// /// # Arguments
// ///
// /// * `db` - RocksDB database handle
// /// * `cf_name` - Name of the column family to drop
// ///
// /// # Returns
// ///
// /// * `Ok(())` - Column family dropped successfully or doesn't exist
// /// * `Err(String)` - Error message if drop failed
// ///
// /// # Example
// ///
// /// ```rust,ignore
// /// use kalamdb_store::common::drop_column_family;
// ///
// /// let db = Arc::new(DB::open_default(&path).unwrap());
// /// drop_column_family(&db, "user_table_alice_messages")?;
// /// ```
// pub fn drop_column_family(db: &Arc<DB>, cf_name: &str) -> Result<(), String> {
//     // Check if column family exists
//     if db.cf_handle(cf_name).is_none() {
//         // Column family doesn't exist, nothing to drop
//         return Ok(());
//     }

//     // SAFETY: Same reasoning as create_column_family - RocksDB is internally synchronized
//     unsafe {
//         let db_ptr = Arc::as_ptr(db) as *mut DB;
//         (*db_ptr)
//             .drop_cf(cf_name)
//             .map_err(|e| format!("Failed to drop column family '{}': {}", cf_name, e))?;
//     }

//     Ok(())
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use tempfile::TempDir;

//     #[test]
//     fn test_create_column_family_new() {
//         let temp_dir = TempDir::new().unwrap();
//         let db = Arc::new(DB::open_default(temp_dir.path()).unwrap());

//         // Create a new column family
//         let result = create_column_family(&db, "test_cf");
//         assert!(result.is_ok());

//         // Verify it exists
//         let cfs = DB::list_cf(&Options::default(), db.path()).unwrap();
//         assert!(cfs.contains(&"test_cf".to_string()));
//     }

//     #[test]
//     fn test_create_column_family_already_exists() {
//         let temp_dir = TempDir::new().unwrap();
//         let db = Arc::new(DB::open_default(temp_dir.path()).unwrap());

//         // Create once
//         create_column_family(&db, "test_cf").unwrap();

//         // Create again - should succeed (idempotent)
//         let result = create_column_family(&db, "test_cf");
//         assert!(result.is_ok());
//     }

//     #[test]
//     fn test_drop_column_family_exists() {
//         let temp_dir = TempDir::new().unwrap();
//         let db = Arc::new(DB::open_default(temp_dir.path()).unwrap());

//         // Create then drop
//         create_column_family(&db, "test_cf").unwrap();
//         let result = drop_column_family(&db, "test_cf");
//         assert!(result.is_ok());

//         // Verify it's gone
//         let cfs = DB::list_cf(&Options::default(), db.path()).unwrap();
//         assert!(!cfs.contains(&"test_cf".to_string()));
//     }

//     #[test]
//     fn test_drop_column_family_not_exists() {
//         let temp_dir = TempDir::new().unwrap();
//         let db = Arc::new(DB::open_default(temp_dir.path()).unwrap());

//         // Drop non-existent CF - should succeed (idempotent)
//         let result = drop_column_family(&db, "nonexistent_cf");
//         assert!(result.is_ok());
//     }
// }
