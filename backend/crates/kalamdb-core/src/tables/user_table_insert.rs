//! User table INSERT operations
//!
//! This module handles INSERT operations for user tables with:
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic system column injection (_updated = NOW(), _deleted = false)
//! - RocksDB column family writes
//! - Data isolation enforcement

use crate::catalog::{NamespaceId, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::storage::column_family_manager::ColumnFamilyManager;
use chrono::Utc;
use rocksdb::DB;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// User table INSERT handler
///
/// Coordinates INSERT operations for user tables, enforcing:
/// - Data isolation via UserId key prefix
/// - System column injection (_updated, _deleted)
/// - RocksDB writes to appropriate column family
pub struct UserTableInsertHandler {
    db: Arc<DB>,
}

impl UserTableInsertHandler {
    /// Create a new user table INSERT handler
    ///
    /// # Arguments
    /// * `db` - RocksDB instance
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// Insert a single row into a user table
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `row_data` - Row data as JSON object
    ///
    /// # Returns
    /// The generated row ID
    ///
    /// # Key Format
    /// RocksDB key format: `{UserId}:{row_id}`
    ///
    /// # System Columns
    /// Automatically injects:
    /// - `_updated`: TIMESTAMP = NOW()
    /// - `_deleted`: BOOLEAN = false
    pub fn insert_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        mut row_data: JsonValue,
    ) -> Result<String, KalamDbError> {
        // Inject system columns
        let now_ms = Utc::now().timestamp_millis();
        if let Some(obj) = row_data.as_object_mut() {
            obj.insert("_updated".to_string(), JsonValue::Number(now_ms.into()));
            obj.insert("_deleted".to_string(), JsonValue::Bool(false));
        } else {
            return Err(KalamDbError::InvalidOperation(
                "Row data must be a JSON object".to_string(),
            ));
        }

        // Generate row ID (using timestamp + random component for uniqueness)
        let row_id = self.generate_row_id()?;

        // Build RocksDB key: {UserId}:{row_id}
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

        // Serialize row data to JSON bytes
        let value_bytes = serde_json::to_vec(&row_data).map_err(|e| {
            KalamDbError::SerializationError(format!("Failed to serialize row data: {}", e))
        })?;

        // Write to RocksDB
        self.db
            .put_cf(cf, key.as_bytes(), &value_bytes)
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to write row to RocksDB: {}", e))
            })?;

        log::debug!(
            "Inserted row into {}.{} for user {} with row_id {}",
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id
        );

        Ok(row_id)
    }

    /// Insert multiple rows in batch
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `rows` - Vector of row data as JSON objects
    ///
    /// # Returns
    /// Vector of generated row IDs
    pub fn insert_batch(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        rows: Vec<JsonValue>,
    ) -> Result<Vec<String>, KalamDbError> {
        let mut row_ids = Vec::with_capacity(rows.len());

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

        for mut row_data in rows {
            // Inject system columns
            if let Some(obj) = row_data.as_object_mut() {
                obj.insert("_updated".to_string(), JsonValue::Number(now_ms.into()));
                obj.insert("_deleted".to_string(), JsonValue::Bool(false));
            } else {
                return Err(KalamDbError::InvalidOperation(
                    "All rows must be JSON objects".to_string(),
                ));
            }

            // Generate row ID
            let row_id = self.generate_row_id()?;

            // Build RocksDB key
            let key = format!("{}:{}", user_id.as_str(), row_id);

            // Serialize row data
            let value_bytes = serde_json::to_vec(&row_data).map_err(|e| {
                KalamDbError::SerializationError(format!("Failed to serialize row data: {}", e))
            })?;

            // Add to batch
            batch.put_cf(cf, key.as_bytes(), &value_bytes);

            row_ids.push(row_id);
        }

        // Write batch atomically
        self.db.write(batch).map_err(|e| {
            KalamDbError::Other(format!("Failed to write batch to RocksDB: {}", e))
        })?;

        log::debug!(
            "Inserted {} rows into {}.{} for user {}",
            row_ids.len(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        Ok(row_ids)
    }

    /// Generate a unique row ID
    ///
    /// Format: {timestamp_ms}_{random_component}
    ///
    /// This provides time-ordered keys and uniqueness even for concurrent inserts
    fn generate_row_id(&self) -> Result<String, KalamDbError> {
        use std::sync::atomic::{AtomicU64, Ordering};

        // Thread-safe counter for uniqueness within same millisecond
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let now_ms = Utc::now().timestamp_millis() as u64;
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);

        // Combine timestamp + counter for uniqueness
        // Format: timestamp_counter (e.g., 1703001234567_42)
        Ok(format!("{}_{}", now_ms, counter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::column_family_manager::ColumnFamilyManager;
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

    #[test]
    fn test_insert_row() {
        let (db, _temp_dir) = setup_test_db();
        let handler = UserTableInsertHandler::new(db.clone());

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let row_data = serde_json::json!({
            "name": "Alice",
            "age": 30
        });

        let row_id = handler
            .insert_row(&namespace_id, &table_name, &user_id, row_data)
            .unwrap();

        // Verify row_id format
        assert!(row_id.contains('_'));

        // Verify data was written
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();

        let key = format!("{}:{}", user_id.as_str(), row_id);
        let value = db.get_cf(cf, key.as_bytes()).unwrap().unwrap();

        let stored: JsonValue = serde_json::from_slice(&value).unwrap();
        assert_eq!(stored["name"], "Alice");
        assert_eq!(stored["age"], 30);
        assert_eq!(stored["_deleted"], false);
        assert!(stored["_updated"].is_number());
    }

    #[test]
    fn test_insert_batch() {
        let (db, _temp_dir) = setup_test_db();
        let handler = UserTableInsertHandler::new(db.clone());

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let rows = vec![
            serde_json::json!({"name": "Alice", "age": 30}),
            serde_json::json!({"name": "Bob", "age": 25}),
            serde_json::json!({"name": "Charlie", "age": 35}),
        ];

        let row_ids = handler
            .insert_batch(&namespace_id, &table_name, &user_id, rows)
            .unwrap();

        assert_eq!(row_ids.len(), 3);

        // Verify all rows were written
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();

        for (idx, row_id) in row_ids.iter().enumerate() {
            let key = format!("{}:{}", user_id.as_str(), row_id);
            let value = db.get_cf(cf, key.as_bytes()).unwrap().unwrap();

            let stored: JsonValue = serde_json::from_slice(&value).unwrap();
            assert!(stored["name"].is_string());
            assert!(stored["age"].is_number());
            assert_eq!(stored["_deleted"], false);
        }
    }

    #[test]
    fn test_data_isolation() {
        let (db, _temp_dir) = setup_test_db();
        let handler = UserTableInsertHandler::new(db.clone());

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user1 = UserId::new("user1".to_string());
        let user2 = UserId::new("user2".to_string());

        let row_data = serde_json::json!({"name": "Alice"});

        // Insert for user1
        let row_id1 = handler
            .insert_row(&namespace_id, &table_name, &user1, row_data.clone())
            .unwrap();

        // Insert for user2
        let row_id2 = handler
            .insert_row(&namespace_id, &table_name, &user2, row_data.clone())
            .unwrap();

        // Verify keys have different user_id prefixes
        assert!(row_id1 != row_id2); // Different row IDs

        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();

        // Verify user1's data
        let key1 = format!("{}:{}", user1.as_str(), row_id1);
        let value1 = db.get_cf(cf, key1.as_bytes()).unwrap();
        assert!(value1.is_some());

        // Verify user2's data
        let key2 = format!("{}:{}", user2.as_str(), row_id2);
        let value2 = db.get_cf(cf, key2.as_bytes()).unwrap();
        assert!(value2.is_some());
    }

    #[test]
    fn test_system_column_injection() {
        let (db, _temp_dir) = setup_test_db();
        let handler = UserTableInsertHandler::new(db.clone());

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let row_data = serde_json::json!({"name": "Alice"});

        let row_id = handler
            .insert_row(&namespace_id, &table_name, &user_id, row_data)
            .unwrap();

        // Retrieve and verify system columns
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&namespace_id),
            &table_name,
        );
        let cf = db.cf_handle(&cf_name).unwrap();

        let key = format!("{}:{}", user_id.as_str(), row_id);
        let value = db.get_cf(cf, key.as_bytes()).unwrap().unwrap();

        let stored: JsonValue = serde_json::from_slice(&value).unwrap();

        // Verify _updated is a timestamp
        assert!(stored["_updated"].is_number());
        let updated_ms = stored["_updated"].as_i64().unwrap();
        assert!(updated_ms > 0);

        // Verify _deleted is false
        assert_eq!(stored["_deleted"], false);
    }

    #[test]
    fn test_generate_row_id_uniqueness() {
        let (db, _temp_dir) = setup_test_db();
        let handler = UserTableInsertHandler::new(db);

        let mut row_ids = std::collections::HashSet::new();

        // Generate 1000 row IDs and verify uniqueness
        for _ in 0..1000 {
            let row_id = handler.generate_row_id().unwrap();
            assert!(
                row_ids.insert(row_id.clone()),
                "Duplicate row ID generated: {}",
                row_id
            );
        }
    }

    #[test]
    fn test_insert_non_object_fails() {
        let (db, _temp_dir) = setup_test_db();
        let handler = UserTableInsertHandler::new(db);

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        // Try to insert a non-object (array)
        let row_data = serde_json::json!(["not", "an", "object"]);

        let result = handler.insert_row(&namespace_id, &table_name, &user_id, row_data);

        assert!(result.is_err());
        match result {
            Err(KalamDbError::InvalidOperation(msg)) => {
                assert!(msg.contains("JSON object"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }
}
