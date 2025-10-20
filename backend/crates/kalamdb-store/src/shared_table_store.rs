//! Shared table storage operations.
//!
//! Provides storage for shared tables accessible to all users.
//! Key format: `{row_id}` (no user isolation).

use anyhow::{Context, Result};
use chrono::Utc;
use rocksdb::{IteratorMode, DB};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Shared table storage with automatic system column injection.
///
/// # System Columns
///
/// All operations automatically inject:
/// - `_updated`: Current timestamp (ISO 8601)
/// - `_deleted`: Boolean flag (false by default)
///
/// # Key Format
///
/// Keys use the format: `{row_id}` (no user prefix)
pub struct SharedTableStore {
    db: Arc<DB>,
}

impl SharedTableStore {
    /// Create a new shared table store.
    pub fn new(db: Arc<DB>) -> Result<Self> {
        Ok(Self { db })
    }

    /// Insert or update a row.
    ///
    /// Automatically injects system columns:
    /// - `_updated`: Current timestamp
    /// - `_deleted`: false
    pub fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        mut row_data: JsonValue,
    ) -> Result<()> {
        // Inject system columns
        if let Some(obj) = row_data.as_object_mut() {
            obj.insert(
                "_updated".to_string(),
                JsonValue::String(Utc::now().to_rfc3339()),
            );
            obj.insert("_deleted".to_string(), JsonValue::Bool(false));
        }

        let cf_name = format!("shared_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let value = serde_json::to_vec(&row_data)?;
        self.db.put_cf(cf, row_id.as_bytes(), &value)?;
        Ok(())
    }

    /// Get a row.
    ///
    /// Returns `None` if the row doesn't exist or is soft-deleted.
    pub fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
    ) -> Result<Option<JsonValue>> {
        let cf_name = format!("shared_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let value = self.db.get_cf(cf, row_id.as_bytes())?;

        match value {
            Some(bytes) => {
                let row_data: JsonValue = serde_json::from_slice(&bytes)?;

                // Filter out soft-deleted rows
                if let Some(obj) = row_data.as_object() {
                    if let Some(deleted) = obj.get("_deleted") {
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
        row_id: &str,
    ) -> Result<Option<JsonValue>> {
        let cf_name = format!("shared_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let value = self.db.get_cf(cf, row_id.as_bytes())?;

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
        row_id: &str,
        hard: bool,
    ) -> Result<()> {
        let cf_name = format!("shared_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        if hard {
            // Physical deletion
            self.db.delete_cf(cf, row_id.as_bytes())?;
        } else {
            // Soft delete: set _deleted = true, update _updated
            if let Some(bytes) = self.db.get_cf(cf, row_id.as_bytes())? {
                let mut row_data: JsonValue = serde_json::from_slice(&bytes)?;

                if let Some(obj) = row_data.as_object_mut() {
                    obj.insert("_deleted".to_string(), JsonValue::Bool(true));
                    obj.insert(
                        "_updated".to_string(),
                        JsonValue::String(Utc::now().to_rfc3339()),
                    );
                }

                let value = serde_json::to_vec(&row_data)?;
                self.db.put_cf(cf, row_id.as_bytes(), &value)?;
            }
        }

        Ok(())
    }

    /// Scan all rows in the table.
    ///
    /// Returns a vector of `(row_id, row_data)` pairs.
    /// Filters out soft-deleted rows.
    pub fn scan(&self, namespace_id: &str, table_name: &str) -> Result<Vec<(String, JsonValue)>> {
        let cf_name = format!("shared_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let mut results = Vec::new();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (key_bytes, value_bytes) = item?;
            let row_id = String::from_utf8(key_bytes.to_vec())?;
            let row_data: JsonValue = serde_json::from_slice(&value_bytes)?;

            // Filter out soft-deleted rows
            if let Some(obj) = row_data.as_object() {
                if let Some(deleted) = obj.get("_deleted") {
                    if deleted.as_bool() == Some(true) {
                        continue;
                    }
                }
            }

            results.push((row_id, row_data));
        }

        Ok(results)
    }

    /// Delete multiple rows by their row IDs (batch delete).
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    /// * `row_ids` - Vector of row IDs to delete
    pub fn delete_batch_by_row_ids(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_ids: &[String],
    ) -> Result<()> {
        use rocksdb::WriteBatch;

        let cf_name = format!("shared_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let mut batch = WriteBatch::default();

        for row_id in row_ids {
            batch.delete_cf(cf, row_id.as_bytes());
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
    /// # use kalamdb_store::SharedTableStore;
    /// # use std::sync::Arc;
    /// # use rocksdb::DB;
    /// # let db = Arc::new(DB::open_default("test").unwrap());
    /// let store = SharedTableStore::new(db).unwrap();
    /// store.drop_table("app", "conversations").unwrap();
    /// ```
    pub fn drop_table(&self, namespace_id: &str, table_name: &str) -> Result<()> {
        let cf_name = format!("shared_table:{}:{}", namespace_id, table_name);
        
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

        let cf_names = vec!["shared_table:app:conversations"];
        let db = DB::open_cf(&opts, temp_dir.path(), &cf_names).unwrap();

        (Arc::new(db), temp_dir)
    }

    #[test]
    fn test_put_and_get() {
        let (db, _temp_dir) = create_test_db();
        let store = SharedTableStore::new(db).unwrap();

        let row_data = json!({
            "conversation_id": "conv001",
            "title": "Team Discussion"
        });

        store
            .put("app", "conversations", "conv001", row_data.clone())
            .unwrap();

        let retrieved = store.get("app", "conversations", "conv001").unwrap();
        assert!(retrieved.is_some());

        let retrieved_data = retrieved.unwrap();
        assert_eq!(retrieved_data["conversation_id"], "conv001");
        assert_eq!(retrieved_data["title"], "Team Discussion");
        assert!(retrieved_data["_updated"].is_string());
        assert_eq!(retrieved_data["_deleted"], false);
    }

    #[test]
    fn test_soft_delete() {
        let (db, _temp_dir) = create_test_db();
        let store = SharedTableStore::new(db).unwrap();

        let row_data = json!({"conversation_id": "conv001"});
        store
            .put("app", "conversations", "conv001", row_data)
            .unwrap();

        // Soft delete
        store
            .delete("app", "conversations", "conv001", false)
            .unwrap();

        // Should return None because row is soft-deleted
        let retrieved = store.get("app", "conversations", "conv001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_hard_delete() {
        let (db, _temp_dir) = create_test_db();
        let store = SharedTableStore::new(db).unwrap();

        let row_data = json!({"conversation_id": "conv001"});
        store
            .put("app", "conversations", "conv001", row_data)
            .unwrap();

        // Hard delete
        store
            .delete("app", "conversations", "conv001", true)
            .unwrap();

        // Should return None because row is physically deleted
        let retrieved = store.get("app", "conversations", "conv001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan() {
        let (db, _temp_dir) = create_test_db();
        let store = SharedTableStore::new(db).unwrap();

        store
            .put(
                "app",
                "conversations",
                "conv001",
                json!({"title": "Conversation 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "conversations",
                "conv002",
                json!({"title": "Conversation 2"}),
            )
            .unwrap();

        let results = store.scan("app", "conversations").unwrap();
        assert_eq!(results.len(), 2);

        let row_ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
        assert!(row_ids.contains(&"conv001".to_string()));
        assert!(row_ids.contains(&"conv002".to_string()));
    }

    #[test]
    fn test_scan_filters_soft_deleted() {
        let (db, _temp_dir) = create_test_db();
        let store = SharedTableStore::new(db).unwrap();

        store
            .put(
                "app",
                "conversations",
                "conv001",
                json!({"title": "Conversation 1"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "conversations",
                "conv002",
                json!({"title": "Conversation 2"}),
            )
            .unwrap();

        // Soft delete one row
        store
            .delete("app", "conversations", "conv001", false)
            .unwrap();

        let results = store.scan("app", "conversations").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "conv002");
    }

    #[test]
    fn test_drop_table_deletes_all_shared_data() {
        let (db, _temp_dir) = create_test_db();
        let store = SharedTableStore::new(db).unwrap();

        // Insert multiple rows
        store
            .put(
                "app",
                "conversations",
                "conv001",
                json!({"topic": "Tech"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "conversations",
                "conv002",
                json!({"topic": "Sports"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "conversations",
                "conv003",
                json!({"topic": "Politics"}),
            )
            .unwrap();

        // Verify data exists
        assert!(store
            .get("app", "conversations", "conv001")
            .unwrap()
            .is_some());
        assert!(store
            .get("app", "conversations", "conv002")
            .unwrap()
            .is_some());
        assert!(store
            .get("app", "conversations", "conv003")
            .unwrap()
            .is_some());

        // Drop the entire table
        store.drop_table("app", "conversations").unwrap();

        // Verify all data is deleted
        assert!(store
            .get("app", "conversations", "conv001")
            .unwrap()
            .is_none());
        assert!(store
            .get("app", "conversations", "conv002")
            .unwrap()
            .is_none());
        assert!(store
            .get("app", "conversations", "conv003")
            .unwrap()
            .is_none());
    }
}
