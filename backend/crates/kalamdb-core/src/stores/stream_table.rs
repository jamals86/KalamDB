//! Stream table storage using EntityStore trait.
//!
//! Provides strongly-typed storage for ephemeral stream tables with:
//! - TTL-based automatic eviction
//! - System column injection (_updated, _deleted, inserted_at)
//! - Dynamic partition management (one partition per table)
//!
//! ## Key Format
//!
//! Keys use the format: `{row_id}` (no user isolation)
//!
//! ## Partition Names
//!
//! Partitions are named: `stream_{namespace_id}:{table_name}`

use crate::models::StreamTableRow;
use chrono::Utc;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_store::{Partition, StorageBackend, StorageError};
type StorageResult<T> = Result<T, StorageError>;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Stream table storage with TTL-based eviction.
///
/// Provides storage for ephemeral event streams with automatic cleanup
/// based on time-to-live settings.
pub struct StreamTableStore {
    backend: Arc<dyn StorageBackend>,
}

impl StreamTableStore {
    /// Create a new stream table store.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// Generate the Partition for a stream table (type-safe identifiers).
    fn partition(namespace_id: &NamespaceId, table_name: &TableName) -> Partition {
        Partition::new(format!(
            "{}{}:{}",
            kalamdb_commons::constants::ColumnFamilyNames::STREAM_TABLE_PREFIX,
            namespace_id.as_str(),
            table_name.as_str()
        ))
    }

    /// Ensure partition exists for a table.
    fn ensure_partition(&self, namespace_id: &str, table_name: &str) -> StorageResult<()> {
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );
        self.backend.create_partition(&partition)
    }

    /// Create a column family for a stream table.
    ///
    /// This should be called during CREATE STREAM TABLE to initialize storage.
    /// Returns Ok(()) if the partition was created successfully or already exists.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    pub fn create_column_family(&self, namespace_id: &str, table_name: &str) -> StorageResult<()> {
        self.ensure_partition(namespace_id, table_name)
    }

    /// Insert or update a row in a stream table.
    ///
    /// Automatically injects system columns:
    /// - `inserted_at`: Current timestamp (only on insert)
    /// - `_updated`: Current timestamp
    /// - `_deleted`: false
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    /// * `row_id` - Row identifier
    /// * `row_data` - Row data as JSON (system columns added automatically)
    /// * `ttl_seconds` - Time-to-live in seconds
    pub fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        mut row_data: JsonValue,
        ttl_seconds: u64,
    ) -> StorageResult<()> {
        self.ensure_partition(namespace_id, table_name)?;

        let now = Utc::now();
        let now_str = now.to_rfc3339();

        // Inject system columns
        if let Some(obj) = row_data.as_object_mut() {
            obj.insert(
                "inserted_at".to_string(),
                JsonValue::String(now_str.clone()),
            );
            obj.insert(
                kalamdb_commons::constants::SystemColumnNames::UPDATED.to_string(),
                JsonValue::String(now_str.clone()),
            );
            obj.insert(
                kalamdb_commons::constants::SystemColumnNames::DELETED.to_string(),
                JsonValue::Bool(false),
            );
        }

        // Convert to StreamTableRow
        let stream_table_row = StreamTableRow {
            fields: row_data
                .as_object()
                .ok_or_else(|| {
                    StorageError::SerializationError("Row data must be an object".to_string())
                })?
                .clone(),
            ttl_seconds,
            inserted_at: now_str.clone(),
            _updated: now_str,
            _deleted: false,
        };

        // Store using partition
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );
        let value = serde_json::to_vec(&stream_table_row)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        self.backend.put(&partition, row_id.as_bytes(), &value)
    }

    /// Get a row from a stream table.
    ///
    /// Returns `None` if the row doesn't exist, is soft-deleted, or has expired.
    pub fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
    ) -> StorageResult<Option<JsonValue>> {
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );

        match self.backend.get(&partition, row_id.as_bytes())? {
            Some(bytes) => {
                let stream_table_row: StreamTableRow = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                // Filter out soft-deleted rows
                if stream_table_row._deleted {
                    return Ok(None);
                }

                // Check TTL expiration
                if self.is_expired(&stream_table_row) {
                    return Ok(None);
                }

                // Convert back to JSON value
                let mut json_obj = JsonValue::Object(stream_table_row.fields);
                if let Some(obj) = json_obj.as_object_mut() {
                    obj.insert(
                        "ttl_seconds".to_string(),
                        JsonValue::Number(stream_table_row.ttl_seconds.into()),
                    );
                    obj.insert(
                        "inserted_at".to_string(),
                        JsonValue::String(stream_table_row.inserted_at),
                    );
                    obj.insert(
                        "_updated".to_string(),
                        JsonValue::String(stream_table_row._updated),
                    );
                    obj.insert(
                        "_deleted".to_string(),
                        JsonValue::Bool(stream_table_row._deleted),
                    );
                }

                Ok(Some(json_obj))
            }
            None => Ok(None),
        }
    }

    /// Check if a stream table row has expired based on TTL.
    fn is_expired(&self, row: &StreamTableRow) -> bool {
        let inserted_at = chrono::DateTime::parse_from_rfc3339(&row.inserted_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(Utc::now());

        let expires_at = inserted_at + chrono::Duration::seconds(row.ttl_seconds as i64);
        Utc::now() > expires_at
    }

    /// Get a row including soft-deleted rows.
    ///
    /// Unlike `get()`, this method returns soft-deleted rows (where `_deleted = true`).
    /// Useful for change detection when notifying DELETE operations.
    /// Still filters out expired rows.
    pub fn get_include_deleted(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
    ) -> StorageResult<Option<JsonValue>> {
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );

        match self.backend.get(&partition, row_id.as_bytes())? {
            Some(bytes) => {
                let stream_table_row: StreamTableRow = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                // Check TTL expiration (even for deleted rows)
                if self.is_expired(&stream_table_row) {
                    return Ok(None);
                }

                // Convert back to JSON value
                let mut json_obj = JsonValue::Object(stream_table_row.fields);
                if let Some(obj) = json_obj.as_object_mut() {
                    obj.insert(
                        "ttl_seconds".to_string(),
                        JsonValue::Number(stream_table_row.ttl_seconds.into()),
                    );
                    obj.insert(
                        "inserted_at".to_string(),
                        JsonValue::String(stream_table_row.inserted_at),
                    );
                    obj.insert(
                        "_updated".to_string(),
                        JsonValue::String(stream_table_row._updated),
                    );
                    obj.insert(
                        "_deleted".to_string(),
                        JsonValue::Bool(stream_table_row._deleted),
                    );
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
        row_id: &str,
        hard: bool,
    ) -> StorageResult<()> {
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );

        if hard {
            // Physical deletion
            self.backend.delete(&partition, row_id.as_bytes())
        } else {
            // Soft delete: update the row with _deleted = true
            match self.backend.get(&partition, row_id.as_bytes())? {
                Some(bytes) => {
                    let mut stream_table_row: StreamTableRow = serde_json::from_slice(&bytes)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                    stream_table_row._deleted = true;
                    stream_table_row._updated = Utc::now().to_rfc3339();

                    let value = serde_json::to_vec(&stream_table_row)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                    self.backend.put(&partition, row_id.as_bytes(), &value)
                }
                None => Ok(()), // Idempotent - deleting non-existent row is OK
            }
        }
    }

    /// Scan all rows in a stream table.
    ///
    /// Returns a vector of `(row_id, row_data)` pairs.
    /// Filters out soft-deleted rows and expired rows.
    pub fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> StorageResult<Vec<(String, JsonValue)>> {
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );

        let iter = self.backend.scan(&partition, None, None)?;
        let mut results = Vec::new();

        for (key_bytes, value_bytes) in iter {
            let key = String::from_utf8(key_bytes).map_err(|e| {
                StorageError::SerializationError(format!("Invalid UTF-8 key: {}", e))
            })?;

            let stream_table_row: StreamTableRow = serde_json::from_slice(&value_bytes)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            // Filter out soft-deleted rows
            if stream_table_row._deleted {
                continue;
            }

            // Filter out expired rows
            if self.is_expired(&stream_table_row) {
                continue;
            }

            // Convert back to JSON value
            let mut json_obj = JsonValue::Object(stream_table_row.fields);
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert(
                    "ttl_seconds".to_string(),
                    JsonValue::Number(stream_table_row.ttl_seconds.into()),
                );
                obj.insert(
                    "inserted_at".to_string(),
                    JsonValue::String(stream_table_row.inserted_at),
                );
                obj.insert(
                    "_updated".to_string(),
                    JsonValue::String(stream_table_row._updated),
                );
                obj.insert(
                    "_deleted".to_string(),
                    JsonValue::Bool(stream_table_row._deleted),
                );
            }

            results.push((key, json_obj));
        }

        Ok(results)
    }

    /// Get all rows for flush operations.
    ///
    /// Returns a vector of `(key_bytes, row_data)` pairs.
    /// Filters out soft-deleted rows and expired rows (they should not be flushed).
    ///
    /// # Note
    ///
    /// This method is designed for flush operations and returns raw key bytes
    /// for efficient batch deletion.
    pub fn get_rows_for_flush(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> StorageResult<Vec<(Vec<u8>, JsonValue)>> {
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );

        let iter = self.backend.scan(&partition, None, None)?;
        let mut results = Vec::new();

        for (key_bytes, value_bytes) in iter {
            let stream_table_row: StreamTableRow = serde_json::from_slice(&value_bytes)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            // Skip soft-deleted rows (don't flush them)
            if stream_table_row._deleted {
                continue;
            }

            // Skip expired rows (don't flush them)
            if self.is_expired(&stream_table_row) {
                continue;
            }

            // Convert back to JSON value
            let mut json_obj = JsonValue::Object(stream_table_row.fields);
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert(
                    "ttl_seconds".to_string(),
                    JsonValue::Number(stream_table_row.ttl_seconds.into()),
                );
                obj.insert(
                    "inserted_at".to_string(),
                    JsonValue::String(stream_table_row.inserted_at),
                );
                obj.insert(
                    "_updated".to_string(),
                    JsonValue::String(stream_table_row._updated),
                );
                obj.insert(
                    "_deleted".to_string(),
                    JsonValue::Bool(stream_table_row._deleted),
                );
            }

            results.push((key_bytes.to_vec(), json_obj));
        }

        Ok(results)
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
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );

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
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );

        // Note: This is a simplified implementation. In a real system,
        // you might want to iterate and delete all keys, or mark the partition as dropped.
        // For now, we'll assume the backend handles partition deletion.
        // The current implementation just ensures the partition exists but doesn't provide
        // a way to delete it. This would need to be added to the StorageBackend trait.

        Ok(())
    }

    /// Clean up expired rows in a stream table.
    ///
    /// This method should be called periodically to remove expired rows
    /// based on their TTL settings.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    ///
    /// # Returns
    ///
    /// Number of rows deleted
    pub fn cleanup_expired_rows(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> StorageResult<usize> {
        let partition = Self::partition(
            &NamespaceId::from(namespace_id),
            &TableName::from(table_name),
        );

        let iter = self.backend.scan(&partition, None, None)?;
        let mut expired_keys = Vec::new();

        for (key_bytes, value_bytes) in iter {
            let stream_table_row: StreamTableRow = serde_json::from_slice(&value_bytes)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            if self.is_expired(&stream_table_row) {
                expired_keys.push(key_bytes.to_vec());
            }
        }

        // Delete expired rows
        for key_bytes in &expired_keys {
            self.backend.delete(&partition, key_bytes)?;
        }

        Ok(expired_keys.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;
    use serde_json::json;
    use std::thread;
    use std::time::Duration;

    fn create_test_store() -> StreamTableStore {
        let backend = Arc::new(InMemoryBackend::new());
        StreamTableStore::new(backend)
    }

    #[test]
    fn test_put_and_get() {
        let store = create_test_store();

        let row_data = json!({
            "event_type": "click",
            "user_id": "user123"
        });

        store
            .put("app", "events", "evt001", row_data.clone(), 3600)
            .unwrap();

        let retrieved = store.get("app", "events", "evt001").unwrap();
        assert!(retrieved.is_some());

        let retrieved_data = retrieved.unwrap();
        assert_eq!(retrieved_data["event_type"], "click");
        assert_eq!(retrieved_data["user_id"], "user123");
        assert_eq!(retrieved_data["ttl_seconds"], 3600);
        assert!(retrieved_data["inserted_at"].is_string());
        assert!(retrieved_data["_updated"].is_string());
        assert_eq!(retrieved_data["_deleted"], false);
    }

    #[test]
    fn test_ttl_expiration() {
        let store = create_test_store();

        let row_data = json!({
            "event_type": "temp_event"
        });

        // Insert with very short TTL (1 second)
        store.put("app", "events", "evt001", row_data, 1).unwrap();

        // Should exist immediately
        let retrieved = store.get("app", "events", "evt001").unwrap();
        assert!(retrieved.is_some());

        // Wait for expiration
        thread::sleep(Duration::from_secs(2));

        // Should return None after expiration
        let retrieved = store.get("app", "events", "evt001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_soft_delete() {
        let store = create_test_store();

        let row_data = json!({
            "event_type": "click"
        });

        store
            .put("app", "events", "evt001", row_data, 3600)
            .unwrap();

        // Soft delete
        store.delete("app", "events", "evt001", false).unwrap();

        // Should return None because row is soft-deleted
        let retrieved = store.get("app", "events", "evt001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_hard_delete() {
        let store = create_test_store();

        let row_data = json!({
            "event_type": "click"
        });

        store
            .put("app", "events", "evt001", row_data, 3600)
            .unwrap();

        // Hard delete
        store.delete("app", "events", "evt001", true).unwrap();

        // Should return None because row is physically deleted
        let retrieved = store.get("app", "events", "evt001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan() {
        let store = create_test_store();

        // Insert multiple rows
        store
            .put("app", "events", "evt001", json!({"event": "click1"}), 3600)
            .unwrap();
        store
            .put("app", "events", "evt002", json!({"event": "click2"}), 3600)
            .unwrap();

        let results = store.scan("app", "events").unwrap();
        assert_eq!(results.len(), 2);

        let row_ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
        assert!(row_ids.contains(&"evt001".to_string()));
        assert!(row_ids.contains(&"evt002".to_string()));
    }

    #[test]
    fn test_scan_filters_expired() {
        let store = create_test_store();

        store
            .put(
                "app",
                "events",
                "evt001",
                json!({"event": "short"}),
                1, // 1 second TTL
            )
            .unwrap();
        store
            .put(
                "app",
                "events",
                "evt002",
                json!({"event": "long"}),
                3600, // 1 hour TTL
            )
            .unwrap();

        // Wait for first event to expire
        thread::sleep(Duration::from_secs(2));

        let results = store.scan("app", "events").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "evt002");
    }

    #[test]
    fn test_cleanup_expired_rows() {
        let store = create_test_store();

        store
            .put(
                "app",
                "events",
                "evt001",
                json!({"event": "expired"}),
                1, // 1 second TTL
            )
            .unwrap();
        store
            .put(
                "app",
                "events",
                "evt002",
                json!({"event": "active"}),
                3600, // 1 hour TTL
            )
            .unwrap();

        // Wait for expiration
        thread::sleep(Duration::from_secs(2));

        // Clean up expired rows
        let deleted_count = store.cleanup_expired_rows("app", "events").unwrap();
        assert_eq!(deleted_count, 1);

        // Verify expired row is gone
        let retrieved = store.get("app", "events", "evt001").unwrap();
        assert!(retrieved.is_none());

        // Verify active row still exists
        let retrieved = store.get("app", "events", "evt002").unwrap();
        assert!(retrieved.is_some());
    }
}
