//! User table INSERT operations
//!
//! This module handles INSERT operations for user tables with:
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic system column injection (_updated = NOW(), _deleted = false)
//! - kalamdb-store for RocksDB operations
//! - Data isolation enforcement

use crate::catalog::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use kalamdb_store::UserTableStore;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// User table INSERT handler
///
/// Coordinates INSERT operations for user tables, enforcing:
/// - Data isolation via UserId key prefix
/// - System column injection (_updated, _deleted) - handled by UserTableStore
/// - kalamdb-store for RocksDB writes
pub struct UserTableInsertHandler {
    store: Arc<UserTableStore>,
}

impl UserTableInsertHandler {
    /// Create a new user table INSERT handler
    ///
    /// # Arguments
    /// * `store` - UserTableStore instance for RocksDB operations
    pub fn new(store: Arc<UserTableStore>) -> Self {
        Self { store }
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
    /// RocksDB key format: `{UserId}:{row_id}` (handled by UserTableStore)
    ///
    /// # System Columns
    /// System columns (_updated, _deleted) are automatically injected by UserTableStore
    pub fn insert_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_data: JsonValue,
    ) -> Result<String, KalamDbError> {
        // Validate row data is an object
        if !row_data.is_object() {
            return Err(KalamDbError::InvalidOperation(
                "Row data must be a JSON object".to_string(),
            ));
        }

        // Generate row ID (using timestamp + random component for uniqueness)
        let row_id = self.generate_row_id()?;

        // Delegate to UserTableStore (handles system column injection)
        self.store
            .put(
                namespace_id.as_str(),
                table_name.as_str(),
                user_id.as_str(),
                &row_id,
                row_data,
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to insert row: {}", e)))?;

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

        // Insert each row individually (UserTableStore doesn't have batch insert yet)
        for row_data in rows {
            // Validate row data is an object
            if !row_data.is_object() {
                return Err(KalamDbError::InvalidOperation(
                    "All rows must be JSON objects".to_string(),
                ));
            }

            // Generate row ID
            let row_id = self.generate_row_id()?;

            // Delegate to UserTableStore
            self.store
                .put(
                    namespace_id.as_str(),
                    table_name.as_str(),
                    user_id.as_str(),
                    &row_id,
                    row_data,
                )
                .map_err(|e| {
                    KalamDbError::Other(format!("Failed to insert row in batch: {}", e))
                })?;

            row_ids.push(row_id);
        }

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
        use std::time::{SystemTime, UNIX_EPOCH};

        // Thread-safe counter for uniqueness within same millisecond
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| KalamDbError::Other(format!("System time error: {}", e)))?
            .as_millis() as u64;
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);

        // Combine timestamp + counter for uniqueness
        // Format: timestamp_counter (e.g., 1703001234567_42)
        Ok(format!("{}_{}", now_ms, counter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::UserTableStore;

    fn setup_test_handler() -> UserTableInsertHandler {
        let test_db = TestDb::single_cf("user_table:test_ns:test_table").unwrap();
        let store = Arc::new(UserTableStore::new(test_db.db).unwrap());
        UserTableInsertHandler::new(store)
    }

    #[test]
    fn test_insert_row() {
        let handler = setup_test_handler();

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
    }

    #[test]
    fn test_insert_batch() {
        let handler = setup_test_handler();

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
        for row_id in row_ids {
            assert!(row_id.contains('_'));
        }
    }

    #[test]
    fn test_data_isolation() {
        let handler = setup_test_handler();

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

        // Verify keys have different row IDs (user isolation handled by UserTableStore)
        assert!(row_id1 != row_id2);
    }

    #[test]
    fn test_generate_row_id_uniqueness() {
        let handler = setup_test_handler();

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
        let handler = setup_test_handler();

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
