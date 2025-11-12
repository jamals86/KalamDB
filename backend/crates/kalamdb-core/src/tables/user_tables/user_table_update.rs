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
/// - MVCC append-only updates via unified_dml::append_version_sync()
/// - kalamdb-store for RocksDB operations
pub struct UserTableUpdateHandler {
    app_context: Arc<crate::app_context::AppContext>,
    store: Arc<UserTableStore>,
    live_query_manager: Option<Arc<LiveQueryManager>>,
}

impl UserTableUpdateHandler {
    /// Create a new user table UPDATE handler
    ///
    /// # Arguments
    /// * `app_context` - Application context for unified_dml access
    /// * `store` - UserTableStore instance for RocksDB operations
    pub fn new(app_context: Arc<crate::app_context::AppContext>, store: Arc<UserTableStore>) -> Self {
        Self {
            app_context,
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
    /// * `table_id` - Table identifier for unified_dml
    /// * `user_id` - User ID for data isolation
    /// * `row_id` - Row ID to update (existing SeqId)
    /// * `updates` - Fields to update as JSON object (partial updates supported)
    ///
    /// # Returns
    /// The NEW SeqId of the appended version (different from input row_id)
    ///
    /// # MVCC Architecture (Phase 2 - T032)
    /// - Fetches existing row by old SeqId
    /// - Merges updates into existing fields
    /// - Delegates to unified_dml::append_version_sync() to create new version
    /// - Returns NEW SeqId (not the old row_id)
    /// - System columns (_seq, _deleted) are managed by unified_dml
    pub fn update_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        table_id: &TableId,
        user_id: &UserId,
        row_id: &str,
        updates: JsonValue,
    ) -> Result<String, KalamDbError> {
        use crate::app_context::AppContext;
        use crate::tables::unified_dml::append_version_sync;
        use kalamdb_commons::models::schemas::TableType;

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

        // Clone for old_data before merging (for notifications)
        let old_data = existing_row.fields.clone();

        // Merge updates into existing fields
        let mut updated_fields = existing_row.fields.clone();
        if let Some(updated_obj) = updates.as_object() {
            if let Some(fields_obj) = updated_fields.as_object_mut() {
                for (key, value) in updated_obj {
                    // Prevent updates to system columns
                    if key == "_seq" || key == "_deleted" {
                        log::warn!("Attempted to update system column '{}', ignored", key);
                        continue;
                    }
                    fields_obj.insert(key.clone(), value.clone());
                }
            }
        }

        // Delegate to unified_dml::append_version_sync() (Phase 2 - T032)
        let new_seq_id = append_version_sync(
            Arc::clone(&self.app_context),
            table_id,
            TableType::User,
            Some(user_id.clone()),
            updated_fields.clone(),
            false, // UPDATE maintains _deleted=false (use DELETE for soft delete)
        )?;

        log::debug!(
            "Updated row {} → new version {} in {}.{} for user {}",
            row_id,
            new_seq_id,
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
            let mut notification_data = updated_fields.clone();
            if let Some(obj) = notification_data.as_object_mut() {
                obj.insert("user_id".to_string(), serde_json::json!(user_id.as_str()));
            }

            let notification = ChangeNotification::update(
                qualified_table_name.clone(),
                old_data,
                notification_data,
            );

            manager.notify_table_change_async(user_id.clone(), table_id.clone(), notification);
        }

        // Return NEW SeqId (MVCC: each UPDATE creates a new version)
        Ok(new_seq_id.to_string())
    }

    /// Update multiple rows in batch
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `table_id` - Table identifier for unified_dml
    /// * `user_id` - User ID for data isolation
    /// * `row_updates` - Vector of (row_id, updates) pairs
    ///
    /// # Returns
    /// Vector of NEW SeqIds (one per updated row)
    pub fn update_batch(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        table_id: &TableId,
        user_id: &UserId,
        row_updates: Vec<(String, JsonValue)>,
    ) -> Result<Vec<String>, KalamDbError> {
        let mut updated_row_ids = Vec::with_capacity(row_updates.len());

        // Update each row individually (UserTableStore doesn't have batch update yet)
        for (row_id, updates) in row_updates {
            // Call update_row for each row
            let updated_id =
                self.update_row(namespace_id, table_name, table_id, user_id, &row_id, updates)?;
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
        let app_context = Arc::new(crate::app_context::AppContext::new_test());
        let handler = UserTableUpdateHandler::new(app_context, store.clone());
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
        let seq = SeqId::new(1);
        let row = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq,
            _deleted: false,
            fields: initial_data,
        };
        let row_id_key = UserTableRowId::new(user_id.clone(), seq);
        let row_id_str = seq.as_i64().to_string();
        UserTableStoreExt::put(
            store.as_ref(),
            &row_id_key,
            &row,
        )
        .unwrap();

        // Update the row
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        let updates = serde_json::json!({"age": 31, "city": "NYC"});
        let updated_row_id = handler
            .update_row(&namespace_id, &table_name, &table_id, &user_id, &row_id_str, updates)
            .unwrap();

        // Verify NEW SeqId is returned (MVCC: UPDATE creates new version)
        assert_ne!(updated_row_id, row_id_str, "UPDATE should return NEW SeqId");

        // Verify the update
        let stored = UserTableStoreExt::get(
            store.as_ref(),
            &row_id_key,
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
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        let user_id = UserId::new("user123".to_string());

        let updates = serde_json::json!({"age": 31});
        let result =
            handler.update_row(&namespace_id, &table_name, &table_id, &user_id, "nonexistent", updates);

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
        let row_id_1 = UserTableRowId::new(user_id.clone(), SeqId::new(1));
        UserTableStoreExt::put(
            store.as_ref(),
            &row_id_1,
            &row1,
        )
        .unwrap();

        let row2 = UserTableRow {
            user_id: user_id.clone(),
            _seq: SeqId::new(2),
            _deleted: false,
            fields: serde_json::json!({"name": "Bob", "age": 25}),
        };
        let row_id_2 = UserTableRowId::new(user_id.clone(), SeqId::new(2));
        UserTableStoreExt::put(
            store.as_ref(),
            &row_id_2,
            &row2,
        )
        .unwrap();

        // Update batch
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        let row_updates = vec![
            ("1".to_string(), serde_json::json!({"age": 31})),
            ("2".to_string(), serde_json::json!({"age": 26})),
        ];

        let updated_ids = handler
            .update_batch(&namespace_id, &table_name, &table_id, &user_id, row_updates)
            .unwrap();

        assert_eq!(updated_ids.len(), 2);

        // Verify updates
        let stored1 = UserTableStoreExt::get(
            store.as_ref(),
            &row_id_1,
        )
        .unwrap()
        .expect("Row1 should exist");
        assert_eq!(stored1.fields["age"], 31);

        let stored2 = UserTableStoreExt::get(
            store.as_ref(),
            &row_id_2,
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
        let seq = SeqId::new(1);
        let row = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq,
            _deleted: false,
            fields: serde_json::json!({"name": "Alice"}),
        };
        let row_id_key = UserTableRowId::new(user_id.clone(), seq);
        let row_id = seq.as_i64().to_string();
        UserTableStoreExt::put(
            store.as_ref(),
            &row_id_key,
            &row,
        )
        .unwrap();

        // Try to update system columns
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        let updates = serde_json::json!({
            "name": "Bob",
            "_seq": 9999, // Should be ignored
            "_deleted": true  // Should be ignored
        });

        handler
            .update_row(&namespace_id, &table_name, &table_id, &user_id, &row_id, updates)
            .unwrap();

        // Verify system columns were NOT modified by updates
        let stored = UserTableStoreExt::get(
            store.as_ref(),
            &row_id_key,
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
        let row_id_user1 = UserTableRowId::new(user1.clone(), SeqId::new(1));
        UserTableStoreExt::put(
            store.as_ref(),
            &row_id_user1,
            &row1,
        )
        .unwrap();

        let row2 = UserTableRow {
            user_id: user2.clone(),
            _seq: SeqId::new(1),
            _deleted: false,
            fields: serde_json::json!({"name": "Bob"}),
        };
        let row_id_user2 = UserTableRowId::new(user2.clone(), SeqId::new(1));
        UserTableStoreExt::put(
            store.as_ref(),
            &row_id_user2,
            &row2,
        )
        .unwrap();

        // Update user1's row
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        handler
            .update_row(
                &namespace_id,
                &table_name,
                &table_id,
                &user1,
                "1",
                serde_json::json!({"name": "Alice Updated"}),
            )
            .unwrap();

        // Verify user2's row is unchanged
        let stored2 = UserTableStoreExt::get(
            store.as_ref(),
            &row_id_user2,
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
        let seq = SeqId::new(1);
        let row = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq,
            _deleted: false,
            fields: serde_json::json!({"name": "Alice"}),
        };
        let row_id_key = UserTableRowId::new(user_id.clone(), seq);
        let row_id = seq.as_i64().to_string();
        UserTableStoreExt::put(
            store.as_ref(),
            &row_id_key,
            &row,
        )
        .unwrap();

        // Try to update with non-object
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        let updates = serde_json::json!(["not", "an", "object"]);
        let result = handler.update_row(&namespace_id, &table_name, &table_id, &user_id, &row_id, updates);

        assert!(result.is_err());
        match result {
            Err(KalamDbError::InvalidOperation(msg)) => {
                assert!(msg.contains("JSON object"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }
}
