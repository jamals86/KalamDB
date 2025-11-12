//! User table UPDATE operations
//!
//! This module handles UPDATE operations for user tables with:
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic _updated timestamp update (handled by UserTableStore)
//! - Data isolation enforcement
//! - Atomic update operations

use crate::schema_registry::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::tables::system::system_table_store::UserTableStoreExt;
use crate::tables::UserTableStore;
use kalamdb_commons::ids::{SeqId, UserTableRowId};
use kalamdb_commons::models::TableId;
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
    live_query_manager: Option<Arc<LiveQueryManager>>,
}

impl UserTableUpdateHandler {
    /// Create a new user table UPDATE handler
    ///
    /// # Arguments
    /// * `store` - UserTableStore instance for RocksDB operations
    pub fn new(store: Arc<UserTableStore>) -> Self {
        Self {
            store,
            live_query_manager: None,
        }
    }

    /// Configure LiveQueryManager for WebSocket notifications
    ///
    /// # Arguments
    /// * `manager` - LiveQueryManager instance for notifications
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
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

        // Parse row_id to SeqId and construct key
        let seq_id = SeqId::from_string(row_id)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid row_id: {}", e)))?;
        let key = UserTableRowId::new(user_id.clone(), seq_id);

        // Read existing row from store
        let existing_row = UserTableStoreExt::get(
            self.store.as_ref(),
            &key,
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

        // Clone for old_data before merging
        let old_data = existing_row.fields.clone();
        let mut updated_row = existing_row.clone();

        // Merge updates into existing row
        if let Some(updated_obj) = updates.as_object() {
            if let Some(fields_obj) = updated_row.fields.as_object_mut() {
                for (key, value) in updated_obj {
                    // Prevent updates to system columns
                    if key == "_updated" || key == "_deleted" {
                        log::warn!("Attempted to update system column '{}', ignored", key);
                        continue;
                    }
                    fields_obj.insert(key.clone(), value.clone());
                }
            }
        }

        // T044-T046: Use SystemColumnsService for MVCC append-only UPDATE
        // In MVCC, UPDATE creates a new version with a new SeqId
        
        // Get AppContext and SystemColumnsService (T024, T042)
        use crate::app_context::AppContext;
        let app_context = AppContext::get();
        let sys_cols = app_context.system_columns_service();
        
        // Get new SeqId for this UPDATE operation (new version)
        let (new_seq, _is_new) = sys_cols.handle_update()?;
        
        // Create new version with updated fields
        updated_row._seq = new_seq;
        // _deleted remains the same (false for active records)

        // Write updated row back
        UserTableStoreExt::put(
            self.store.as_ref(),
            &key,
            &updated_row,
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to write updated row: {}", e)))?;

        log::debug!(
            "Updated row {} in {}.{} for user {}",
            row_id,
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        // ✅ REQUIREMENT 2: Notification AFTER storage success
        // ✅ REQUIREMENT 1 & 3: Async fire-and-forget pattern
        if let Some(manager) = &self.live_query_manager {
            // CRITICAL: Use fully qualified table name (namespace.table_name) for notification matching
            let qualified_table_name = format!("{}.{}", namespace_id.as_str(), table_name.as_str());

            // Add user_id to notification data for filter matching
            let mut notification_data = updated_row;
            if let Some(obj) = notification_data.fields.as_object_mut() {
                obj.insert("user_id".to_string(), serde_json::json!(user_id.as_str()));
            }

            let notification = ChangeNotification::update(
                qualified_table_name.clone(),
                old_data,
                serde_json::to_value(notification_data).unwrap(),
            );

            let table_id = TableId::new(namespace_id.clone(), table_name.clone());
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }

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
            let updated_id =
                self.update_row(namespace_id, table_name, user_id, &row_id, updates)?;
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

    /// Parse RFC3339 timestamp string to nanoseconds since epoch
    ///
    /// # Arguments
    /// * `timestamp_str` - RFC3339 formatted timestamp string
    ///
    /// # Returns
    /// Nanoseconds since Unix epoch
    fn parse_timestamp_to_nanos(timestamp_str: &str) -> Result<i64, KalamDbError> {
        use chrono::{DateTime, Utc};
        
        let dt = DateTime::parse_from_rfc3339(timestamp_str)
            .map_err(|e| KalamDbError::InvalidOperation(
                format!("Failed to parse timestamp '{}': {}", timestamp_str, e)
            ))?;
        
        let utc_dt: DateTime<Utc> = dt.with_timezone(&Utc);
        Ok(utc_dt.timestamp_nanos_opt().unwrap_or(0))
    }

    /// Format nanoseconds since epoch to RFC3339 timestamp string
    ///
    /// # Arguments
    /// * `nanos` - Nanoseconds since Unix epoch
    ///
    /// # Returns
    /// RFC3339 formatted timestamp string
    fn format_nanos_to_timestamp(nanos: i64) -> String {
        use chrono::{DateTime, Utc};
        
        // Convert nanoseconds to DateTime
        let secs = nanos / 1_000_000_000;
        let nsecs = (nanos % 1_000_000_000) as u32;
        
        if let Some(dt) = DateTime::from_timestamp(secs, nsecs) {
            dt.to_rfc3339()
        } else {
            // Fallback to current time if conversion fails
            Utc::now().to_rfc3339()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::user_tables::user_table_store::{
        new_user_table_store, UserTableRow,
    };
    use crate::tables::UserTableStore;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn setup_test_handler() -> (UserTableUpdateHandler, Arc<UserTableStore>) {
        let backend = Arc::new(InMemoryBackend::new());
        let store = Arc::new(new_user_table_store(
            backend,
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
        ));
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
        let row = UserTableRow {
            user_id: user_id.clone(),
            _seq: SeqId::new(1),
            _deleted: false,
            fields: initial_data,
        };
        let row_id = row._seq.as_i64().to_string();
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            &row_id,
            &row,
        )
        .unwrap();

        // Update the row
        let updates = serde_json::json!({"age": 31, "city": "NYC"});
        let updated_row_id = handler
            .update_row(&namespace_id, &table_name, &user_id, &row_id, updates)
            .unwrap();

        assert_eq!(updated_row_id, row_id);

        // Verify the update
        let stored = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            &row_id,
        )
        .unwrap()
        .expect("Row should exist");
        assert_eq!(stored.fields["name"], "Alice"); // Unchanged
        assert_eq!(stored.fields["age"], 31); // Updated
        assert_eq!(stored.fields["city"], "NYC"); // New field
        assert_eq!(stored._deleted, false); // Unchanged
        assert!(stored._seq.as_i64() > 1); // New version created
    }

    #[test]
    fn test_update_nonexistent_row() {
        let (handler, _store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let updates = serde_json::json!({"age": 31});
        let result =
            handler.update_row(&namespace_id, &table_name, &user_id, "nonexistent", updates);

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
        let row1 = UserTableRow {
            user_id: user_id.clone(),
            _seq: SeqId::new(1),
            _deleted: false,
            fields: serde_json::json!({"name": "Alice", "age": 30}),
        };
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "1",
            &row1,
        )
        .unwrap();

        let row2 = UserTableRow {
            user_id: user_id.clone(),
            _seq: SeqId::new(2),
            _deleted: false,
            fields: serde_json::json!({"name": "Bob", "age": 25}),
        };
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "2",
            &row2,
        )
        .unwrap();

        // Update batch
        let row_updates = vec![
            ("1".to_string(), serde_json::json!({"age": 31})),
            ("2".to_string(), serde_json::json!({"age": 26})),
        ];

        let updated_ids = handler
            .update_batch(&namespace_id, &table_name, &user_id, row_updates)
            .unwrap();

        assert_eq!(updated_ids.len(), 2);

        // Verify updates
        let stored1 = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "1",
        )
        .unwrap()
        .expect("Row1 should exist");
        assert_eq!(stored1.fields["age"], 31);

        let stored2 = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "2",
        )
        .unwrap()
        .expect("Row2 should exist");
        assert_eq!(stored2.fields["age"], 26);
    }

    #[test]
    fn test_update_prevents_system_column_modification() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        // Insert initial row
        let row = UserTableRow {
            user_id: user_id.clone(),
            _seq: SeqId::new(1),
            _deleted: false,
            fields: serde_json::json!({"name": "Alice"}),
        };
        let row_id = row._seq.as_i64().to_string();
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            &row_id,
            &row,
        )
        .unwrap();

        // Try to update system columns
        let updates = serde_json::json!({
            "name": "Bob",
            "_seq": 9999, // Should be ignored
            "_deleted": true  // Should be ignored
        });

        handler
            .update_row(&namespace_id, &table_name, &user_id, &row_id, updates)
            .unwrap();

        // Verify system columns were NOT modified by updates
        let stored = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            &row_id,
        )
        .unwrap()
        .expect("Row should exist");
        assert_eq!(stored.fields["name"], "Bob"); // User field updated
        assert_eq!(stored._deleted, false); // System column unchanged
        assert!(stored._seq.as_i64() > 1); // New version created (not 9999)
    }

    #[test]
    fn test_update_data_isolation() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user1 = UserId::new("user1".to_string());
        let user2 = UserId::new("user2".to_string());

        // Insert rows for different users
        let row1 = UserTableRow {
            user_id: user1.clone(),
            _seq: SeqId::new(1),
            _deleted: false,
            fields: serde_json::json!({"name": "Alice"}),
        };
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user1.as_str(),
            "1",
            &row1,
        )
        .unwrap();

        let row2 = UserTableRow {
            user_id: user2.clone(),
            _seq: SeqId::new(1),
            _deleted: false,
            fields: serde_json::json!({"name": "Bob"}),
        };
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user2.as_str(),
            "1",
            &row2,
        )
        .unwrap();

        // Update user1's row
        handler
            .update_row(
                &namespace_id,
                &table_name,
                &user1,
                "1",
                serde_json::json!({"name": "Alice Updated"}),
            )
            .unwrap();

        // Verify user2's row is unchanged
        let stored2 = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user2.as_str(),
            "1",
        )
        .unwrap()
        .expect("User2's row should exist");
        assert_eq!(stored2.fields["name"], "Bob"); // Unchanged
    }

    #[test]
    fn test_update_non_object_fails() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        // Insert initial row
        let row = UserTableRow {
            user_id: user_id.clone(),
            _seq: SeqId::new(1),
            _deleted: false,
            fields: serde_json::json!({"name": "Alice"}),
        };
        let row_id = row._seq.as_i64().to_string();
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            &row_id,
            &row,
        )
        .unwrap();

        // Try to update with non-object
        let updates = serde_json::json!(["not", "an", "object"]);
        let result = handler.update_row(&namespace_id, &table_name, &user_id, &row_id, updates);

        assert!(result.is_err());
        match result {
            Err(KalamDbError::InvalidOperation(msg)) => {
                assert!(msg.contains("JSON object"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }
}
