//! Shared table storage using EntityStore trait.
//!
//! Provides strongly-typed storage for cross-user shared tables with:
//! - Access control (public, private, restricted)
//! - System column injection (_updated, _deleted)
//! - Dynamic partition management (one partition per table)
//!
//! ## Key Format
//!
//! Keys use the format: `{row_id}` (no user isolation)
//!
//! ## Partition Names
//!
//! Partitions are named: `shared_{namespace_id}:{table_name}`

use crate::models::SharedTableRow;
use chrono::Utc;
use kalamdb_commons::storage::{Partition, Result as StorageResult, StorageBackend, StorageError};
use kalamdb_commons::models::{NamespaceId, TableName};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Shared table storage with access control.
///
/// Provides storage for tables that can be shared across users with configurable
/// access levels (public, private, restricted).
pub struct SharedTableStore {
    backend: Arc<dyn StorageBackend>,
}

impl SharedTableStore {
    /// Create a new shared table store.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// Generate the Partition for a shared table (type-safe identifiers).
    fn partition(namespace_id: &NamespaceId, table_name: &TableName) -> Partition {
        Partition::new(format!(
            "{}{}:{}",
            kalamdb_commons::constants::ColumnFamilyNames::SHARED_TABLE_PREFIX,
            namespace_id.as_str(),
            table_name.as_str()
        ))
    }

    /// Ensure partition exists for a table.
    fn ensure_partition(&self, namespace_id: &str, table_name: &str) -> StorageResult<()> {
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));
        self.backend.create_partition(&partition)
    }

    /// Create a column family for a shared table.
    ///
    /// This should be called during CREATE SHARED TABLE to initialize storage.
    /// Returns Ok(()) if the partition was created successfully or already exists.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    pub fn create_column_family(&self, namespace_id: &str, table_name: &str) -> StorageResult<()> {
        self.ensure_partition(namespace_id, table_name)
    }

    /// Insert or update a row in a shared table.
    ///
    /// Automatically injects system columns:
    /// - `_updated`: Current timestamp
    /// - `_deleted`: false
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    /// * `row_id` - Row identifier
    /// * `row_data` - Row data as JSON (system columns added automatically)
    /// * `access_level` - Access level for the row (public, private, restricted)
    pub fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        mut row_data: JsonValue,
        access_level: &str,
    ) -> StorageResult<()> {
        self.ensure_partition(namespace_id, table_name)?;

        // Extract system columns from row_data (they should already be there from SharedTableProvider)
        let _updated_str = if let Some(obj) = row_data.as_object_mut() {
            // Extract _updated and remove from fields to avoid duplication
            let updated = obj.remove("_updated")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            
            // Extract _deleted and remove from fields
            let _deleted_val = obj.remove("_deleted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            
            updated
        } else {
            Utc::now().to_rfc3339()
        };

        // Convert to SharedTableRow (fields no longer contain _updated/_deleted)
        let shared_table_row = SharedTableRow {
            fields: row_data.as_object()
                .ok_or_else(|| StorageError::SerializationError("Row data must be an object".to_string()))?
                .clone(),
            access_level: access_level.to_string(),
            _updated: _updated_str,
            _deleted: false,
        };

        // Store using partition
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));
        let value = serde_json::to_vec(&shared_table_row)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        self.backend.put(&partition, row_id.as_bytes(), &value)
    }

    /// Get a row from a shared table.
    ///
    /// Returns `None` if the row doesn't exist or is soft-deleted.
    pub fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
    ) -> StorageResult<Option<JsonValue>> {
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));

        match self.backend.get(&partition, row_id.as_bytes())? {
            Some(bytes) => {
                let shared_table_row: SharedTableRow = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                // Filter out soft-deleted rows
                if shared_table_row._deleted {
                    return Ok(None);
                }

                // Convert back to JSON value
                let mut json_obj = JsonValue::Object(shared_table_row.fields);
                if let Some(obj) = json_obj.as_object_mut() {
                    obj.insert("access_level".to_string(), JsonValue::String(shared_table_row.access_level));
                    obj.insert("_updated".to_string(), JsonValue::String(shared_table_row._updated));
                    obj.insert("_deleted".to_string(), JsonValue::Bool(shared_table_row._deleted));
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
        row_id: &str,
    ) -> StorageResult<Option<JsonValue>> {
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));

        match self.backend.get(&partition, row_id.as_bytes())? {
            Some(bytes) => {
                let shared_table_row: SharedTableRow = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                // Convert back to JSON value
                let mut json_obj = JsonValue::Object(shared_table_row.fields);
                if let Some(obj) = json_obj.as_object_mut() {
                    obj.insert("access_level".to_string(), JsonValue::String(shared_table_row.access_level));
                    obj.insert("_updated".to_string(), JsonValue::String(shared_table_row._updated));
                    obj.insert("_deleted".to_string(), JsonValue::Bool(shared_table_row._deleted));
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
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));

        if hard {
            // Physical deletion
            self.backend.delete(&partition, row_id.as_bytes())
        } else {
            // Soft delete: update the row with _deleted = true
            match self.backend.get(&partition, row_id.as_bytes())? {
                Some(bytes) => {
                    let mut shared_table_row: SharedTableRow = serde_json::from_slice(&bytes)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                    shared_table_row._deleted = true;
                    shared_table_row._updated = Utc::now().to_rfc3339();

                    let value = serde_json::to_vec(&shared_table_row)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                    self.backend.put(&partition, row_id.as_bytes(), &value)
                }
                None => Ok(()), // Idempotent - deleting non-existent row is OK
            }
        }
    }

    /// Scan all rows in a shared table.
    ///
    /// Returns a vector of `(row_id, row_data)` pairs.
    /// Filters out soft-deleted rows.
    pub fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> StorageResult<Vec<(String, JsonValue)>> {
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));

        let iter = self.backend.scan(&partition, None, None)?;
        let mut results = Vec::new();

        for (key_bytes, value_bytes) in iter {
            let key = String::from_utf8(key_bytes)
                .map_err(|e| StorageError::SerializationError(format!("Invalid UTF-8 key: {}", e)))?;

            let shared_table_row: SharedTableRow = serde_json::from_slice(&value_bytes)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            // Filter out soft-deleted rows
            if shared_table_row._deleted {
                continue;
            }

            // Convert back to JSON value
            let mut json_obj = JsonValue::Object(shared_table_row.fields);
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert("access_level".to_string(), JsonValue::String(shared_table_row.access_level));
                obj.insert("_updated".to_string(), JsonValue::String(shared_table_row._updated));
                obj.insert("_deleted".to_string(), JsonValue::Bool(shared_table_row._deleted));
            }

            results.push((key, json_obj));
        }

        Ok(results)
    }

    /// Streaming iterator over raw key/value bytes for a shared table.
    ///
    /// Uses the backend's snapshot-backed scan to provide a consistent view.
    pub fn scan_iter(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> StorageResult<Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_>> {
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));
        self.backend.scan(&partition, None, None)
    }

    /// Get all rows for flush operations.
    ///
    /// Returns a vector of `(key_bytes, row_data)` pairs.
    /// Filters out soft-deleted rows (they should not be flushed).
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
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));

        let iter = self.backend.scan(&partition, None, None)?;
        let mut results = Vec::new();

        for (key_bytes, value_bytes) in iter {
            let shared_table_row: SharedTableRow = serde_json::from_slice(&value_bytes)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            // Skip soft-deleted rows (don't flush them)
            if shared_table_row._deleted {
                continue;
            }

            // Convert back to JSON value
            let mut json_obj = JsonValue::Object(shared_table_row.fields);
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert("access_level".to_string(), JsonValue::String(shared_table_row.access_level));
                obj.insert("_updated".to_string(), JsonValue::String(shared_table_row._updated));
                obj.insert("_deleted".to_string(), JsonValue::Bool(shared_table_row._deleted));
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
        let partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));

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
        let _partition = Self::partition(&NamespaceId::from(namespace_id), &TableName::from(table_name));

        // Note: This is a simplified implementation. In a real system,
        // you might want to iterate and delete all keys, or mark the partition as dropped.
        // For now, we'll assume the backend handles partition deletion.
        // The current implementation just ensures the partition exists but doesn't provide
        // a way to delete it. This would need to be added to the StorageBackend trait.

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;
    use serde_json::json;

    fn create_test_store() -> SharedTableStore {
        let backend = Arc::new(InMemoryBackend::new());
        SharedTableStore::new(backend)
    }

    #[test]
    fn test_put_and_get() {
        let store = create_test_store();

        let row_data = json!({
            "title": "Shared Document",
            "content": "This is shared content"
        });

        store
            .put("app", "documents", "doc001", row_data.clone(), "public")
            .unwrap();

        let retrieved = store.get("app", "documents", "doc001").unwrap();
        assert!(retrieved.is_some());

        let retrieved_data = retrieved.unwrap();
        assert_eq!(retrieved_data["title"], "Shared Document");
        assert_eq!(retrieved_data["content"], "This is shared content");
        assert_eq!(retrieved_data["access_level"], "public");
        assert!(retrieved_data["_updated"].is_string());
        assert_eq!(retrieved_data["_deleted"], false);
    }

    #[test]
    fn test_soft_delete() {
        let store = create_test_store();

        let row_data = json!({
            "title": "Document",
            "content": "Content"
        });

        store
            .put("app", "documents", "doc001", row_data, "private")
            .unwrap();

        // Soft delete
        store
            .delete("app", "documents", "doc001", false)
            .unwrap();

        // Should return None because row is soft-deleted
        let retrieved = store.get("app", "documents", "doc001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_hard_delete() {
        let store = create_test_store();

        let row_data = json!({
            "title": "Document",
            "content": "Content"
        });

        store
            .put("app", "documents", "doc001", row_data, "public")
            .unwrap();

        // Hard delete
        store
            .delete("app", "documents", "doc001", true)
            .unwrap();

        // Should return None because row is physically deleted
        let retrieved = store.get("app", "documents", "doc001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan() {
        let store = create_test_store();

        // Insert multiple rows
        store
            .put(
                "app",
                "documents",
                "doc001",
                json!({"title": "Doc 1"}),
                "public",
            )
            .unwrap();
        store
            .put(
                "app",
                "documents",
                "doc002",
                json!({"title": "Doc 2"}),
                "private",
            )
            .unwrap();

        let results = store.scan("app", "documents").unwrap();
        assert_eq!(results.len(), 2);

        let row_ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
        assert!(row_ids.contains(&"doc001".to_string()));
        assert!(row_ids.contains(&"doc002".to_string()));
    }

    #[test]
    fn test_scan_filters_soft_deleted() {
        let store = create_test_store();

        store
            .put(
                "app",
                "documents",
                "doc001",
                json!({"title": "Doc 1"}),
                "public",
            )
            .unwrap();
        store
            .put(
                "app",
                "documents",
                "doc002",
                json!({"title": "Doc 2"}),
                "private",
            )
            .unwrap();

        // Soft delete one row
        store
            .delete("app", "documents", "doc001", false)
            .unwrap();

        let results = store.scan("app", "documents").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "doc002");
    }

    #[test]
    fn test_access_level_preservation() {
        let store = create_test_store();

        store
            .put(
                "app",
                "documents",
                "doc001",
                json!({"title": "Private Doc"}),
                "private",
            )
            .unwrap();

        let retrieved = store.get("app", "documents", "doc001").unwrap().unwrap();
        assert_eq!(retrieved["access_level"], "private");
    }
}
