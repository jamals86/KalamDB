//! User table UPDATE operations
//!
//! This module handles UPDATE operations for user tables with:
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic _updated timestamp update
//! - Data isolation enforcement
//! - Atomic update operations

use crate::catalog::{NamespaceId, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::storage::column_family_manager::ColumnFamilyManager;
use chrono::Utc;
use rocksdb::DB;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// User table UPDATE handler
///
/// Coordinates UPDATE operations for user tables, enforcing:
/// - Data isolation via UserId key prefix
/// - Automatic _updated timestamp refresh
/// - RocksDB writes to appropriate column family
pub struct UserTableUpdateHandler {
    db: Arc<DB>,
}

impl UserTableUpdateHandler {
    /// Create a new user table UPDATE handler
    ///
    /// # Arguments
    /// * `db` - RocksDB instance
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// Update a single row in a user table
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `row_id` - Row ID to update
    /// * `updates` - Fields to update as JSON object (partial updates supported)
    ///
    /// # Returns
    /// The row ID of the updated row
    ///
    /// # Key Format
    /// RocksDB key format: `{UserId}:{row_id}`
    ///
    /// # System Columns
    /// Automatically updates:
    /// - `_updated`: TIMESTAMP = NOW()
    ///
    /// Note: `_deleted` is NOT modified during UPDATE operations
    pub fn update_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
        updates: JsonValue,
    ) -> Result<String, KalamDbError> {
        // Validate updates is an object
        if !updates.is_object() {
            return Err(KalamDbError::InvalidOperation(
                "Updates must be a JSON object".to_string(),
            ));
        }

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

        // Merge updates into existing row
        if let Some(existing_obj) = existing_row.as_object_mut() {
            if let Some(updates_obj) = updates.as_object() {
                for (key, value) in updates_obj {
                    // Prevent updates to system columns
                    if key == "_updated" || key == "_deleted" {
                        log::warn!(
                            "Attempted to update system column '{}', ignored",
                            key
                        );
                        continue;
                    }
                    existing_obj.insert(key.clone(), value.clone());
                }
            }
        }

        // Update _updated timestamp
        let now_ms = Utc::now().timestamp_millis();
        if let Some(obj) = existing_row.as_object_mut() {
            obj.insert("_updated".to_string(), JsonValue::Number(now_ms.into()));
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
            "Updated row {} in {}.{} for user {}",
            row_id,
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        Ok(row_id.to_string())
    }

    /// Update multiple rows in batch
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `row_updates` - Vector of (row_id, updates) pairs
    ///
    /// # Returns
    /// Vector of updated row IDs
    ///
    /// # Note
    /// This is atomic - either all updates succeed or none do
    pub fn update_batch(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_updates: Vec<(String, JsonValue)>,
    ) -> Result<Vec<String>, KalamDbError> {
        let mut updated_row_ids = Vec::with_capacity(row_updates.len());

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

        for (row_id, updates) in row_updates {
            // Validate updates is an object
            if !updates.is_object() {
                return Err(KalamDbError::InvalidOperation(
                    "All updates must be JSON objects".to_string(),
                ));
            }

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

            // Merge updates
            if let Some(existing_obj) = existing_row.as_object_mut() {
                if let Some(updates_obj) = updates.as_object() {
                    for (key, value) in updates_obj {
                        if key == "_updated" || key == "_deleted" {
                            continue; // Skip system columns
                        }
                        existing_obj.insert(key.clone(), value.clone());
                    }
                }

                // Update _updated timestamp
                existing_obj.insert("_updated".to_string(), JsonValue::Number(now_ms.into()));
            }

            // Serialize and add to batch
            let updated_bytes = serde_json::to_vec(&existing_row).map_err(|e| {
                KalamDbError::SerializationError(format!("Failed to serialize updated row: {}", e))
            })?;

            batch.put_cf(cf, key.as_bytes(), &updated_bytes);

            updated_row_ids.push(row_id);
        }

        // Write batch atomically
        self.db.write(batch).map_err(|e| {
            KalamDbError::Other(format!("Failed to write batch to RocksDB: {}", e))
        })?;

        log::debug!(
            "Updated {} rows in {}.{} for user {}",
            updated_row_ids.len(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        Ok(updated_row_ids)
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
    fn test_update_row() {
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

        // Update the row
        let handler = UserTableUpdateHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id_type = UserId::new(user_id.to_string());

        let updates = serde_json::json!({"age": 31, "city": "NYC"});

        let updated_row_id = handler
            .update_row(&namespace_id, &table_name, &user_id_type, row_id, updates)
            .unwrap();

        assert_eq!(updated_row_id, row_id);

        // Verify the update
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();
        let key = format!("{}:{}", user_id, row_id);
        let value = db.get_cf(cf, key.as_bytes()).unwrap().unwrap();

        let stored: JsonValue = serde_json::from_slice(&value).unwrap();
        assert_eq!(stored["name"], "Alice"); // Unchanged
        assert_eq!(stored["age"], 31); // Updated
        assert_eq!(stored["city"], "NYC"); // New field
        assert_eq!(stored["_deleted"], false); // Unchanged
        assert!(stored["_updated"].as_i64().unwrap() > 1000); // Updated timestamp
    }

    #[test]
    fn test_update_nonexistent_row() {
        let (db, _temp_dir) = setup_test_db();
        let handler = UserTableUpdateHandler::new(db);

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let updates = serde_json::json!({"age": 31});

        let result = handler.update_row(&namespace_id, &table_name, &user_id, "nonexistent", updates);

        assert!(result.is_err());
        match result {
            Err(KalamDbError::NotFound(msg)) => {
                assert!(msg.contains("Row not found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_update_batch() {
        let (db, _temp_dir) = setup_test_db();

        // Insert initial rows
        let user_id = "user123";
        insert_test_row(
            &db,
            user_id,
            "row1",
            serde_json::json!({"name": "Alice", "age": 30}),
        )
        .unwrap();
        insert_test_row(
            &db,
            user_id,
            "row2",
            serde_json::json!({"name": "Bob", "age": 25}),
        )
        .unwrap();

        // Update batch
        let handler = UserTableUpdateHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id_type = UserId::new(user_id.to_string());

        let row_updates = vec![
            ("row1".to_string(), serde_json::json!({"age": 31})),
            ("row2".to_string(), serde_json::json!({"age": 26})),
        ];

        let updated_ids = handler
            .update_batch(&namespace_id, &table_name, &user_id_type, row_updates)
            .unwrap();

        assert_eq!(updated_ids.len(), 2);

        // Verify updates
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();

        let key1 = format!("{}:row1", user_id);
        let value1 = db.get_cf(cf, key1.as_bytes()).unwrap().unwrap();
        let stored1: JsonValue = serde_json::from_slice(&value1).unwrap();
        assert_eq!(stored1["age"], 31);

        let key2 = format!("{}:row2", user_id);
        let value2 = db.get_cf(cf, key2.as_bytes()).unwrap().unwrap();
        let stored2: JsonValue = serde_json::from_slice(&value2).unwrap();
        assert_eq!(stored2["age"], 26);
    }

    #[test]
    fn test_update_prevents_system_column_modification() {
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

        // Try to update system columns
        let handler = UserTableUpdateHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id_type = UserId::new(user_id.to_string());

        let updates = serde_json::json!({
            "name": "Bob",
            "_updated": 9999, // Should be ignored
            "_deleted": true  // Should be ignored
        });

        handler
            .update_row(&namespace_id, &table_name, &user_id_type, row_id, updates)
            .unwrap();

        // Verify system columns were NOT modified by updates
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();
        let key = format!("{}:{}", user_id, row_id);
        let value = db.get_cf(cf, key.as_bytes()).unwrap().unwrap();

        let stored: JsonValue = serde_json::from_slice(&value).unwrap();
        assert_eq!(stored["name"], "Bob"); // User field updated
        assert_eq!(stored["_deleted"], false); // System column unchanged
        assert!(stored["_updated"].as_i64().unwrap() > 1000); // Timestamp auto-updated (not 9999)
    }

    #[test]
    fn test_update_data_isolation() {
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

        // Update user1's row
        let handler = UserTableUpdateHandler::new(db.clone());
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user1 = UserId::new("user1".to_string());

        handler
            .update_row(
                &namespace_id,
                &table_name,
                &user1,
                "row1",
                serde_json::json!({"name": "Alice Updated"}),
            )
            .unwrap();

        // Verify user2's row is unchanged
        let cf = db.cf_handle("user_table:test_ns:test_table").unwrap();
        let key2 = "user2:row1";
        let value2 = db.get_cf(cf, key2.as_bytes()).unwrap().unwrap();
        let stored2: JsonValue = serde_json::from_slice(&value2).unwrap();
        assert_eq!(stored2["name"], "Bob"); // Unchanged
    }

    #[test]
    fn test_update_non_object_fails() {
        let (db, _temp_dir) = setup_test_db();

        // Insert initial row
        insert_test_row(
            &db,
            "user123",
            "row1",
            serde_json::json!({"name": "Alice"}),
        )
        .unwrap();

        let handler = UserTableUpdateHandler::new(db);
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        // Try to update with non-object
        let updates = serde_json::json!(["not", "an", "object"]);

        let result = handler.update_row(&namespace_id, &table_name, &user_id, "row1", updates);

        assert!(result.is_err());
        match result {
            Err(KalamDbError::InvalidOperation(msg)) => {
                assert!(msg.contains("JSON object"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }
}
