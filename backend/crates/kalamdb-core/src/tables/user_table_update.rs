//! User table UPDATE operations
//!
//! This module handles UPDATE operations for user tables with:
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic _updated timestamp update (handled by UserTableStore)
//! - Data isolation enforcement
//! - Atomic update operations

use crate::catalog::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use kalamdb_store::UserTableStore;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// User table UPDATE handler
///
/// Coordinates UPDATE operations for user tables, enforcing:
/// - Data isolation via UserId key prefix
/// - Automatic _updated timestamp refresh (handled by UserTableStore)
/// - kalamdb-store for RocksDB operations
pub struct UserTableUpdateHandler {
    store: Arc<UserTableStore>,
}

impl UserTableUpdateHandler {
    /// Create a new user table UPDATE handler
    ///
    /// # Arguments
    /// * `store` - UserTableStore instance for RocksDB operations
    pub fn new(store: Arc<UserTableStore>) -> Self {
        Self { store }
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
    /// # System Columns
    /// The `_updated` timestamp is automatically refreshed by UserTableStore.
    /// The `_deleted` flag is NOT modified during UPDATE operations.
    /// System column updates in `updates` parameter are ignored.
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

        // Read existing row from store
        let mut existing_row = self
            .store
            .get(
                namespace_id.as_str(),
                table_name.as_str(),
                user_id.as_str(),
                row_id,
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to read row: {}", e)))?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Row not found: {} in table {}.{}",
                    row_id,
                    namespace_id.as_str(),
                    table_name.as_str()
                ))
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

        // Write updated row back (store.put automatically updates _updated timestamp)
        self.store
            .put(
                namespace_id.as_str(),
                table_name.as_str(),
                user_id.as_str(),
                row_id,
                existing_row,
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to write updated row: {}", e)))?;

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
    pub fn update_batch(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_updates: Vec<(String, JsonValue)>,
    ) -> Result<Vec<String>, KalamDbError> {
        let mut updated_row_ids = Vec::with_capacity(row_updates.len());

        // Update each row individually (UserTableStore doesn't have batch update yet)
        for (row_id, updates) in row_updates {
            // Call update_row for each row
            let updated_id = self.update_row(
                namespace_id,
                table_name,
                user_id,
                &row_id,
                updates,
            )?;
            updated_row_ids.push(updated_id);
        }

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
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::UserTableStore;

    fn setup_test_handler() -> (UserTableUpdateHandler, Arc<UserTableStore>) {
        let test_db = TestDb::single_cf("user_table:test_ns:test_table").unwrap();
        let store = Arc::new(UserTableStore::new(test_db.db).unwrap());
        let handler = UserTableUpdateHandler::new(store.clone());
        (handler, store)
    }

    #[test]
    fn test_update_row() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());
        let row_id = "row1";

        // Insert initial row
        let initial_data = serde_json::json!({"name": "Alice", "age": 30});
        store.put(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), row_id, initial_data).unwrap();

        // Update the row
        let updates = serde_json::json!({"age": 31, "city": "NYC"});
        let updated_row_id = handler
            .update_row(&namespace_id, &table_name, &user_id, row_id, updates)
            .unwrap();

        assert_eq!(updated_row_id, row_id);

        // Verify the update
        let stored = store.get(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), row_id)
            .unwrap().expect("Row should exist");
        assert_eq!(stored["name"], "Alice"); // Unchanged
        assert_eq!(stored["age"], 31); // Updated
        assert_eq!(stored["city"], "NYC"); // New field
        assert_eq!(stored["_deleted"], false); // Unchanged
        assert!(stored["_updated"].as_str().is_some()); // Updated timestamp
    }

    #[test]
    fn test_update_nonexistent_row() {
        let (handler, _store) = setup_test_handler();
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
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        // Insert initial rows
        store.put(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row1", 
                  serde_json::json!({"name": "Alice", "age": 30})).unwrap();
        store.put(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row2",
                  serde_json::json!({"name": "Bob", "age": 25})).unwrap();

        // Update batch
        let row_updates = vec![
            ("row1".to_string(), serde_json::json!({"age": 31})),
            ("row2".to_string(), serde_json::json!({"age": 26})),
        ];

        let updated_ids = handler
            .update_batch(&namespace_id, &table_name, &user_id, row_updates)
            .unwrap();

        assert_eq!(updated_ids.len(), 2);

        // Verify updates
        let stored1 = store.get(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row1")
            .unwrap().expect("Row1 should exist");
        assert_eq!(stored1["age"], 31);

        let stored2 = store.get(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row2")
            .unwrap().expect("Row2 should exist");
        assert_eq!(stored2["age"], 26);
    }

    #[test]
    fn test_update_prevents_system_column_modification() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());
        let row_id = "row1";

        // Insert initial row
        store.put(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), row_id,
                  serde_json::json!({"name": "Alice"})).unwrap();

        // Try to update system columns
        let updates = serde_json::json!({
            "name": "Bob",
            "_updated": 9999, // Should be ignored
            "_deleted": true  // Should be ignored
        });

        handler
            .update_row(&namespace_id, &table_name, &user_id, row_id, updates)
            .unwrap();

        // Verify system columns were NOT modified by updates
        let stored = store.get(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), row_id)
            .unwrap().expect("Row should exist");
        assert_eq!(stored["name"], "Bob"); // User field updated
        assert_eq!(stored["_deleted"], false); // System column unchanged
        assert!(stored["_updated"].as_str().is_some()); // Timestamp auto-updated (not 9999)
    }

    #[test]
    fn test_update_data_isolation() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user1 = UserId::new("user1".to_string());
        let user2 = UserId::new("user2".to_string());

        // Insert rows for different users
        store.put(namespace_id.as_str(), table_name.as_str(), user1.as_str(), "row1",
                  serde_json::json!({"name": "Alice"})).unwrap();
        store.put(namespace_id.as_str(), table_name.as_str(), user2.as_str(), "row1",
                  serde_json::json!({"name": "Bob"})).unwrap();

        // Update user1's row
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
        let stored2 = store.get(namespace_id.as_str(), table_name.as_str(), user2.as_str(), "row1")
            .unwrap().expect("User2's row should exist");
        assert_eq!(stored2["name"], "Bob"); // Unchanged
    }

    #[test]
    fn test_update_non_object_fails() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        // Insert initial row
        store.put(namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row1",
                  serde_json::json!({"name": "Alice"})).unwrap();

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
