//! Change detection system for live query notifications
//!
//! This module provides the infrastructure for detecting data changes (INSERT/UPDATE/DELETE)
//! and notifying live query subscribers. It acts as a bridge between the storage layer
//! (kalamdb-store) and the live query system (LiveQueryManager).
//!
//! # Architecture
//!
//! ```text
//! UserTableStore/SharedTableStore
//!         ↓
//!   ChangeDetector (this module)
//!         ↓
//!   LiveQueryManager.notify_table_change()
//!         ↓
//!   WebSocket connections (Phase 14)
//! ```
//!
//! # Usage
//!
//! The ChangeDetector wraps store operations and automatically generates change
//! notifications:
//!
//! ```rust,ignore
//! let detector = ChangeDetector::new(user_table_store, Arc::clone(&live_query_manager));
//!
//! // INSERT: Automatically detects new row and notifies subscribers
//! detector.put(namespace_id, table_name, user_id, row_id, row_data).await?;
//!
//! // UPDATE: Fetches old values, compares, notifies with old+new data
//! detector.put(namespace_id, table_name, user_id, existing_row_id, new_data).await?;
//!
//! // DELETE: Notifies with deleted row data
//! detector.delete(namespace_id, table_name, user_id, row_id, hard=false).await?;
//! ```

use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, ChangeType, LiveQueryManager};
use kalamdb_store::{SharedTableStore, StreamTableStore, UserTableStore};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Change detector for user tables
///
/// Wraps UserTableStore operations and generates change notifications
/// for live query subscribers.
pub struct UserTableChangeDetector {
    store: Arc<UserTableStore>,
    live_query_manager: Arc<LiveQueryManager>,
}

impl UserTableChangeDetector {
    /// Create a new user table change detector
    pub fn new(store: Arc<UserTableStore>, live_query_manager: Arc<LiveQueryManager>) -> Self {
        Self {
            store,
            live_query_manager,
        }
    }

    /// Insert or update a row with change notification
    ///
    /// # Change Detection Logic
    ///
    /// 1. Check if row exists (get old value)
    /// 2. If exists → UPDATE notification (old + new values)
    /// 3. If not exists → INSERT notification (new values only)
    /// 4. Store the row
    /// 5. Notify subscribers asynchronously
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    /// * `user_id` - User identifier (for row-level isolation)
    /// * `row_id` - Row identifier
    /// * `row_data` - New row data (system columns added by store)
    pub async fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
        row_data: JsonValue,
    ) -> Result<(), KalamDbError> {
        // Step 1: Check if row exists (for INSERT vs UPDATE detection)
        let old_value = self
            .store
            .get(namespace_id, table_name, user_id, row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Step 2: Determine change type
        let change_type = if old_value.is_some() {
            ChangeType::Update
        } else {
            ChangeType::Insert
        };

        // Step 3: Store the row (system columns injected by store)
        self.store
            .put(namespace_id, table_name, user_id, row_id, row_data.clone())
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Step 4: Get the stored value with system columns
        let new_value = self
            .store
            .get(namespace_id, table_name, user_id, row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::Other("Failed to retrieve stored row".to_string()))?;

        // T175: Filter out _deleted=true rows from INSERT/UPDATE notifications
        // Only send DELETE notification for deleted rows
        let is_deleted = new_value
            .get("_deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_deleted {
            // Row is soft-deleted, skip INSERT/UPDATE notification
            // DELETE notification will be sent by the delete() method
            return Ok(());
        }

        // Step 5: Create appropriate notification based on change type
        let notification = match change_type {
            ChangeType::Update => {
                // UPDATE: include both old and new values
                let old_data = old_value.ok_or_else(|| {
                    KalamDbError::Other("UPDATE detected but old value is None".to_string())
                })?;
                ChangeNotification::update(table_name.to_string(), old_data, new_value)
            }
            ChangeType::Insert => {
                // INSERT: only new values
                ChangeNotification::insert(table_name.to_string(), new_value)
            }
            _ => {
                return Err(KalamDbError::Other("Invalid change type".to_string()));
            }
        };

        // Step 6: Notify subscribers asynchronously
        let manager = Arc::clone(&self.live_query_manager);
        let table_name_copy = table_name.to_string();

        // Spawn async notification task (non-blocking)
        tokio::spawn(async move {
            if let Err(e) = manager
                .notify_table_change(&table_name_copy, notification)
                .await
            {
                #[cfg(debug_assertions)]
                eprintln!("Failed to notify subscribers: {}", e);
            }
        });

        Ok(())
    }

    /// Delete a row with change notification
    ///
    /// # Arguments
    ///
    /// * `hard` - If true, physical delete. If false, soft delete (_deleted=true)
    pub async fn delete(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
        hard: bool,
    ) -> Result<(), KalamDbError> {
        // Step 1: Get existing row data (for notification)
        let old_value = self
            .store
            .get(namespace_id, table_name, user_id, row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Step 2: Perform deletion
        self.store
            .delete(namespace_id, table_name, user_id, row_id, hard)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Step 3: Create appropriate notification based on delete type
        let notification = if hard {
            // Hard delete: row physically removed, only send row_id
            ChangeNotification::delete_hard(table_name.to_string(), row_id.to_string())
        } else {
            // Soft delete: get updated row with _deleted=true
            let deleted_row = self
                .store
                .get_include_deleted(namespace_id, table_name, user_id, row_id)
                .map_err(|e| KalamDbError::Other(e.to_string()))?
                .unwrap_or_else(|| old_value.clone().unwrap_or(serde_json::json!({})));

            // Soft delete sends the row with _deleted=true
            ChangeNotification::delete_soft(table_name.to_string(), deleted_row)
        };

        // Step 4: Notify subscribers
        let manager = Arc::clone(&self.live_query_manager);
        let table_name_copy = table_name.to_string();

        tokio::spawn(async move {
            if let Err(e) = manager
                .notify_table_change(&table_name_copy, notification)
                .await
            {
                #[cfg(debug_assertions)]
                eprintln!("Failed to notify subscribers: {}", e);
            }
        });

        Ok(())
    }

    /// Get the underlying store (for read operations)
    pub fn store(&self) -> &Arc<UserTableStore> {
        &self.store
    }
}

/// Change detector for shared tables
///
/// Similar to UserTableChangeDetector but for shared tables (no user_id isolation)
pub struct SharedTableChangeDetector {
    store: Arc<SharedTableStore>,
    live_query_manager: Arc<LiveQueryManager>,
}

impl SharedTableChangeDetector {
    /// Create a new shared table change detector
    pub fn new(store: Arc<SharedTableStore>, live_query_manager: Arc<LiveQueryManager>) -> Self {
        Self {
            store,
            live_query_manager,
        }
    }

    /// Insert or update a row with change notification
    pub async fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        row_data: JsonValue,
    ) -> Result<(), KalamDbError> {
        // Check if row exists
        let old_value = self
            .store
            .get(namespace_id, table_name, row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        let change_type = if old_value.is_some() {
            ChangeType::Update
        } else {
            ChangeType::Insert
        };

        // Store the row
        self.store
            .put(namespace_id, table_name, row_id, row_data.clone())
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Get stored value with system columns
        let new_value = self
            .store
            .get(namespace_id, table_name, row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::Other("Failed to retrieve stored row".to_string()))?;

        // T175: Filter out _deleted=true rows from INSERT/UPDATE notifications
        // Only send DELETE notification for deleted rows
        let is_deleted = new_value
            .get("_deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_deleted {
            // Row is soft-deleted, skip INSERT/UPDATE notification
            // DELETE notification will be sent by the delete() method
            return Ok(());
        }

        // Create appropriate notification based on change type
        let notification = match change_type {
            ChangeType::Update => {
                let old_data = old_value.ok_or_else(|| {
                    KalamDbError::Other("UPDATE detected but old value is None".to_string())
                })?;
                ChangeNotification::update(table_name.to_string(), old_data, new_value)
            }
            ChangeType::Insert => ChangeNotification::insert(table_name.to_string(), new_value),
            _ => {
                return Err(KalamDbError::Other("Invalid change type".to_string()));
            }
        };

        // Notify subscribers
        let manager = Arc::clone(&self.live_query_manager);
        let table_name_copy = table_name.to_string();

        tokio::spawn(async move {
            if let Err(e) = manager
                .notify_table_change(&table_name_copy, notification)
                .await
            {
                #[cfg(debug_assertions)]
                eprintln!("Failed to notify subscribers: {}", e);
            }
        });

        Ok(())
    }

    /// Delete a row with change notification
    pub async fn delete(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        hard: bool,
    ) -> Result<(), KalamDbError> {
        // Get existing row
        let old_value = self
            .store
            .get(namespace_id, table_name, row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Perform deletion
        self.store
            .delete(namespace_id, table_name, row_id, hard)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Create appropriate notification based on delete type
        let notification = if hard {
            // Hard delete: row physically removed, only send row_id
            ChangeNotification::delete_hard(table_name.to_string(), row_id.to_string())
        } else {
            // Soft delete: get updated row with _deleted=true
            let deleted_row = self
                .store
                .get_include_deleted(namespace_id, table_name, row_id)
                .map_err(|e| KalamDbError::Other(e.to_string()))?
                .unwrap_or_else(|| old_value.clone().unwrap_or(serde_json::json!({})));

            // Soft delete sends the row with _deleted=true
            ChangeNotification::delete_soft(table_name.to_string(), deleted_row)
        };

        // Notify subscribers
        let manager = Arc::clone(&self.live_query_manager);
        let table_name_copy = table_name.to_string();

        tokio::spawn(async move {
            if let Err(e) = manager
                .notify_table_change(&table_name_copy, notification)
                .await
            {
                #[cfg(debug_assertions)]
                eprintln!("Failed to notify subscribers: {}", e);
            }
        });

        Ok(())
    }

    /// Get the underlying store
    pub fn store(&self) -> &Arc<SharedTableStore> {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_query::connection_registry::NodeId;
    use crate::storage::RocksDbInit;
    use kalamdb_sql::KalamSql;
    use tempfile::TempDir;

    async fn create_test_detector() -> (UserTableChangeDetector, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();

        let user_table_store = Arc::new(UserTableStore::new(Arc::clone(&db)).unwrap());
        let shared_table_store = Arc::new(SharedTableStore::new(Arc::clone(&db)).unwrap());
        let stream_table_store = Arc::new(StreamTableStore::new(Arc::clone(&db)).unwrap());
        let backend: Arc<dyn kalamdb_store::storage_trait::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(Arc::clone(&db)));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());
        let live_query_manager = Arc::new(LiveQueryManager::new(
            kalam_sql,
            NodeId::new("test".to_string()),
            Some(user_table_store.clone()),
            Some(shared_table_store.clone()),
            Some(stream_table_store.clone()),
        ));

        let detector = UserTableChangeDetector::new(user_table_store, live_query_manager);

        (detector, temp_dir)
    }

    #[tokio::test]
    async fn test_insert_detection() {
        let (detector, _temp_dir) = create_test_detector().await;

        let namespace_id = "default";
        let table_name = "messages";
        let user_id = "user1";
        let row_id = "msg1";
        let row_data = serde_json::json!({"text": "Hello"});

        detector
            .put(namespace_id, table_name, user_id, row_id, row_data)
            .await
            .unwrap();

        // Verify row exists
        let retrieved = detector
            .store()
            .get(namespace_id, table_name, user_id, row_id)
            .unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_update_detection() {
        let (detector, _temp_dir) = create_test_detector().await;

        let namespace_id = "default";
        let table_name = "messages";
        let user_id = "user1";
        let row_id = "msg1";

        // Insert initial row
        let initial_data = serde_json::json!({"text": "Hello"});
        detector
            .put(namespace_id, table_name, user_id, row_id, initial_data)
            .await
            .unwrap();

        // Update the row
        let updated_data = serde_json::json!({"text": "Hello World"});
        detector
            .put(namespace_id, table_name, user_id, row_id, updated_data)
            .await
            .unwrap();

        // Verify updated value
        let retrieved = detector
            .store()
            .get(namespace_id, table_name, user_id, row_id)
            .unwrap()
            .unwrap();
        assert_eq!(retrieved["text"], "Hello World");
    }

    #[tokio::test]
    async fn test_delete_notification() {
        let (detector, _temp_dir) = create_test_detector().await;

        let namespace_id = "default";
        let table_name = "messages";
        let user_id = "user1";
        let row_id = "msg1";

        // Insert row
        let row_data = serde_json::json!({"text": "Hello"});
        detector
            .put(namespace_id, table_name, user_id, row_id, row_data)
            .await
            .unwrap();

        // Soft delete
        detector
            .delete(namespace_id, table_name, user_id, row_id, false)
            .await
            .unwrap();

        // Verify soft deleted (get returns None)
        let retrieved = detector
            .store()
            .get(namespace_id, table_name, user_id, row_id)
            .unwrap();
        assert!(retrieved.is_none());
    }
}
