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
use crate::tables::system::system_table_store::{SharedTableStoreExt, UserTableStoreExt};
use crate::tables::{SharedTableRow, SharedTableStore, UserTableStore};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use kalamdb_commons::{NamespaceId, TableName, UserId};

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
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
        row_data: JsonValue,
    ) -> Result<(), KalamDbError> {
        // Step 1: Check if row exists (for INSERT vs UPDATE detection)
        let old_value = UserTableStoreExt::get(
            self.store.as_ref(),
            namespace_id.as_ref(),
            table_name.as_ref(),
            user_id.as_ref(),
            row_id,
        )
        .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Step 2: Determine change type
        let change_type = if old_value.is_some() {
            ChangeType::Update
        } else {
            ChangeType::Insert
        };

        // Step 3: Create UserTableRow from JsonValue (system columns will be updated by store)
        let entity = crate::tables::user_tables::user_table_store::UserTableRow {
            row_id: row_id.to_string(),
            user_id: user_id.as_str().to_string(),
            fields: row_data,
            _updated: chrono::Utc::now().to_rfc3339(),
            _deleted: false,
        };

        // Step 4: Store the row (system columns injected by store)
        UserTableStoreExt::put(
            self.store.as_ref(),
            namespace_id.as_ref(),
            table_name.as_ref(),
            user_id.as_ref(),
            row_id,
            &entity,
        )
        .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Step 4: Get the stored value with system columns
        let new_value = UserTableStoreExt::get_include_deleted(
            self.store.as_ref(),
            namespace_id.as_ref(),
            table_name.as_ref(),
            user_id.as_ref(),
            row_id,
        )
        .map_err(|e| KalamDbError::Other(e.to_string()))?
        .ok_or_else(|| KalamDbError::Other("Failed to retrieve stored row".to_string()))?;

        // T175: Filter out _deleted=true rows from INSERT/UPDATE notifications
        // Only send DELETE notification for deleted rows
        let is_deleted = new_value._deleted;

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
                ChangeNotification::update(
                    table_name.as_str().to_string(),
                    serde_json::to_value(&old_data.fields).unwrap_or(serde_json::json!({})),
                    serde_json::to_value(&new_value.fields).unwrap_or(serde_json::json!({})),
                )
            }
            ChangeType::Insert => {
                // INSERT: only new values
                ChangeNotification::insert(
                    table_name.as_str().to_string(),
                    serde_json::to_value(&new_value.fields).unwrap_or(serde_json::json!({})),
                )
            }
            _ => {
                return Err(KalamDbError::Other("Invalid change type".to_string()));
            }
        };

        // Step 6: Notify subscribers asynchronously
        let manager = Arc::clone(&self.live_query_manager);
        let table_name_copy = table_name.as_str().to_string();

        // Spawn async notification task (non-blocking)
        tokio::spawn(async move {
            if let Err(_e) = manager
                .notify_table_change(&table_name_copy, notification)
                .await
            {
                #[cfg(debug_assertions)]
                eprintln!("Failed to notify subscribers: {}", _e);
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
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
        hard: bool,
    ) -> Result<(), KalamDbError> {
        // Step 1: Get existing row data (for notification)
        let old_value = UserTableStoreExt::get(
            self.store.as_ref(),
            namespace_id.as_ref(),
            table_name.as_ref(),
            user_id.as_ref(),
            row_id,
        )
        .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Step 2: Perform deletion
        UserTableStoreExt::delete(
            self.store.as_ref(),
            namespace_id.as_ref(),
            table_name.as_ref(),
            user_id.as_ref(),
            row_id,
            hard,
        )
        .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Step 3: Create appropriate notification based on delete type
        let notification = if hard {
            // Hard delete: row physically removed, only send row_id
            ChangeNotification::delete_hard(table_name.as_str().to_string(), row_id.to_string())
        } else {
            // Soft delete: get updated row with _deleted=true
            let deleted_row = UserTableStoreExt::get_include_deleted(
                self.store.as_ref(),
                namespace_id.as_ref(),
                table_name.as_ref(),
                user_id.as_ref(),
                row_id,
            )
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .or_else(|| old_value.clone())
            .ok_or_else(|| KalamDbError::Other("Row not found after soft delete".to_string()))?;

            // Soft delete sends the row with _deleted=true
            ChangeNotification::delete_soft(
                table_name.as_str().to_string(),
                serde_json::to_value(&deleted_row.fields).unwrap_or(serde_json::json!({})),
            )
        };

        // Step 4: Notify subscribers
        let manager = Arc::clone(&self.live_query_manager);
    let table_name_copy = table_name.as_str().to_string();

        tokio::spawn(async move {
            if let Err(_e) = manager
                .notify_table_change(&table_name_copy, notification)
                .await
            {
                #[cfg(debug_assertions)]
                eprintln!("Failed to notify subscribers: {}", _e);
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
        namespace_id: &NamespaceId,
        table_name: &TableName,
        row_id: &str,
        row_data: JsonValue,
    ) -> Result<(), KalamDbError> {
        // Check if row exists
        let old_value = self
            .store
            .get(namespace_id.as_ref(), table_name.as_ref(), row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        let change_type = if old_value.is_some() {
            ChangeType::Update
        } else {
            ChangeType::Insert
        };

        // Store the row
        let shared_row = SharedTableRow {
            row_id: row_id.to_string(),
            fields: row_data,
            _updated: chrono::Utc::now().to_rfc3339(),
            _deleted: false,
            access_level: kalamdb_commons::TableAccess::Public,
        };
        self.store
            .put(namespace_id.as_ref(), table_name.as_ref(), row_id, &shared_row)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Get stored value with system columns
        let new_value = self
            .store
            .get(namespace_id.as_ref(), table_name.as_ref(), row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::Other("Failed to retrieve stored row".to_string()))?;

        // T175: Filter out _deleted=true rows from INSERT/UPDATE notifications
        // Only send DELETE notification for deleted rows
        let is_deleted = new_value._deleted;

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
                ChangeNotification::update(
                    table_name.as_str().to_string(),
                    serde_json::to_value(&old_data.fields).unwrap_or(serde_json::json!({})),
                    serde_json::to_value(&new_value.fields).unwrap_or(serde_json::json!({})),
                )
            }
            ChangeType::Insert => ChangeNotification::insert(
                table_name.as_str().to_string(),
                serde_json::to_value(&new_value.fields).unwrap_or(serde_json::json!({})),
            ),
            _ => {
                return Err(KalamDbError::Other("Invalid change type".to_string()));
            }
        };

        // Notify subscribers
        let manager = Arc::clone(&self.live_query_manager);
    let table_name_copy = table_name.as_str().to_string();

        tokio::spawn(async move {
            if let Err(_e) = manager
                .notify_table_change(&table_name_copy, notification)
                .await
            {
                #[cfg(debug_assertions)]
                eprintln!("Failed to notify subscribers: {}", _e);
            }
        });

        Ok(())
    }

    /// Delete a row with change notification
    pub async fn delete(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        row_id: &str,
        hard: bool,
    ) -> Result<(), KalamDbError> {
        // Get existing row
        let old_value = self
            .store
            .get(namespace_id.as_ref(), table_name.as_ref(), row_id)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Perform deletion
        self.store
            .delete(namespace_id.as_ref(), table_name.as_ref(), row_id, hard)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Create appropriate notification based on delete type
        let notification = if hard {
            // Hard delete: row physically removed, only send row_id
            ChangeNotification::delete_hard(table_name.as_str().to_string(), row_id.to_string())
        } else {
            // Soft delete: get updated row with _deleted=true
            let deleted_row = self
                .store
                .get_include_deleted(namespace_id.as_ref(), table_name.as_ref(), row_id)
                .map_err(|e| KalamDbError::Other(e.to_string()))?
                .unwrap_or_else(|| {
                    old_value.clone().unwrap_or(SharedTableRow {
                        row_id: row_id.to_string(),
                        fields: serde_json::json!({}),
                        _updated: "".to_string(),
                        _deleted: true,
                        access_level: kalamdb_commons::TableAccess::Public,
                    })
                });

            // Soft delete sends the row with _deleted=true
            ChangeNotification::delete_soft(
                table_name.as_str().to_string(),
                serde_json::to_value(deleted_row).unwrap(),
            )
        };

        // Notify subscribers
        let manager = Arc::clone(&self.live_query_manager);
    let table_name_copy = table_name.as_str().to_string();

        tokio::spawn(async move {
            if let Err(_e) = manager
                .notify_table_change(&table_name_copy, notification)
                .await
            {
                #[cfg(debug_assertions)]
                eprintln!("Failed to notify subscribers: {}", _e);
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
    use crate::schema_registry::SchemaRegistry;
    use crate::tables::system::LiveQueriesTableProvider;
    use crate::tables::{new_shared_table_store, new_stream_table_store, new_user_table_store};
    use crate::test_helpers::init_test_app_context;
    use kalamdb_commons::datatypes::KalamDataType;
    use kalamdb_commons::models::TableId;
    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::{NamespaceId, TableName};
    use kalamdb_store::RocksDbInit;
    use tempfile::TempDir;

    async fn create_test_detector() -> (UserTableChangeDetector, TempDir) {
        init_test_app_context();
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::with_defaults(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();

        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(Arc::clone(&db)));

        let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(backend.clone()));
    let schema_registry = Arc::new(SchemaRegistry::new(64, None));

        let default_ns = NamespaceId::new("default");
        let default_table = TableName::new("test");
        let user_table_store = Arc::new(new_user_table_store(
            backend.clone(),
            &default_ns,
            &default_table,
        ));
        let shared_table_store = Arc::new(new_shared_table_store(
            backend.clone(),
            &default_ns,
            &default_table,
        ));
        let stream_table_store = Arc::new(new_stream_table_store(&default_ns, &default_table));

        let messages_table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("messages"),
            TableType::User,
            vec![
                ColumnDefinition::new(
                    "id",
                    1,
                    KalamDataType::Int,
                    false,
                    true,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
                ColumnDefinition::new(
                    "user_id",
                    2,
                    KalamDataType::Text,
                    true,
                    false,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
            ],
            TableOptions::user(),
            None,
        )
        .unwrap();
        let messages_table_id = TableId::new(
            messages_table.namespace_id.clone(),
            messages_table.table_name.clone(),
        );
        schema_registry
            .put_table_definition(&messages_table_id, &messages_table)
            .unwrap();

        let live_query_manager = Arc::new(LiveQueryManager::new(
            live_queries_provider,
            schema_registry,
            NodeId::from("node1"),
            Some(Arc::clone(&user_table_store)),
            Some(Arc::clone(&shared_table_store)),
            Some(Arc::clone(&stream_table_store)),
        ));

        let detector = UserTableChangeDetector::new(Arc::clone(&user_table_store), live_query_manager);

        (detector, temp_dir)
    }

    #[tokio::test]
    async fn test_insert_detection() {
        let (detector, _temp_dir) = create_test_detector().await;

        let namespace_id = NamespaceId::new("default");
        let table_name = TableName::new("messages");
        let user_id = UserId::new("user1");
        let row_id = "msg1";
        let row_data = serde_json::json!({"text": "Hello"});

        detector
            .put(&namespace_id, &table_name, &user_id, row_id, row_data)
            .await
            .unwrap();

        // Verify row exists
        let retrieved = UserTableStoreExt::get(
            detector.store().as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
        )
        .unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_update_detection() {
        let (detector, _temp_dir) = create_test_detector().await;

        let namespace_id = NamespaceId::new("default");
        let table_name = TableName::new("messages");
        let user_id = UserId::new("user1");
        let row_id = "msg1";

        // Insert initial row
        let initial_data = serde_json::json!({"text": "Hello"});
        detector
            .put(&namespace_id, &table_name, &user_id, row_id, initial_data)
            .await
            .unwrap();

        // Update the row
        let updated_data = serde_json::json!({"text": "Hello World"});
        detector
            .put(&namespace_id, &table_name, &user_id, row_id, updated_data)
            .await
            .unwrap();

        // Verify updated value
        let retrieved = UserTableStoreExt::get(
            detector.store().as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
        )
        .unwrap()
        .unwrap();
        assert_eq!(retrieved.fields["text"], "Hello World");
    }

    #[tokio::test]
    async fn test_delete_notification() {
        let (detector, _temp_dir) = create_test_detector().await;

        let namespace_id = NamespaceId::new("default");
        let table_name = TableName::new("messages");
        let user_id = UserId::new("user1");
        let row_id = "msg1";

        // Insert row
        let row_data = serde_json::json!({"text": "Hello"});
        detector
            .put(&namespace_id, &table_name, &user_id, row_id, row_data)
            .await
            .unwrap();

        // Soft delete
        detector
            .delete(&namespace_id, &table_name, &user_id, row_id, false)
            .await
            .unwrap();

        // Verify soft deleted (get returns None)
        let retrieved = UserTableStoreExt::get(
            detector.store().as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
        )
        .unwrap();
        assert!(retrieved.is_none());
    }
}
