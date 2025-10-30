//! User table DELETE operations (soft delete)
//!
//! This module handles DELETE operations for user tables with:
//! - Soft delete: Sets _deleted = true instead of removing data
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic _updated timestamp update
//! - Data isolation enforcement
//! - Atomic delete operations

use crate::catalog::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::stores::system_table::UserTableStoreExt;
use crate::tables::UserTableStore;
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
        // Fetch row data BEFORE deleting (for notification)
        let row_to_delete = UserTableStoreExt::get(
            self.store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
        )?;

        // Soft delete via store (automatically updates _deleted and _updated)
        UserTableStoreExt::delete(
            self.store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
            false,
        )
            .map_err(|e| {
                // Check if it's a "not found" error
                if e.to_string().contains("Column family not found") {
                    KalamDbError::NotFound(format!(
                        "Column family not found for table: {}.{}",
                        namespace_id.as_str(),
                        table_name.as_str()
                    ))
                } else {
                    KalamDbError::Other(format!("Failed to delete row: {}", e))
                }
            })?;

        log::debug!(
            "Soft deleted row {} in {}.{} for user {}",
            row_id,
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        // ✅ REQUIREMENT 2: Notification AFTER storage success
        // ✅ REQUIREMENT 1 & 3: Async fire-and-forget pattern
        if let Some(manager) = &self.live_query_manager {
            if let Some(row_data) = row_to_delete {
                // Convert UserTableRow to JsonValue for notification
                let mut data = serde_json::to_value(&row_data.fields)
                    .unwrap_or(serde_json::json!({}));
                
                // CRITICAL: Use fully qualified table name (namespace.table_name) for notification matching
                let qualified_table_name =
                    format!("{}.{}", namespace_id.as_str(), table_name.as_str());

                // Add user_id to notification data for filter matching
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("user_id".to_string(), serde_json::json!(user_id.as_str()));
                }

                let notification =
                    ChangeNotification::delete_soft(qualified_table_name.clone(), data);

                let mgr = Arc::clone(manager);
                tokio::spawn(async move {
                    // ✅ REQUIREMENT 2: Log errors, don't propagate
                    if let Err(e) = mgr
                        .notify_table_change(&qualified_table_name, notification)
                        .await
                    {
                        log::warn!("Failed to notify subscribers for DELETE: {}", e);
                    }
                });
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::user_tables::user_table_store::{
        new_user_table_store, UserTableRow, UserTableRowId,
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
        let key = UserTableRowId::new(user_id.clone(), row_id.to_string());
        let entity = UserTableRow {
            row_id: row_id.to_string(),
            user_id: user_id.as_str().to_string(),
            fields: json!({"name": "Alice", "age": 30}),
            _updated: chrono::Utc::now().to_rfc3339(),
            _deleted: false,
        };
        UserTableStoreExt::put(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user_id.as_str(), row_id, &entity).unwrap();

        // Soft delete the row
        let deleted_row_id = handler
            .delete_row(&namespace_id, &table_name, &user_id, row_id)
            .unwrap();

        assert_eq!(deleted_row_id, row_id);

        // Verify the row is marked as deleted
        let stored = UserTableStoreExt::get(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user_id.as_str(), row_id).unwrap();

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
            let result = UserTableStoreExt::get(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user_id.as_str(), row_id)
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

        UserTableStoreExt::put(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user1.as_str(), "row1", &UserTableRow { row_id: "row1".to_string(), user_id: user1.as_str().to_string(),
                    fields: serde_json::json!({"name": "Alice"}),
                    _updated: chrono::Utc::now().to_rfc3339(),
                    _deleted: false,
                },
            )
            .unwrap();
        UserTableStoreExt::put(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user2.as_str(), "row1", &UserTableRow { row_id: "row1".to_string(), user_id: user2.as_str().to_string(),
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
        let result1 = UserTableStoreExt::get(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user1.as_str(), "row1")
            .unwrap();
        assert!(result1.is_none(), "user1's row should be soft-deleted");

        // Verify user2's row is NOT deleted
        let result2 = UserTableStoreExt::get(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user2.as_str(), "row1")
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

        UserTableStoreExt::put(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row1", &UserTableRow { row_id: "row1".to_string(), user_id: user_id.as_str().to_string(),
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
        let result = UserTableStoreExt::get(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row1")
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

        UserTableStoreExt::put(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row1", &UserTableRow { row_id: "row1".to_string(), user_id: user_id.as_str().to_string(),
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
        let result = UserTableStoreExt::get(store.as_ref(), namespace_id.as_str(), table_name.as_str(), user_id.as_str(), "row1")
            .unwrap();
        assert!(result.is_none(), "Row should remain soft-deleted");
    }
}
