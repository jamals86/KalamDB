//! User table storage using EntityStore trait.
//!
//! Provides strongly-typed storage for user-scoped tables with automatic:
//! - User isolation (data scoped by user_id)
//! - System column injection (_updated, _deleted)
//! - Dynamic partition management (one partition per table)
//!
//! ## Key Format
//!
//! Keys use the format: `{user_id}:{row_id}`
//!
//! ## Partition Names
//!
//! Partitions are named: `user_{namespace_id}:{table_name}`

use crate::models::UserTableRow;
use chrono::Utc;
use kalamdb_commons::storage::{Partition, Result as StorageResult, StorageBackend, StorageError};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// User table storage with automatic system column injection.
///
/// Provides isolated storage for user-scoped tables with user data isolation.
/// Each table gets its own partition, and within each table, data is keyed by user_id:row_id.
pub struct UserTableStore {
    backend: Arc<dyn StorageBackend>,
}

impl UserTableStore {
    /// Create a new user table store.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// Generate partition name for a user table.
    fn partition_name(namespace_id: &str, table_name: &str) -> String {
        format!(
            "{}{}:{}",
            kalamdb_commons::constants::ColumnFamilyNames::USER_TABLE_PREFIX,
            namespace_id,
            table_name
        )
    }

    /// Ensure partition exists for a table.
    fn ensure_partition(&self, namespace_id: &str, table_name: &str) -> StorageResult<()> {
        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);
        self.backend.create_partition(&partition)
    }

    /// Create a column family for a user table.
    ///
    /// This should be called during CREATE USER TABLE to initialize storage.
    /// Returns Ok(()) if the partition was created successfully or already exists.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    pub fn create_column_family(&self, namespace_id: &str, table_name: &str) -> StorageResult<()> {
        self.ensure_partition(namespace_id, table_name)
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
    ) -> StorageResult<()> {
        self.ensure_partition(namespace_id, table_name)?;

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

        // Convert to UserTableRow
        let user_table_row = UserTableRow {
            fields: row_data.as_object()
                .ok_or_else(|| StorageError::SerializationError("Row data must be an object".to_string()))?
                .clone(),
            _updated: Utc::now().to_rfc3339(),
            _deleted: false,
        };

        // Create key: {user_id}:{row_id}
        let key = format!("{}:{}", user_id, row_id);

        // Store using EntityStore-like interface
        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);
        let value = serde_json::to_vec(&user_table_row)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        self.backend.put(&partition, key.as_bytes(), &value)
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
    ) -> StorageResult<Option<JsonValue>> {
        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);
        let key = format!("{}:{}", user_id, row_id);

        match self.backend.get(&partition, key.as_bytes())? {
            Some(bytes) => {
                let user_table_row: UserTableRow = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                // Filter out soft-deleted rows
                if user_table_row._deleted {
                    return Ok(None);
                }

                // Convert back to JSON value
                let mut json_obj = JsonValue::Object(user_table_row.fields);
                if let Some(obj) = json_obj.as_object_mut() {
                    obj.insert("_updated".to_string(), JsonValue::String(user_table_row._updated));
                    obj.insert("_deleted".to_string(), JsonValue::Bool(user_table_row._deleted));
                }

                Ok(Some(json_obj))
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
    ) -> StorageResult<Option<JsonValue>> {
        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);
        let key = format!("{}:{}", user_id, row_id);

        match self.backend.get(&partition, key.as_bytes())? {
            Some(bytes) => {
                let user_table_row: UserTableRow = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                // Convert back to JSON value
                let mut json_obj = JsonValue::Object(user_table_row.fields);
                if let Some(obj) = json_obj.as_object_mut() {
                    obj.insert("_updated".to_string(), JsonValue::String(user_table_row._updated));
                    obj.insert("_deleted".to_string(), JsonValue::Bool(user_table_row._deleted));
                }

                Ok(Some(json_obj))
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
    ) -> StorageResult<()> {
        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);
        let key = format!("{}:{}", user_id, row_id);

        if hard {
            // Physical deletion
            self.backend.delete(&partition, key.as_bytes())
        } else {
            // Soft delete: update the row with _deleted = true
            match self.backend.get(&partition, key.as_bytes())? {
                Some(bytes) => {
                    let mut user_table_row: UserTableRow = serde_json::from_slice(&bytes)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                    user_table_row._deleted = true;
                    user_table_row._updated = Utc::now().to_rfc3339();

                    let value = serde_json::to_vec(&user_table_row)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                    self.backend.put(&partition, key.as_bytes(), &value)
                }
                None => Ok(()), // Idempotent - deleting non-existent row is OK
            }
        }
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
    ) -> StorageResult<Vec<(String, JsonValue)>> {
        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);
        let prefix = format!("{}:", user_id);

        let iter = self.backend.scan(&partition, Some(prefix.as_bytes()), None)?;
        let mut results = Vec::new();

        for (key_bytes, value_bytes) in iter {
            let key = String::from_utf8(key_bytes)
                .map_err(|e| StorageError::SerializationError(format!("Invalid UTF-8 key: {}", e)))?;

            // Only include rows with matching user_id prefix
            if !key.starts_with(&prefix) {
                continue;
            }

            let user_table_row: UserTableRow = serde_json::from_slice(&value_bytes)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            // Filter out soft-deleted rows
            if user_table_row._deleted {
                continue;
            }

            // Convert back to JSON value
            let mut json_obj = JsonValue::Object(user_table_row.fields);
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert("_updated".to_string(), JsonValue::String(user_table_row._updated));
                obj.insert("_deleted".to_string(), JsonValue::Bool(user_table_row._deleted));
            }

            let (_, row_id) = Self::parse_user_key(&key)?;
            results.push((row_id, json_obj));
        }

        Ok(results)
    }

    /// Scan all rows for a specific user (alias for scan_user).
    pub fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
    ) -> StorageResult<Vec<(String, JsonValue)>> {
        self.scan_user(namespace_id, table_name, user_id)
    }

    /// Parse user key into (user_id, row_id).
    fn parse_user_key(key: &str) -> StorageResult<(String, String)> {
        let parts: Vec<&str> = key.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(StorageError::SerializationError(
                format!("Invalid user key format: {}", key)
            ));
        }
        Ok((parts[0].to_string(), parts[1].to_string()))
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
    ) -> StorageResult<std::collections::HashMap<String, Vec<(Vec<u8>, JsonValue)>>> {
        use std::collections::HashMap;

        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);

        let iter = self.backend.scan(&partition, None, None)?;
        let mut rows_by_user: HashMap<String, Vec<(Vec<u8>, JsonValue)>> = HashMap::new();

        for (key_bytes, value_bytes) in iter {
            let user_table_row: UserTableRow = serde_json::from_slice(&value_bytes)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            // Skip soft-deleted rows (don't flush them)
            if user_table_row._deleted {
                continue;
            }

            // Parse key to get user_id
            let key_str = String::from_utf8(key_bytes.to_vec())
                .map_err(|e| StorageError::SerializationError(format!("Invalid UTF-8 key: {}", e)))?;
            let (user_id, _) = Self::parse_user_key(&key_str)?;

            // Convert back to JSON value
            let mut json_obj = JsonValue::Object(user_table_row.fields);
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert("_updated".to_string(), JsonValue::String(user_table_row._updated));
                obj.insert("_deleted".to_string(), JsonValue::Bool(user_table_row._deleted));
            }

            rows_by_user
                .entry(user_id)
                .or_default()
                .push((key_bytes.to_vec(), json_obj));
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
    ) -> StorageResult<()> {
        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);

        for key_bytes in keys {
            self.backend.delete(&partition, key_bytes)?;
        }
        Ok(())
    }

    /// Drop entire table by deleting its partition.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - The namespace identifier
    /// * `table_name` - The table name
    pub fn drop_table(&self, namespace_id: &str, table_name: &str) -> StorageResult<()> {
        let _partition_name = Self::partition_name(namespace_id, table_name);

        // Note: This is a simplified implementation. In a real system,
        // you might want to iterate and delete all keys, or mark the partition as dropped.
        // For now, we'll assume the backend handles partition deletion.
        // The current implementation just ensures the partition exists but doesn't provide
        // a way to delete it. This would need to be added to the StorageBackend trait.

        Ok(())
    }

    /// Scan all rows for a table across all users (for admin/stats operations).
    pub fn scan_all(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> StorageResult<Vec<(String, JsonValue)>> {
        let partition_name = Self::partition_name(namespace_id, table_name);
        let partition = Partition::new(&partition_name);

        let iter = self.backend.scan(&partition, None, None)?;
        let mut results = Vec::new();

        for (key_bytes, value_bytes) in iter {
            let user_table_row: UserTableRow = serde_json::from_slice(&value_bytes)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            // Skip soft-deleted rows
            if user_table_row._deleted {
                continue;
            }

            let key_str = String::from_utf8(key_bytes)
                .map_err(|e| StorageError::SerializationError(format!("Invalid UTF-8 key: {}", e)))?;

            // Convert back to JSON value
            let mut json_obj = JsonValue::Object(user_table_row.fields);
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert("_updated".to_string(), JsonValue::String(user_table_row._updated));
                obj.insert("_deleted".to_string(), JsonValue::Bool(user_table_row._deleted));
            }

            results.push((key_str, json_obj));
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;
    use serde_json::json;

    fn create_test_store() -> UserTableStore {
        let backend = Arc::new(InMemoryBackend::new());
        UserTableStore::new(backend)
    }

    #[test]
    fn test_put_and_get() {
        let store = create_test_store();

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
        let store = create_test_store();

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
        let store = create_test_store();

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
        let store = create_test_store();

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
        let store = create_test_store();

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
        let store = create_test_store();

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
        let store = create_test_store();

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
}
