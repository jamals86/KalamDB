//! User table DELETE operations (soft delete)
//!
//! This module handles DELETE operations for user tables with:
//! - Soft delete: Sets _deleted = true instead of removing data
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic _updated timestamp update
//! - Data isolation enforcement
//! - Atomic delete operations

use crate::schema_registry::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::tables::system::system_table_store::UserTableStoreExt;
use crate::tables::UserTableStore;
use kalamdb_commons::models::TableId;
use std::sync::Arc;

/// User table DELETE handler
///
/// Implements soft delete for user tables by marking rows as deleted
/// instead of physically removing them. This allows for:
/// - Data recovery within retention period
/// - Audit trail maintenance
/// - Filtered queries (WHERE _deleted = false)
pub struct UserTableDeleteHandler {
    store: Arc<UserTableStore>,
    live_query_manager: Option<Arc<LiveQueryManager>>,
}

impl UserTableDeleteHandler {
    /// Create a new user table DELETE handler
    ///
    /// # Arguments
    /// * `store` - UserTableStore instance
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
    /// # System Columns
    /// Automatically updates:
    /// - `_deleted`: BOOLEAN = true
    /// - `_updated`: TIMESTAMP = NOW()
    ///
    /// # Note
    /// This performs a soft delete. The row is marked as deleted but not physically removed.
    pub fn delete_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
    ) -> Result<String, KalamDbError> {
        // Fetch existing row to get current _updated timestamp
        let existing_row = UserTableStoreExt::get(
            self.store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
        )?
        .ok_or_else(|| {
            KalamDbError::NotFound(format!(
                "Row {} not found in {}.{} for user {}",
                row_id,
                namespace_id.as_str(),
                table_name.as_str(),
                user_id.as_str()
            ))
        })?;

        // Clone for notification before modifying
        let row_data_before = existing_row.clone();

        // T049-T050: Use SystemColumnsService for soft delete with monotonic _updated
        // Parse existing _updated timestamp to nanoseconds
        let previous_updated_ns = Self::parse_timestamp_to_nanos(&existing_row._updated)?;
        
        // Get AppContext and SystemColumnsService (T025, T048)
        use crate::app_context::AppContext;
        let app_context = AppContext::get();
        let sys_cols = app_context.system_columns_service();
        
        // Snowflake ID is stored in row_id as String, parse to i64
        let record_id = existing_row.row_id.parse::<i64>()
            .map_err(|_| KalamDbError::InvalidOperation(
                format!("Invalid row_id format: {}", existing_row.row_id)
            ))?;
        
        // Get new _updated timestamp and _deleted=true (monotonic, >= previous + 1ns)
        let (new_updated_ns, deleted) = sys_cols.handle_delete(record_id, previous_updated_ns)?;
        
        // Create updated row with soft delete markers
        let mut deleted_row = existing_row;
        deleted_row._updated = Self::format_nanos_to_timestamp(new_updated_ns);
        deleted_row._deleted = deleted; // Should be true for DELETE

        // Write updated row back to storage (soft delete)
        UserTableStoreExt::put(
            self.store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
            &deleted_row,
        )
        .map_err(|e| {
            KalamDbError::Other(format!("Failed to soft delete row: {}", e))
        })?;

        log::debug!(
            "Soft deleted row {} in {}.{} for user {}",
            row_id,
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        // ✅ REQUIREMENT 2: Notification AFTER storage success
        // ✅ REQUIREMENT 1 & 3: Async fire-and-forget pattern (handled by notify_table_change_async)
        if let Some(manager) = &self.live_query_manager {
            // Convert UserTableRow to JsonValue for notification
            let mut data =
                serde_json::to_value(&row_data_before.fields).unwrap_or(serde_json::json!({}));

            // CRITICAL: Use fully qualified table name (namespace.table_name) for notification matching
            let qualified_table_name =
                format!("{}.{}", namespace_id.as_str(), table_name.as_str());

            // Add user_id to notification data for filter matching
            if let Some(obj) = data.as_object_mut() {
                obj.insert("user_id".to_string(), serde_json::json!(user_id.as_str()));
            }

            let notification =
                ChangeNotification::delete_soft(qualified_table_name.clone(), data);

            let table_id = TableId::new(namespace_id.clone(), table_name.clone());
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }

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
    pub fn delete_batch(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_ids: Vec<String>,
    ) -> Result<Vec<String>, KalamDbError> {
        let mut deleted_row_ids = Vec::with_capacity(row_ids.len());

        // Delete each row individually (store handles atomicity per row)
        for row_id in row_ids {
            let deleted_id = self.delete_row(namespace_id, table_name, user_id, &row_id)?;
            deleted_row_ids.push(deleted_id);
        }

        log::debug!(
            "Batch deleted {} rows in {}.{} for user {}",
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
        // Hard delete via store
        UserTableStoreExt::delete(
            self.store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
            true,
        )
        .map_err(|e| {
            if e.to_string().contains("Column family not found") {
                KalamDbError::NotFound(format!(
                    "Column family not found for table: {}.{}",
                    namespace_id.as_str(),
                    table_name.as_str()
                ))
            } else {
                KalamDbError::Other(format!("Failed to hard delete row: {}", e))
            }
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

    /// Parse RFC3339 timestamp string to nanoseconds since epoch
    /// T049: Timestamp conversion for SystemColumnsService integration
    fn parse_timestamp_to_nanos(timestamp: &str) -> Result<i64, KalamDbError> {
        use chrono::DateTime;
        let dt = DateTime::parse_from_rfc3339(timestamp)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid timestamp format: {}", e)))?;
        Ok(dt.timestamp_nanos_opt().unwrap_or(0))
    }

    /// Format nanoseconds since epoch to RFC3339 timestamp string
    /// T049: Timestamp conversion for SystemColumnsService integration
    fn format_nanos_to_timestamp(nanos: i64) -> String {
        use chrono::{DateTime, Utc};
        let secs = nanos / 1_000_000_000;
        let nsecs = (nanos % 1_000_000_000) as u32;
        let dt = DateTime::from_timestamp(secs, nsecs).unwrap_or_else(|| Utc::now());
        dt.to_rfc3339()
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
    use serde_json::json;

    fn setup_test_handler() -> (UserTableDeleteHandler, Arc<UserTableStore>) {
        let backend = Arc::new(InMemoryBackend::new());
        let store = Arc::new(new_user_table_store(
            backend,
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
        ));
        let handler = UserTableDeleteHandler::new(store.clone());
        (handler, store)
    }

    #[test]
    fn test_delete_row() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());
        let row_id = "row1";

        // Insert initial row
        let entity = UserTableRow {
            row_id: row_id.to_string(),
            user_id: user_id.as_str().to_string(),
            fields: json!({"name": "Alice", "age": 30}),
            _updated: chrono::Utc::now().to_rfc3339(),
            _deleted: false,
        };
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
            &entity,
        )
        .unwrap();

        // Soft delete the row
        let deleted_row_id = handler
            .delete_row(&namespace_id, &table_name, &user_id, row_id)
            .unwrap();

        assert_eq!(deleted_row_id, row_id);

        // Verify the row is marked as deleted
        let stored = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
        )
        .unwrap();

        // Soft-deleted rows are filtered out by store.get()
        assert!(
            stored.is_none(),
            "Soft-deleted row should not be returned by store.get()"
        );
    }

    #[test]
    fn test_delete_nonexistent_row() {
        let (handler, _store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let result = handler.delete_row(&namespace_id, &table_name, &user_id, "nonexistent");

        // Note: UserTableStore.delete() doesn't fail on nonexistent rows in soft delete mode
        // It's a no-op if the row doesn't exist
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_batch() {
        let (handler, store) = setup_test_handler();

        // Insert initial rows
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        // Insert initial rows
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "row1",
            &UserTableRow {
                row_id: "row1".to_string(),
                user_id: user_id.as_str().to_string(),
                fields: serde_json::json!({"name": "Alice"}),
                _updated: chrono::Utc::now().to_rfc3339(),
                _deleted: false,
            },
        )
        .unwrap();
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "row2",
            &UserTableRow {
                row_id: "row2".to_string(),
                user_id: user_id.as_str().to_string(),
                fields: serde_json::json!({"name": "Bob"}),
                _updated: chrono::Utc::now().to_rfc3339(),
                _deleted: false,
            },
        )
        .unwrap();
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "row3",
            &UserTableRow {
                row_id: "row3".to_string(),
                user_id: user_id.as_str().to_string(),
                fields: serde_json::json!({"name": "Charlie"}),
                _updated: chrono::Utc::now().to_rfc3339(),
                _deleted: false,
            },
        )
        .unwrap();

        // Delete batch
        let row_ids = vec!["row1".to_string(), "row2".to_string(), "row3".to_string()];

        let deleted_ids = handler
            .delete_batch(&namespace_id, &table_name, &user_id, row_ids)
            .unwrap();

        assert_eq!(deleted_ids.len(), 3);

        // Verify all rows are soft-deleted (not returned by get)
        for row_id in &["row1", "row2", "row3"] {
            let result = UserTableStoreExt::get(
                store.as_ref(),
                namespace_id.as_str(),
                table_name.as_str(),
                user_id.as_str(),
                row_id,
            )
            .unwrap();
            assert!(result.is_none(), "Row {} should be soft-deleted", row_id);
        }
    }

    #[test]
    fn test_delete_data_isolation() {
        let (handler, store) = setup_test_handler();
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());

        // Insert rows for different users
        let user1 = UserId::new("user1".to_string());
        let user2 = UserId::new("user2".to_string());

        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user1.as_str(),
            "row1",
            &UserTableRow {
                row_id: "row1".to_string(),
                user_id: user1.as_str().to_string(),
                fields: serde_json::json!({"name": "Alice"}),
                _updated: chrono::Utc::now().to_rfc3339(),
                _deleted: false,
            },
        )
        .unwrap();
        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user2.as_str(),
            "row1",
            &UserTableRow {
                row_id: "row1".to_string(),
                user_id: user2.as_str().to_string(),
                fields: serde_json::json!({"name": "Bob"}),
                _updated: chrono::Utc::now().to_rfc3339(),
                _deleted: false,
            },
        )
        .unwrap();

        // Delete user1's row
        handler
            .delete_row(&namespace_id, &table_name, &user1, "row1")
            .unwrap();

        // Verify user1's row is soft-deleted (not returned)
        let result1 = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user1.as_str(),
            "row1",
        )
        .unwrap();
        assert!(result1.is_none(), "user1's row should be soft-deleted");

        // Verify user2's row is NOT deleted
        let result2 = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user2.as_str(),
            "row1",
        )
        .unwrap();
        assert!(result2.is_some(), "user2's row should still exist");
        let row2 = result2.unwrap();
        assert_eq!(row2.fields["name"], "Bob");
    }

    #[test]
    fn test_hard_delete_row() {
        let (handler, store) = setup_test_handler();

        // Insert initial row
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "row1",
            &UserTableRow {
                row_id: "row1".to_string(),
                user_id: user_id.as_str().to_string(),
                fields: serde_json::json!({"name": "Alice"}),
                _updated: chrono::Utc::now().to_rfc3339(),
                _deleted: false,
            },
        )
        .unwrap();

        // Hard delete the row
        handler
            .hard_delete_row(&namespace_id, &table_name, &user_id, "row1")
            .unwrap();

        // Verify the row is completely removed
        let result = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "row1",
        )
        .unwrap();

        assert!(result.is_none(), "Row should be completely removed");
    }

    #[test]
    fn test_multiple_deletes_idempotent() {
        let (handler, store) = setup_test_handler();

        // Insert initial row
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        UserTableStoreExt::put(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "row1",
            &UserTableRow {
                row_id: "row1".to_string(),
                user_id: user_id.as_str().to_string(),
                fields: serde_json::json!({"name": "Alice"}),
                _updated: chrono::Utc::now().to_rfc3339(),
                _deleted: false,
            },
        )
        .unwrap();

        // Delete twice
        handler
            .delete_row(&namespace_id, &table_name, &user_id, "row1")
            .unwrap();

        let result2 = handler.delete_row(&namespace_id, &table_name, &user_id, "row1");

        // Second delete should succeed (idempotent)
        assert!(result2.is_ok());

        // Verify row is still soft-deleted (returns None)
        let result = UserTableStoreExt::get(
            store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            "row1",
        )
        .unwrap();
        assert!(result.is_none(), "Row should remain soft-deleted");
    }
}
