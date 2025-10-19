//! User table storage operations.
//!
//! Provides isolated storage for user-scoped tables with automatic system column injection.
//! Each user's data is isolated with key format `{user_id}:{row_id}`.

use anyhow::{Context, Result};
use chrono::Utc;
use rocksdb::{IteratorMode, DB};
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
                "_updated".to_string(),
                JsonValue::String(Utc::now().to_rfc3339()),
            );
            obj.insert("_deleted".to_string(), JsonValue::Bool(false));
        }

        let cf_name = format!("user_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

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
        let cf_name = format!("user_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let key = user_key(user_id, row_id);
        let value = self.db.get_cf(cf, key.as_bytes())?;

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
        let cf_name = format!("user_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let key = user_key(user_id, row_id);

        if hard {
            // Physical deletion
            self.db.delete_cf(cf, key.as_bytes())?;
        } else {
            // Soft delete: set _deleted = true, update _updated
            if let Some(bytes) = self.db.get_cf(cf, key.as_bytes())? {
                let mut row_data: JsonValue = serde_json::from_slice(&bytes)?;

                if let Some(obj) = row_data.as_object_mut() {
                    obj.insert("_deleted".to_string(), JsonValue::Bool(true));
                    obj.insert(
                        "_updated".to_string(),
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
        let cf_name = format!("user_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

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
                if let Some(deleted) = obj.get("_deleted") {
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
        assert!(retrieved_data["_updated"].is_string());
        assert_eq!(retrieved_data["_deleted"], false);
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
}
