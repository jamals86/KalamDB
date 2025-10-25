//! User table storage operations.
//!
//! Provides isolated storage for user-scoped tables with automatic system column injection.
//! Each user's data is isolated with key format `{user_id}:{row_id}`.

use anyhow::{Context, Result};
use chrono::Utc;
use rocksdb::{ColumnFamily, IteratorMode, DB};
use serde_json::Value as JsonValue;
use std::sync::Arc;

use crate::key_encoding::{parse_user_key, user_key};

/// User table storage with automatic system column injection.
///
/// # System Columns
///
/// All operations automatically inject:
/// - `_updated`: Current timestamp (ISO 8601)
/// - `_deleted`: Boolean flag (false by default)
///
/// # Key Format
///
/// Keys use the format: `{user_id}:{row_id}`
pub struct UserTableStore {
    db: Arc<DB>,
}

impl UserTableStore {
    /// Create a new user table store.
    pub fn new(db: Arc<DB>) -> Result<Self> {
        Ok(Self { db })
    }

    fn cf_name(namespace_id: &str, table_name: &str) -> String {
        format!(
            "{}{}:{}",
            kalamdb_commons::constants::ColumnFamilyNames::USER_TABLE_PREFIX,
            namespace_id,
            table_name
        )
    }

    fn ensure_cf(&self, namespace_id: &str, table_name: &str) -> Result<&ColumnFamily> {
        let cf_name = Self::cf_name(namespace_id, table_name);
        if self.db.cf_handle(&cf_name).is_none() {
            self.create_column_family(namespace_id, table_name)?;
        }

        self.db.cf_handle(&cf_name).with_context(|| {
            format!(
                "Column family not found even after creation attempt: {}",
                cf_name
            )
        })
    }

    /// Create a column family for a user table.
    ///
    /// This should be called during CREATE USER TABLE to initialize storage.
    /// Returns Ok(()) if the CF was created successfully or already exists.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    pub fn create_column_family(&self, namespace_id: &str, table_name: &str) -> Result<()> {
        let cf_name = Self::cf_name(namespace_id, table_name);
        crate::common::create_column_family(&self.db, &cf_name)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Insert or update a row for a specific user.
    ///
    /// Automatically injects system columns:
    /// - `_updated`: Current timestamp
    /// - `_deleted`: false
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    /// * `user_id` - User identifier
    /// * `row_id` - Row identifier
    /// * `row_data` - Row data as JSON (system columns added automatically)
    pub fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
        mut row_data: JsonValue,
    ) -> Result<()> {
        // Inject system columns
        if let Some(obj) = row_data.as_object_mut() {
            obj.insert(
                kalamdb_commons::constants::SystemColumnNames::UPDATED.to_string(),
                JsonValue::String(Utc::now().to_rfc3339()),
            );
            obj.insert(
                kalamdb_commons::constants::SystemColumnNames::DELETED.to_string(),
                JsonValue::Bool(false),
            );
        }

        let cf = self.ensure_cf(namespace_id, table_name)?;

        let key = user_key(user_id, row_id);
        let value = serde_json::to_vec(&row_data)?;

        self.db.put_cf(cf, key.as_bytes(), &value)?;
        Ok(())
    }

    /// Get a row for a specific user.
    ///
    /// Returns `None` if the row doesn't exist or is soft-deleted.
    pub fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
    ) -> Result<Option<JsonValue>> {
        let cf_name = Self::cf_name(namespace_id, table_name);
        let cf = match self.db.cf_handle(&cf_name) {
            Some(cf) => cf,
            None => return Ok(None),
        };

        let key = user_key(user_id, row_id);
        let value = self.db.get_cf(cf, key.as_bytes())?;

        match value {
            Some(bytes) => {
                let row_data: JsonValue = serde_json::from_slice(&bytes)?;

                // Filter out soft-deleted rows
                if let Some(obj) = row_data.as_object() {
                    if let Some(deleted) =
                        obj.get(kalamdb_commons::constants::SystemColumnNames::DELETED)
                    {
                        if deleted.as_bool() == Some(true) {
                            return Ok(None);
                        }
                    }
                }

                Ok(Some(row_data))
            }
            None => Ok(None),
        }
    }

    /// Get a row including soft-deleted rows.
    ///
    /// Unlike `get()`, this method returns soft-deleted rows (where `_deleted = true`).
    /// Useful for change detection when notifying DELETE operations.
    pub fn get_include_deleted(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
    ) -> Result<Option<JsonValue>> {
        let cf_name = Self::cf_name(namespace_id, table_name);
        let cf = match self.db.cf_handle(&cf_name) {
            Some(cf) => cf,
            None => return Ok(None),
        };

        let key = user_key(user_id, row_id);
        let value = self.db.get_cf(cf, key.as_bytes())?;

        match value {
            Some(bytes) => {
                let row_data: JsonValue = serde_json::from_slice(&bytes)?;
                Ok(Some(row_data))
            }
            None => Ok(None),
        }
    }

    /// Delete a row (soft or hard delete).
    ///
    /// # Arguments
    ///
    /// * `hard` - If true, physically remove the row. If false, set `_deleted = true`.
    pub fn delete(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
        hard: bool,
    ) -> Result<()> {
        let cf = self.ensure_cf(namespace_id, table_name)?;

        let key = user_key(user_id, row_id);

        if hard {
            // Physical deletion
            self.db.delete_cf(cf, key.as_bytes())?;
        } else {
            // Soft delete: set _deleted = true, update _updated
            if let Some(bytes) = self.db.get_cf(cf, key.as_bytes())? {
                let mut row_data: JsonValue = serde_json::from_slice(&bytes)?;

                if let Some(obj) = row_data.as_object_mut() {
                    obj.insert(
                        kalamdb_commons::constants::SystemColumnNames::DELETED.to_string(),
                        JsonValue::Bool(true),
                    );
                    obj.insert(
                        kalamdb_commons::constants::SystemColumnNames::UPDATED.to_string(),
                        JsonValue::String(Utc::now().to_rfc3339()),
                    );
                }

                let value = serde_json::to_vec(&row_data)?;
                self.db.put_cf(cf, key.as_bytes(), &value)?;
            }
        }

        Ok(())
    }

    /// Scan all rows for a specific user.
    ///
    /// Returns an iterator over `(row_id, row_data)` pairs.
    /// Filters out soft-deleted rows.
    pub fn scan_user(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
    ) -> Result<Vec<(String, JsonValue)>> {
        let cf_name = Self::cf_name(namespace_id, table_name);
        let cf = match self.db.cf_handle(&cf_name) {
            Some(cf) => cf,
            None => return Ok(Vec::new()),
        };

        let prefix = format!("{}:", user_id);
        let mut results = Vec::new();

        let iter = self.db.iterator_cf(cf, IteratorMode::Start);
        for item in iter {
            let (key_bytes, value_bytes) = item?;
            let key = String::from_utf8(key_bytes.to_vec())?;

            // Only include rows with matching user_id prefix
            if !key.starts_with(&prefix) {
                continue;
            }

            let row_data: JsonValue = serde_json::from_slice(&value_bytes)?;

            // Filter out soft-deleted rows
            if let Some(obj) = row_data.as_object() {
                if let Some(deleted) =
                    obj.get(kalamdb_commons::constants::SystemColumnNames::DELETED)
                {
                    if deleted.as_bool() == Some(true) {
                        continue;
                    }
                }
            }

            let (_, row_id) = parse_user_key(&key)?;
            results.push((row_id, row_data));
        }

        Ok(results)
    }

    /// Scan all rows for a specific user (alias for scan_user).
    ///
    /// Returns a vector of `(row_id, row_data)` pairs.
    /// Filters out soft-deleted rows.
    pub fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
    ) -> Result<Vec<(String, JsonValue)>> {
        self.scan_user(namespace_id, table_name, user_id)
    }

    /// Scan all rows across all users.
    ///
    /// Returns a vector of `(user_id, row_id, row_data)` tuples.
    /// Filters out soft-deleted rows.
    ///
    /// # Note
    ///
    /// This method scans the entire column family and may be expensive for large tables.
    /// Use with caution in production environments.
    pub fn scan_all(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<Vec<(String, String, JsonValue)>> {
        let cf_name = Self::cf_name(namespace_id, table_name);
        let cf = match self.db.cf_handle(&cf_name) {
            Some(cf) => cf,
            None => return Ok(Vec::new()),
        };

        let mut results = Vec::new();

        let iter = self.db.iterator_cf(cf, IteratorMode::Start);
        for item in iter {
            let (key_bytes, value_bytes) = item?;
            let key = String::from_utf8(key_bytes.to_vec())?;

            let row_data: JsonValue = serde_json::from_slice(&value_bytes)?;

            // Filter out soft-deleted rows
            if let Some(obj) = row_data.as_object() {
                if let Some(deleted) = obj.get("_deleted") {
                    if deleted.as_bool() == Some(true) {
                        continue;
                    }
                }
            }

            let (user_id, row_id) = parse_user_key(&key)?;
            results.push((user_id, row_id, row_data));
        }

        Ok(results)
    }

    /// Get all rows grouped by user ID for flush operations.
    ///
    /// Returns a HashMap of `user_id -> Vec<(key_bytes, row_data)>`.
    /// Filters out soft-deleted rows (they should not be flushed).
    ///
    /// # Note
    ///
    /// This method is designed for flush operations and returns raw key bytes
    /// for efficient batch deletion.
    pub fn get_rows_by_user(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<std::collections::HashMap<String, Vec<(Vec<u8>, JsonValue)>>> {
        use std::collections::HashMap;

        let cf_name = Self::cf_name(namespace_id, table_name);
        let cf = match self.db.cf_handle(&cf_name) {
            Some(cf) => cf,
            None => return Ok(std::collections::HashMap::new()),
        };

        let mut rows_by_user: HashMap<String, Vec<(Vec<u8>, JsonValue)>> = HashMap::new();

        let iter = self.db.iterator_cf(cf, IteratorMode::Start);
        for item in iter {
            let (key_bytes, value_bytes) = item?;

            // Parse JSON value
            let row_data: JsonValue = serde_json::from_slice(&value_bytes)?;

            // Skip soft-deleted rows (don't flush them)
            if let Some(obj) = row_data.as_object() {
                if let Some(deleted) = obj.get("_deleted") {
                    if deleted.as_bool() == Some(true) {
                        continue;
                    }
                }
            }

            // Parse key to get user_id
            let key_str = String::from_utf8(key_bytes.to_vec())?;
            let (user_id, _row_id) = parse_user_key(&key_str)?;

            rows_by_user
                .entry(user_id)
                .or_default()
                .push((key_bytes.to_vec(), row_data));
        }

        Ok(rows_by_user)
    }

    /// Delete multiple rows by their raw key bytes (batch operation).
    ///
    /// This method is designed for flush operations where we need to delete
    /// rows after successfully writing them to Parquet files.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    /// * `keys` - Vector of raw key bytes to delete
    pub fn delete_batch_by_keys(
        &self,
        namespace_id: &str,
        table_name: &str,
        keys: &[Vec<u8>],
    ) -> Result<()> {
        use rocksdb::WriteBatch;

        let cf_name = Self::cf_name(namespace_id, table_name);
        let cf = match self.db.cf_handle(&cf_name) {
            Some(cf) => cf,
            None => return Ok(()),
        };

        let mut batch = WriteBatch::default();

        for key_bytes in keys {
            batch.delete_cf(cf, key_bytes);
        }

        self.db.write(batch)?;
        Ok(())
    }

    /// Drop entire table by deleting its column family
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - The namespace identifier
    /// * `table_name` - The table name
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the column family was successfully dropped,
    /// or an error if the operation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use kalamdb_store::UserTableStore;
    /// # use std::sync::Arc;
    /// # use rocksdb::DB;
    /// # let db = Arc::new(DB::open_default("test").unwrap());
    /// let store = UserTableStore::new(db).unwrap();
    /// store.drop_table("app", "messages").unwrap();
    /// ```
    pub fn drop_table(&self, namespace_id: &str, table_name: &str) -> Result<()> {
        let cf_name = Self::cf_name(namespace_id, table_name);

        // Get the column family handle
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        // Delete all keys by iterating and batching deletes
        let mut batch = rocksdb::WriteBatch::default();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, _) = item?;
            batch.delete_cf(cf, key);
        }

        self.db.write(batch)?;
        Ok(())
    }

    /// Create a RocksDB snapshot for consistent reads during flush operations
    ///
    /// The snapshot provides a consistent view of the database at the time it was created,
    /// preventing data loss from concurrent inserts during the flush process.
    ///
    /// # Returns
    ///
    /// A RocksDB Snapshot that can be used with `scan_with_snapshot`
    ///
    /// # T151a: RocksDB snapshot for read consistency
    pub fn create_snapshot(&self) -> rocksdb::Snapshot<'_> {
        self.db.snapshot()
    }

    /// Scan table using a snapshot for consistent iteration
    ///
    /// This method provides a consistent view of the table data using the provided snapshot.
    /// It's designed for flush operations where we need to ensure no rows are missed due to
    /// concurrent writes.
    ///
    /// # Arguments
    ///
    /// * `snapshot` - RocksDB snapshot created with `create_snapshot()`
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    ///
    /// # Returns
    ///
    /// A RocksDB iterator over (key_bytes, value_bytes) pairs from the snapshot.
    /// Returns an empty iterator if the column family doesn't exist.
    ///
    /// # T151b: Sequential scan with snapshot for streaming flush
    pub fn scan_with_snapshot<'a>(
        &'a self,
        snapshot: &'a rocksdb::Snapshot,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<rocksdb::DBIteratorWithThreadMode<'a, rocksdb::DB>> {
        let cf_name = Self::cf_name(namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        Ok(snapshot.iterator_cf(cf, IteratorMode::Start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocksdb::Options;
    use serde_json::json;
    use tempfile::TempDir;

    fn create_test_db() -> (Arc<DB>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cf_names = vec!["user_table:app:messages"];
        let db = DB::open_cf(&opts, temp_dir.path(), &cf_names).unwrap();

        (Arc::new(db), temp_dir)
    }

    #[test]
    fn test_put_and_get() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        let row_data = json!({
            "message_id": "msg001",
            "content": "Hello, world!"
        });

        store
            .put("app", "messages", "user123", "msg001", row_data.clone())
            .unwrap();

        let retrieved = store.get("app", "messages", "user123", "msg001").unwrap();
        assert!(retrieved.is_some());

        let retrieved_data = retrieved.unwrap();
        assert_eq!(retrieved_data["message_id"], "msg001");
        assert_eq!(retrieved_data["content"], "Hello, world!");
        assert!(retrieved_data[kalamdb_commons::constants::SystemColumnNames::UPDATED].is_string());
        assert_eq!(
            retrieved_data[kalamdb_commons::constants::SystemColumnNames::DELETED],
            false
        );
    }

    #[test]
    fn test_soft_delete() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        let row_data = json!({
            "message_id": "msg001",
            "content": "Hello, world!"
        });

        store
            .put("app", "messages", "user123", "msg001", row_data)
            .unwrap();

        // Soft delete
        store
            .delete("app", "messages", "user123", "msg001", false)
            .unwrap();

        // Should return None because row is soft-deleted
        let retrieved = store.get("app", "messages", "user123", "msg001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_hard_delete() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        let row_data = json!({
            "message_id": "msg001",
            "content": "Hello, world!"
        });

        store
            .put("app", "messages", "user123", "msg001", row_data)
            .unwrap();

        // Hard delete
        store
            .delete("app", "messages", "user123", "msg001", true)
            .unwrap();

        // Should return None because row is physically deleted
        let retrieved = store.get("app", "messages", "user123", "msg001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_user() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        // Insert multiple rows for user123
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "Message 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg002",
                json!({"content": "Message 2"}),
            )
            .unwrap();

        // Insert row for different user
        store
            .put(
                "app",
                "messages",
                "user456",
                "msg003",
                json!({"content": "Message 3"}),
            )
            .unwrap();

        let results = store.scan_user("app", "messages", "user123").unwrap();
        assert_eq!(results.len(), 2);

        let row_ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
        assert!(row_ids.contains(&"msg001".to_string()));
        assert!(row_ids.contains(&"msg002".to_string()));
    }

    #[test]
    fn test_scan_user_filters_soft_deleted() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "Message 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg002",
                json!({"content": "Message 2"}),
            )
            .unwrap();

        // Soft delete one row
        store
            .delete("app", "messages", "user123", "msg001", false)
            .unwrap();

        let results = store.scan_user("app", "messages", "user123").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "msg002");
    }

    #[test]
    fn test_user_isolation() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "User 123 message"}),
            )
            .unwrap();

        // Different user shouldn't see the row
        let retrieved = store.get("app", "messages", "user456", "msg001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_alias() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "Message 1"}),
            )
            .unwrap();

        // scan() should work the same as scan_user()
        let results = store.scan("app", "messages", "user123").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "msg001");
    }

    #[test]
    fn test_scan_all() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        // Insert rows for multiple users
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "User 123 - Message 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg002",
                json!({"content": "User 123 - Message 2"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user456",
                "msg003",
                json!({"content": "User 456 - Message 1"}),
            )
            .unwrap();

        let results = store.scan_all("app", "messages").unwrap();
        assert_eq!(results.len(), 3);

        // Verify we have rows from both users
        let user_ids: Vec<String> = results.iter().map(|(uid, _, _)| uid.clone()).collect();
        assert!(user_ids.contains(&"user123".to_string()));
        assert!(user_ids.contains(&"user456".to_string()));

        // Count rows per user
        let user123_count = user_ids.iter().filter(|&id| id == "user123").count();
        let user456_count = user_ids.iter().filter(|&id| id == "user456").count();
        assert_eq!(user123_count, 2);
        assert_eq!(user456_count, 1);
    }

    #[test]
    fn test_scan_all_filters_soft_deleted() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "Message 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg002",
                json!({"content": "Message 2"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user456",
                "msg003",
                json!({"content": "Message 3"}),
            )
            .unwrap();

        // Soft delete one row
        store
            .delete("app", "messages", "user123", "msg001", false)
            .unwrap();

        let results = store.scan_all("app", "messages").unwrap();
        assert_eq!(results.len(), 2);

        // Verify deleted row is not in results
        let row_ids: Vec<String> = results.iter().map(|(_, rid, _)| rid.clone()).collect();
        assert!(!row_ids.contains(&"msg001".to_string()));
        assert!(row_ids.contains(&"msg002".to_string()));
        assert!(row_ids.contains(&"msg003".to_string()));
    }

    #[test]
    fn test_get_rows_by_user() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        // Insert rows for multiple users
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "User 123 - Message 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg002",
                json!({"content": "User 123 - Message 2"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user456",
                "msg003",
                json!({"content": "User 456 - Message 1"}),
            )
            .unwrap();

        let rows_by_user = store.get_rows_by_user("app", "messages").unwrap();

        assert_eq!(rows_by_user.len(), 2);
        assert_eq!(rows_by_user.get("user123").unwrap().len(), 2);
        assert_eq!(rows_by_user.get("user456").unwrap().len(), 1);

        // Verify data
        let user123_rows = rows_by_user.get("user123").unwrap();
        assert_eq!(user123_rows[0].1["content"], "User 123 - Message 1");
        assert_eq!(user123_rows[1].1["content"], "User 123 - Message 2");
    }

    #[test]
    fn test_get_rows_by_user_filters_deleted() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "Message 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg002",
                json!({"content": "Message 2"}),
            )
            .unwrap();

        // Soft delete one row
        store
            .delete("app", "messages", "user123", "msg001", false)
            .unwrap();

        let rows_by_user = store.get_rows_by_user("app", "messages").unwrap();

        // Only 1 row should be returned (msg002)
        assert_eq!(rows_by_user.get("user123").unwrap().len(), 1);
        assert_eq!(
            rows_by_user.get("user123").unwrap()[0].1["content"],
            "Message 2"
        );
    }

    #[test]
    fn test_delete_batch_by_keys() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        // Insert rows
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "Message 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg002",
                json!({"content": "Message 2"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user456",
                "msg003",
                json!({"content": "Message 3"}),
            )
            .unwrap();

        // Get rows by user to get the raw keys
        let rows_by_user = store.get_rows_by_user("app", "messages").unwrap();
        let user123_keys: Vec<Vec<u8>> = rows_by_user
            .get("user123")
            .unwrap()
            .iter()
            .map(|(k, _)| k.clone())
            .collect();

        // Delete user123's rows
        store
            .delete_batch_by_keys("app", "messages", &user123_keys)
            .unwrap();

        // Verify user123's rows are deleted
        assert!(store
            .get("app", "messages", "user123", "msg001")
            .unwrap()
            .is_none());
        assert!(store
            .get("app", "messages", "user123", "msg002")
            .unwrap()
            .is_none());

        // Verify user456's row still exists
        assert!(store
            .get("app", "messages", "user456", "msg003")
            .unwrap()
            .is_some());
    }

    #[test]
    fn test_drop_table_deletes_all_user_data() {
        let (db, _temp_dir) = create_test_db();
        let store = UserTableStore::new(db).unwrap();

        // Insert data for multiple users
        store
            .put(
                "app",
                "messages",
                "user123",
                "msg001",
                json!({"content": "Message 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user456",
                "msg002",
                json!({"content": "Message 2"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "messages",
                "user789",
                "msg003",
                json!({"content": "Message 3"}),
            )
            .unwrap();

        // Verify data exists
        assert!(store
            .get("app", "messages", "user123", "msg001")
            .unwrap()
            .is_some());
        assert!(store
            .get("app", "messages", "user456", "msg002")
            .unwrap()
            .is_some());
        assert!(store
            .get("app", "messages", "user789", "msg003")
            .unwrap()
            .is_some());

        // Drop the entire table
        store.drop_table("app", "messages").unwrap();

        // Verify all data is deleted
        assert!(store
            .get("app", "messages", "user123", "msg001")
            .unwrap()
            .is_none());
        assert!(store
            .get("app", "messages", "user456", "msg002")
            .unwrap()
            .is_none());
        assert!(store
            .get("app", "messages", "user789", "msg003")
            .unwrap()
            .is_none());
    }
}
