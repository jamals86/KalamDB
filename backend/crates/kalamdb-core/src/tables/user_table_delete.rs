//! User table DELETE operations (soft delete)
//!
//! This module handles DELETE operations for user tables with:
//! - Soft delete: Sets _deleted = true instead of removing data
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic _updated timestamp update
//! - Data isolation enforcement
//! - Atomic delete operations

use crate::catalog::{NamespaceId, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::storage::column_family_manager::ColumnFamilyManager;
use chrono::Utc;
use rocksdb::DB;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// User table DELETE handler
///
/// Implements soft delete for user tables by marking rows as deleted
/// instead of physically removing them. This allows for:
/// - Data recovery within retention period
/// - Audit trail maintenance
/// - Filtered queries (WHERE _deleted = false)
pub struct UserTableDeleteHandler {
    db: Arc<DB>,
}

impl UserTableDeleteHandler {
    /// Create a new user table DELETE handler
    ///
    /// # Arguments
    /// * `db` - RocksDB instance
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// Soft delete a single row in a user table
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `row_id` - Row ID to delete
    ///
    /// # Returns
    /// The row ID of the deleted row
    ///
    /// # Key Format
    /// RocksDB key format: `{UserId}:{row_id}`
    ///
    /// # System Columns
    /// Automatically updates:
    /// - `_deleted`: BOOLEAN = true
    /// - `_updated`: TIMESTAMP = NOW()
    pub fn delete_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
    ) -> Result<String, KalamDbError> {
        // Build RocksDB key
        let key = format!("{}:{}", user_id.as_str(), row_id);

        // Get column family handle
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(namespace_id),
            table_name,
        );
        let cf = self
            .db
            .cf_handle(&cf_name)
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Column family not found for table: {}.{}",
                    namespace_id.as_str(),
                    table_name.as_str()
                ))
            })?;

        // Read existing row
        let existing_bytes = self
            .db
            .get_cf(cf, key.as_bytes())
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to read row from RocksDB: {}", e))
            })?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Row not found: {} in table {}.{}",
                    row_id,
                    namespace_id.as_str(),
                    table_name.as_str()
                ))
            })?;

        let mut existing_row: JsonValue = serde_json::from_slice(&existing_bytes).map_err(|e| {
            KalamDbError::SerializationError(format!("Failed to deserialize existing row: {}", e))
        })?;

        // Set _deleted = true and _updated = NOW()
        let now_ms = Utc::now().timestamp_millis();
        if let Some(obj) = existing_row.as_object_mut() {
            obj.insert("_deleted".to_string(), JsonValue::Bool(true));
            obj.insert("_updated".to_string(), JsonValue::Number(now_ms.into()));
        } else {
            return Err(KalamDbError::SerializationError(
                "Existing row is not a JSON object".to_string(),
            ));
        }

        // Serialize updated row
        let updated_bytes = serde_json::to_vec(&existing_row).map_err(|e| {
            KalamDbError::SerializationError(format!("Failed to serialize updated row: {}", e))
        })?;

        // Write updated row back to RocksDB
        self.db
            .put_cf(cf, key.as_bytes(), &updated_bytes)
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to write updated row to RocksDB: {}", e))
            })?;

        log::debug!(
            "Soft deleted row {} in {}.{} for user {}",
            row_id,
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        Ok(row_id.to_string())
    }

    /// Soft delete multiple rows in batch
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `row_ids` - Vector of row IDs to delete
    ///
    /// # Returns
    /// Vector of deleted row IDs
    ///
    /// # Note
    /// This is atomic - either all deletes succeed or none do
    pub fn delete_batch(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_ids: Vec<String>,
    ) -> Result<Vec<String>, KalamDbError> {
        let mut deleted_row_ids = Vec::with_capacity(row_ids.len());

        // Get column family handle
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(namespace_id),
            table_name,
        );
        let cf = self
            .db
            .cf_handle(&cf_name)
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Column family not found for table: {}.{}",
                    namespace_id.as_str(),
                    table_name.as_str()
                ))
            })?;

        // Use WriteBatch for atomicity
        let mut batch = rocksdb::WriteBatch::default();

        let now_ms = Utc::now().timestamp_millis();

        for row_id in row_ids {
            // Build key
            let key = format!("{}:{}", user_id.as_str(), row_id);

            // Read existing row
            let existing_bytes = self
                .db
                .get_cf(cf, key.as_bytes())
                .map_err(|e| {
                    KalamDbError::Other(format!("Failed to read row from RocksDB: {}", e))
                })?
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!(
                        "Row not found: {} in table {}.{}",
                        row_id,
                        namespace_id.as_str(),
                        table_name.as_str()
                    ))
                })?;

            let mut existing_row: JsonValue =
                serde_json::from_slice(&existing_bytes).map_err(|e| {
                    KalamDbError::SerializationError(format!(
                        "Failed to deserialize existing row: {}",
                        e
                    ))
                })?;

            // Set _deleted = true and _updated = NOW()
            if let Some(obj) = existing_row.as_object_mut() {
                obj.insert("_deleted".to_string(), JsonValue::Bool(true));
                obj.insert("_updated".to_string(), JsonValue::Number(now_ms.into()));
            } else {
                return Err(KalamDbError::SerializationError(
                    "Existing row is not a JSON object".to_string(),
                ));
            }

            // Serialize and add to batch
            let updated_bytes = serde_json::to_vec(&existing_row).map_err(|e| {
                KalamDbError::SerializationError(format!("Failed to serialize updated row: {}", e))
            })?;

            batch.put_cf(cf, key.as_bytes(), &updated_bytes);

            deleted_row_ids.push(row_id);
        }

        // Write batch atomically
        self.db.write(batch).map_err(|e| {
            KalamDbError::Other(format!("Failed to write batch to RocksDB: {}", e))
        })?;

        log::debug!(
            "Soft deleted {} rows in {}.{} for user {}",
            deleted_row_ids.len(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        Ok(deleted_row_ids)
    }

    /// Hard delete a row (physically remove from RocksDB)
    ///
    /// **WARNING**: This permanently removes data and should only be used
    /// for cleanup operations after the retention period has expired.
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `row_id` - Row ID to hard delete
    ///
    /// # Returns
    /// The row ID of the hard deleted row
    pub fn hard_delete_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
    ) -> Result<String, KalamDbError> {
        // Build RocksDB key
        let key = format!("{}:{}", user_id.as_str(), row_id);

        // Get column family handle
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(namespace_id),
            table_name,
        );
        let cf = self
            .db
            .cf_handle(&cf_name)
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Column family not found for table: {}.{}",
                    namespace_id.as_str(),
                    table_name.as_str()
                ))
            })?;

        // Delete from RocksDB
        self.db
            .delete_cf(cf, key.as_bytes())
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to hard delete row from RocksDB: {}", e))
            })?;

        log::debug!(
            "Hard deleted row {} in {}.{} for user {}",
            row_id,
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        Ok(row_id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocksdb::Options;
    use tempfile::TempDir;

    fn setup_test_db() -> (Arc<DB>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Create DB with column families
        let cf_names = vec!["user_table:test_ns:test_table"];
        let db = DB::open_cf(&opts, temp_dir.path(), &cf_names).unwrap();

        (Arc::new(db), temp_dir)
    }

    fn insert_test_row(
        db: &Arc<DB>,
        user_id: &str,
        row_id: &str,
        data: JsonValue,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cf = db.cf_handle("user_table:test_ns:test_table").unwrap();
        let key = format!("{}:{}", user_id, row_id);

        // Add system columns
        let mut row_data = data;
        if let Some(obj) = row_data.as_object_mut() {
            obj.insert("_updated".to_string(), JsonValue::Number(1000.into()));
            obj.insert("_deleted".to_string(), JsonValue::Bool(false));
        }

        let value_bytes = serde_json::to_vec(&row_data)?;
        db.put_cf(cf, key.as_bytes(), &value_bytes)?;

        Ok(())
    }

    #[test]
    fn test_delete_row() {
        let (db, _temp_dir) = setup_test_db();

        // Insert initial row
        let user_id = "user123";
        let row_id = "row1";
        insert_test_row(
            &db,
            user_id,
            row_id,
            serde_json::json!({"name": "Alice", "age": 30}),
        )
        .unwrap();

        // Soft delete the row
        let handler = UserTableDeleteHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id_type = UserId::new(user_id.to_string());

        let deleted_row_id = handler
            .delete_row(&namespace_id, &table_name, &user_id_type, row_id)
            .unwrap();

        assert_eq!(deleted_row_id, row_id);

        // Verify the row is marked as deleted
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();
        let key = format!("{}:{}", user_id, row_id);
        let value = db.get_cf(cf, key.as_bytes()).unwrap().unwrap();

        let stored: JsonValue = serde_json::from_slice(&value).unwrap();
        assert_eq!(stored["name"], "Alice"); // Data still present
        assert_eq!(stored["age"], 30); // Data still present
        assert_eq!(stored["_deleted"], true); // Marked as deleted
        assert!(stored["_updated"].as_i64().unwrap() > 1000); // Updated timestamp
    }

    #[test]
    fn test_delete_nonexistent_row() {
        let (db, _temp_dir) = setup_test_db();
        let handler = UserTableDeleteHandler::new(db);

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let result = handler.delete_row(&namespace_id, &table_name, &user_id, "nonexistent");

        assert!(result.is_err());
        match result {
            Err(KalamDbError::NotFound(msg)) => {
                assert!(msg.contains("Row not found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_delete_batch() {
        let (db, _temp_dir) = setup_test_db();

        // Insert initial rows
        let user_id = "user123";
        insert_test_row(
            &db,
            user_id,
            "row1",
            serde_json::json!({"name": "Alice"}),
        )
        .unwrap();
        insert_test_row(&db, user_id, "row2", serde_json::json!({"name": "Bob"}))
            .unwrap();
        insert_test_row(
            &db,
            user_id,
            "row3",
            serde_json::json!({"name": "Charlie"}),
        )
        .unwrap();

        // Delete batch
        let handler = UserTableDeleteHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id_type = UserId::new(user_id.to_string());

        let row_ids = vec!["row1".to_string(), "row2".to_string(), "row3".to_string()];

        let deleted_ids = handler
            .delete_batch(&namespace_id, &table_name, &user_id_type, row_ids)
            .unwrap();

        assert_eq!(deleted_ids.len(), 3);

        // Verify all rows are marked as deleted
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();

        for row_id in &["row1", "row2", "row3"] {
            let key = format!("{}:{}", user_id, row_id);
            let value = db.get_cf(cf, key.as_bytes()).unwrap().unwrap();
            let stored: JsonValue = serde_json::from_slice(&value).unwrap();
            assert_eq!(stored["_deleted"], true);
        }
    }

    #[test]
    fn test_delete_data_isolation() {
        let (db, _temp_dir) = setup_test_db();

        // Insert rows for different users
        insert_test_row(
            &db,
            "user1",
            "row1",
            serde_json::json!({"name": "Alice"}),
        )
        .unwrap();
        insert_test_row(
            &db,
            "user2",
            "row1",
            serde_json::json!({"name": "Bob"}),
        )
        .unwrap();

        // Delete user1's row
        let handler = UserTableDeleteHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user1 = UserId::new("user1".to_string());

        handler
            .delete_row(&namespace_id, &table_name, &user1, "row1")
            .unwrap();

        // Verify user1's row is deleted
        let cf = db.cf_handle("user_table:test_ns:test_table").unwrap();
        let key1 = "user1:row1";
        let value1 = db.get_cf(cf, key1.as_bytes()).unwrap().unwrap();
        let stored1: JsonValue = serde_json::from_slice(&value1).unwrap();
        assert_eq!(stored1["_deleted"], true);

        // Verify user2's row is NOT deleted
        let key2 = "user2:row1";
        let value2 = db.get_cf(cf, key2.as_bytes()).unwrap().unwrap();
        let stored2: JsonValue = serde_json::from_slice(&value2).unwrap();
        assert_eq!(stored2["_deleted"], false);
    }

    #[test]
    fn test_hard_delete_row() {
        let (db, _temp_dir) = setup_test_db();

        // Insert initial row
        let user_id = "user123";
        let row_id = "row1";
        insert_test_row(
            &db,
            user_id,
            row_id,
            serde_json::json!({"name": "Alice"}),
        )
        .unwrap();

        // Hard delete the row
        let handler = UserTableDeleteHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id_type = UserId::new(user_id.to_string());

        handler
            .hard_delete_row(&namespace_id, &table_name, &user_id_type, row_id)
            .unwrap();

        // Verify the row is completely removed
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();
        let key = format!("{}:{}", user_id, row_id);
        let value = db.get_cf(cf, key.as_bytes()).unwrap();

        assert!(value.is_none(), "Row should be completely removed");
    }

    #[test]
    fn test_multiple_deletes_idempotent() {
        let (db, _temp_dir) = setup_test_db();

        // Insert initial row
        let user_id = "user123";
        let row_id = "row1";
        insert_test_row(
            &db,
            user_id,
            row_id,
            serde_json::json!({"name": "Alice"}),
        )
        .unwrap();

        let handler = UserTableDeleteHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id_type = UserId::new(user_id.to_string());

        // Delete twice
        handler
            .delete_row(&namespace_id, &table_name, &user_id_type, row_id)
            .unwrap();

        let result2 = handler.delete_row(&namespace_id, &table_name, &user_id_type, row_id);

        // Second delete should succeed (idempotent)
        assert!(result2.is_ok());

        // Verify row is still marked as deleted
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();
        let key = format!("{}:{}", user_id, row_id);
        let value = db.get_cf(cf, key.as_bytes()).unwrap().unwrap();
        let stored: JsonValue = serde_json::from_slice(&value).unwrap();
        assert_eq!(stored["_deleted"], true);
    }
}
