//! System table store implementation using EntityStore pattern.
//!
//! This module provides a generic SystemTableStore<K, V> that implements EntityStore<K, V>
//! for all system tables. It provides strongly-typed CRUD operations with automatic
//! JSON serialization and admin-only access control.
//!
//! ## Architecture
//!
//! SystemTableStore sits on top of EntityStore<T> and adds:
//! - Admin-only access control (table_access() returns None)
//! - Consistent partition naming for system tables
//! - Type-safe key/value operations for system entities
//!
//! ## Usage Examples
//!
//! ```rust,ignore
//! use kalamdb_core::stores::SystemTableStore;
//! use kalamdb_commons::system::User;
//! use kalamdb_commons::UserId;
//!
//! // Create a users table store
//! let users_store = SystemTableStore::<UserId, User>::new(backend, "system_users");
//!
//! // Store a user
//! let user = User { /* ... */ };
//! users_store.put(&user_id, &user).unwrap();
//!
//! // Retrieve a user
//! let retrieved = users_store.get(&user_id).unwrap();
//! ```
//!
//! ## System Tables
//!
//! - `SystemTableStore<UserId, User>` - system.users
//! - `SystemTableStore<JobId, Job>` - system.jobs
//! - `SystemTableStore<NamespaceId, Namespace>` - system.namespaces
//! - `SystemTableStore<StorageId, Storage>` - system.storages
//! - `SystemTableStore<LiveQueryId, LiveQuery>` - system.live_queries
//! - `SystemTableStore<String, SystemTable>` - system.tables

use crate::error::KalamDbError;
use crate::tables::shared_tables::shared_table_store::{SharedTableRow, SharedTableRowId};
use crate::tables::stream_tables::stream_table_store::{StreamTableRow, StreamTableRowId};
use crate::tables::system::SystemTableProviderExt;
use crate::tables::user_tables::user_table_store::{UserTableRow, UserTableRowId};
use kalamdb_store::{entity_store::{CrossUserTableStore, EntityStore}, StorageBackend, storage_trait::Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Generic store for system tables with admin-only access control.
///
/// This store provides strongly-typed CRUD operations for system entities.
/// All system tables are admin-only (dba/system roles required).
///
/// ## Type Parameters
/// - `K`: Key type (must implement AsRef<[u8]> + Clone + Send + Sync)
/// - `V`: Value type (must implement Serialize + DeserializeOwned + Send + Sync)
///
/// ## Examples
/// ```rust,ignore
/// // Users table
/// let users_store = SystemTableStore::<UserId, User>::new(backend, "system_users");
///
/// // Jobs table
/// let jobs_store = SystemTableStore::<JobId, Job>::new(backend, "system_jobs");
/// ```
pub struct SystemTableStore<K, V> {
    /// Storage backend (RocksDB, in-memory, etc.)
    backend: Arc<dyn StorageBackend>,
    /// Partition name for this table
    partition: String,
    /// Phantom data to ensure type safety
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> SystemTableStore<K, V> {
    /// Creates a new system table store.
    ///
    /// # Arguments
    /// * `backend` - Storage backend implementation
    /// * `partition` - Partition name (e.g., "system_users", "system_jobs")
    ///
    /// # Returns
    /// A new SystemTableStore instance
    pub fn new(backend: Arc<dyn StorageBackend>, partition: impl Into<String>) -> Self {
        Self {
            backend,
            partition: partition.into(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<K, V> EntityStore<K, V> for SystemTableStore<K, V>
where
    K: AsRef<[u8]> + Clone + Send + Sync,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    fn partition(&self) -> &str {
        &self.partition
    }
}

/// Access control for system tables (admin-only).
///
/// System tables can only be accessed by dba and system roles.
/// This trait implementation ensures all system table stores
/// return None for table_access(), indicating admin-only access.
impl<K: Send + Sync, V: Send + Sync> SystemTableProviderExt for SystemTableStore<K, V> {
    fn table_name(&self) -> &str {
        // Extract table name from partition (remove "system_" prefix)
        self.partition
            .strip_prefix("system_")
            .unwrap_or(&self.partition)
    }

    fn schema_ref(&self) -> arrow::datatypes::SchemaRef {
        // System tables don't have fixed schemas in the traditional sense
        // They use dynamic JSON serialization
        Arc::new(arrow::datatypes::Schema::empty())
    }

    fn load_batch(&self) -> std::result::Result<arrow::record_batch::RecordBatch, KalamDbError> {
        // System tables are accessed via typed EntityStore operations
        // Not via SQL/RecordBatch interface
        Err(KalamDbError::InvalidOperation(
            "System tables do not support RecordBatch loading".to_string(),
        ))
    }
}

impl<K, V> CrossUserTableStore<K, V> for SystemTableStore<K, V>
where
    K: AsRef<[u8]> + Clone + Send + Sync,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn table_access(&self) -> Option<kalamdb_commons::models::TableAccess> {
        // System tables are admin-only (return None)
        None
    }
}

/// Extension trait for user table stores with user-specific operations
pub trait UserTableStoreExt<K, V> {
    /// Scan all rows for a specific user in a table
    fn scan_user(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
    ) -> std::result::Result<Vec<(String, V)>, KalamDbError>;

    /// Get a row (excludes soft-deleted by default)
    fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
    ) -> std::result::Result<Option<V>, KalamDbError>;

    /// Put a row (insert or update)
    fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
        row: &V,
    ) -> std::result::Result<(), KalamDbError>;

    /// Delete a row (soft or hard)
    fn delete(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
        hard: bool,
    ) -> std::result::Result<(), KalamDbError>;

    /// Get a row including soft-deleted ones
    fn get_include_deleted(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
        row_id: &str,
    ) -> std::result::Result<Option<V>, KalamDbError>;

    /// Scan all rows in a user table (including deleted)
    fn scan_all(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<Vec<(String, V)>, KalamDbError>;

    /// Create column family for user table
    fn create_column_family(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<(), KalamDbError>;

    /// Drop user table (delete all data)
    fn drop_table(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<(), KalamDbError>;

    /// Delete all rows for a specific user
    fn delete_all_for_user(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
    ) -> std::result::Result<usize, KalamDbError>;

    /// Scan with iterator for efficient streaming
    fn scan_iter(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError>;

    /// Delete multiple keys in batch
    fn delete_batch_by_keys(
        &self,
        namespace_id: &str,
        table_name: &str,
        keys: &[String],
    ) -> std::result::Result<(), KalamDbError>;
}

impl UserTableStoreExt<UserTableRowId, UserTableRow> for SystemTableStore<UserTableRowId, UserTableRow> {
    fn scan_user(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        user_id: &str,
    ) -> std::result::Result<Vec<(String, UserTableRow)>, KalamDbError> {
        // Scan with prefix "user_id:"
        let prefix_key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), "");
        let results = EntityStore::scan_prefix(self, &prefix_key)?;
        Ok(results.into_iter().map(|(key, row)| {
            (String::from_utf8_lossy(key.as_ref()).to_string(), row)
        }).collect())
    }

    fn get(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        user_id: &str,
        row_id: &str,
    ) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), row_id);
        let row = EntityStore::get(self, &key)?;
        // Filter out soft-deleted rows
        Ok(row.filter(|r| !r._deleted))
    }

    fn put(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        user_id: &str,
        row_id: &str,
        row: &UserTableRow,
    ) -> std::result::Result<(), KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), row_id);
        EntityStore::put(self, &key, row)?;
        Ok(())
    }

    fn delete(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        user_id: &str,
        row_id: &str,
        hard: bool,
    ) -> std::result::Result<(), KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), row_id);
        if hard {
            EntityStore::delete(self, &key)?;
        } else {
            // Soft delete: mark _deleted=true
            let mut row = EntityStore::get(self, &key)?
                .ok_or_else(|| KalamDbError::InvalidOperation("Row not found for soft delete".to_string()))?;
            row._deleted = true;
            EntityStore::put(self, &key, &row)?;
        }
        Ok(())
    }

    fn get_include_deleted(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        user_id: &str,
        row_id: &str,
    ) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), row_id);
        Ok(EntityStore::get(self, &key)?)
    }

    fn scan_all(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<Vec<(String, UserTableRow)>, KalamDbError> {
        let results = EntityStore::scan_all(self)?;
        Ok(results.into_iter().map(|(key, row)| {
            (String::from_utf8_lossy(key.as_ref()).to_string(), row)
        }).collect())
    }

    fn create_column_family(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<(), KalamDbError> {
        // Column family creation is handled by the backend
        Ok(())
    }

    fn drop_table(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<(), KalamDbError> {
        // For now, just scan and delete all - in practice this should use backend drop
        let all_results = EntityStore::scan_all(self)?;
        for (key_bytes, _) in all_results {
            let key_str = String::from_utf8_lossy(key.as_ref()).to_string();
            let key = UserTableRowId::from_key_string(key_str);
            EntityStore::delete(self, &key)?;
        }
        Ok(())
    }

    fn delete_all_for_user(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
    ) -> std::result::Result<usize, KalamDbError> {
        let user_rows = self.scan_user(namespace_id, table_name, user_id)?;
        let mut deleted_count = 0;
        for (key_str, _) in user_rows {
            let key = UserTableRowId::from_key_string(key_str);
            EntityStore::delete(self, &key)?;
            deleted_count += 1;
        }
        Ok(deleted_count)
    }

    fn scan_iter(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        // For now, just collect all and return as iterator
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| {
            let value_bytes = serde_json::to_vec(&v)
                .map_err(|e| KalamDbError::SerializationError(e.to_string()))?;
            Ok((k, value_bytes))
        });
        Ok(Box::new(iter))
    }

    fn delete_batch_by_keys(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        keys: &[String],
    ) -> std::result::Result<(), KalamDbError> {
        for key_str in keys {
            let key = UserTableRowId::from_key_string(key_str.clone());
            EntityStore::delete(self, &key)?;
        }
        Ok(())
    }
}

/// Extension trait for shared table stores with shared table operations
pub trait SharedTableStoreExt<K, V> {
    /// Scan all rows in a shared table
    fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<Vec<(String, V)>, KalamDbError>;

    /// Get a row by ID
    fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
    ) -> std::result::Result<Option<V>, KalamDbError>;

    /// Put a row
    fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        row: &V,
    ) -> std::result::Result<(), KalamDbError>;

    /// Delete a row
    fn delete(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        hard: bool,
    ) -> std::result::Result<(), KalamDbError>;

    /// Get a row including soft-deleted ones
    fn get_include_deleted(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
    ) -> std::result::Result<Option<V>, KalamDbError>;

    /// Create column family for shared table
    fn create_column_family(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<(), KalamDbError>;

    /// Drop shared table (delete all data)
    fn drop_table(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<(), KalamDbError>;

    /// Scan with iterator for efficient streaming
    fn scan_iter(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError>;

    /// Delete multiple keys in batch
    fn delete_batch_by_keys(
        &self,
        namespace_id: &str,
        table_name: &str,
        keys: &[String],
    ) -> std::result::Result<(), KalamDbError>;
}

impl SharedTableStoreExt<UserTableRowId, UserTableRow> for SystemTableStore<UserTableRowId, UserTableRow> {
    fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<Vec<(String, UserTableRow)>, KalamDbError> {
        // Scan all rows for the user in the user table
        UserTableStoreExt::scan_all(self, namespace_id, table_name)
    }

    fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
    ) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        // Get a specific row by ID, including soft-deleted ones
        UserTableStoreExt::get_include_deleted(self, namespace_id, table_name, "", row_id)
    }

    fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        row: &UserTableRow,
    ) -> std::result::Result<(), KalamDbError> {
        // Put a row (insert or update)
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(namespace_id), row_id);
        EntityStore::put(self, &key, row)?;
        Ok(())
    }

    fn delete(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
        hard: bool,
    ) -> std::result::Result<(), KalamDbError> {
        // Delete a row by ID (soft or hard delete)
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(namespace_id), row_id);
        if hard {
            EntityStore::delete(self, &key)?;
        } else {
            // Soft delete: just remove from the main table, keep in the history
            let row = EntityStore::get(self, &key)?;
            if let Some(existing_row) = row {
                // Insert into history table (not shown here)
                // self.insert_history(existing_row)?;
                // Now delete from main table
                EntityStore::delete(self, &key)?;
            }
        }
        Ok(())
    }

    fn get_include_deleted(
        &self,
        namespace_id: &str,
        table_name: &str,
        row_id: &str,
    ) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        // Include deleted rows in the result
        UserTableStoreExt::get_include_deleted(self, namespace_id, table_name, "", row_id)
    }

    fn create_column_family(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<(), KalamDbError> {
        // Create column family for user table
        UserTableStoreExt::create_column_family(self, namespace_id, table_name)
    }

    fn drop_table(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<(), KalamDbError> {
        // Drop the user table
        UserTableStoreExt::drop_table(self, namespace_id, table_name)
    }

    fn scan_iter(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        // Scan with iterator for efficient streaming
        UserTableStoreExt::scan_iter(self, namespace_id, table_name)
    }

    fn delete_batch_by_keys(
        &self,
        namespace_id: &str,
        table_name: &str,
        keys: &[String],
    ) -> std::result::Result<(), KalamDbError> {
        // Delete multiple keys in batch
        UserTableStoreExt::delete_batch_by_keys(self, namespace_id, table_name, keys)
    }
}

impl SharedTableStoreExt<SharedTableRowId, SharedTableRow> for SystemTableStore<SharedTableRowId, SharedTableRow> {
    fn scan(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<Vec<(String, SharedTableRow)>, KalamDbError> {
        let results = EntityStore::scan_all(self)?;
        Ok(results.into_iter().map(|(key, row)| {
            (String::from_utf8_lossy(key.as_ref()).to_string(), row)
        }).collect())
    }

    fn get(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        row_id: &str,
    ) -> std::result::Result<Option<SharedTableRow>, KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        Ok(EntityStore::get(self, &key)?)
    }

    fn put(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        row_id: &str,
        row: &SharedTableRow,
    ) -> std::result::Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        EntityStore::put(self, &key, row)?;
        Ok(())
    }

    fn delete(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        row_id: &str,
        _hard: bool,
    ) -> std::result::Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        EntityStore::delete(self, &key)?;
        Ok(())
    }

    fn get_include_deleted(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        row_id: &str,
    ) -> std::result::Result<Option<SharedTableRow>, KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        Ok(EntityStore::get(self, &key)?)
    }

    fn create_column_family(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<(), KalamDbError> {
        // Column family creation is handled by the backend
        Ok(())
    }

    fn drop_table(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<(), KalamDbError> {
        // For now, just scan and delete all - in practice this should use backend drop
        let all_results = EntityStore::scan_all(self)?;
        for (key_bytes, _) in all_results {
            let key_str = String::from_utf8_lossy(key.as_ref()).to_string();
            let key = SharedTableRowId::new(&key_str);
            EntityStore::delete(self, &key)?;
        }
        Ok(())
    }

    fn scan_iter(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        // For now, just collect all and return as iterator
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| {
            let value_bytes = serde_json::to_vec(&v)
                .map_err(|e| KalamDbError::SerializationError(e.to_string()))?;
            Ok((k, value_bytes))
        });
        Ok(Box::new(iter))
    }

    fn delete_batch_by_keys(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        keys: &[String],
    ) -> std::result::Result<(), KalamDbError> {
        for key_str in keys {
            let key = SharedTableRowId::new(key_str);
            EntityStore::delete(self, &key)?;
        }
        Ok(())
    }
}

impl SharedTableStoreExt<StreamTableRowId, StreamTableRow>
    for SystemTableStore<StreamTableRowId, StreamTableRow>
{
    fn scan(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<Vec<(String, StreamTableRow)>, KalamDbError> {
        let results = EntityStore::scan_all(self)?;
        Ok(results
            .into_iter()
            .map(|(key, row)| (String::from_utf8_lossy(key.as_ref()).to_string(), row))
            .collect())
    }

    fn get(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        row_id: &str,
    ) -> std::result::Result<Option<StreamTableRow>, KalamDbError> {
        let key = StreamTableRowId::new(row_id);
        Ok(EntityStore::get(self, &key)?)
    }

    fn put(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        row_id: &str,
        row: &StreamTableRow,
    ) -> std::result::Result<(), KalamDbError> {
        let key = StreamTableRowId::new(row_id);
        EntityStore::put(self, &key, row)?;
        Ok(())
    }

    fn delete(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        row_id: &str,
        _hard: bool,
    ) -> std::result::Result<(), KalamDbError> {
        let key = StreamTableRowId::new(row_id);
        EntityStore::delete(self, &key)?;
        Ok(())
    }

    fn get_include_deleted(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        row_id: &str,
    ) -> std::result::Result<Option<StreamTableRow>, KalamDbError> {
        let key = StreamTableRowId::new(row_id);
        Ok(EntityStore::get(self, &key)?)
    }

    fn create_column_family(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<(), KalamDbError> {
        // Column family creation is handled by the backend
        Ok(())
    }

    fn drop_table(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<(), KalamDbError> {
        // For now, just scan and delete all - in practice this should use backend drop
        let all_results = EntityStore::scan_all(self)?;
        for (key_id, _) in all_results {
            EntityStore::delete(self, &key_id)?;
        }
        Ok(())
    }

    fn scan_iter(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<
        Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>,
        KalamDbError,
    > {
        // For now, just collect all and return as iterator
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| {
            let value_bytes =
                serde_json::to_vec(&v).map_err(|e| KalamDbError::SerializationError(e.to_string()))?;
            Ok((k, value_bytes))
        });
        Ok(Box::new(iter))
    }

    fn delete_batch_by_keys(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        keys: &[String],
    ) -> std::result::Result<(), KalamDbError> {
        for key_str in keys {
            let key = StreamTableRowId::new(key_str);
            EntityStore::delete(self, &key)?;
        }
        Ok(())
    }
}

// NOTE: cleanup_expired_rows is NOT part of SharedTableStoreExt trait
// It's a separate method that can be called directly on StreamTableStore
impl SystemTableStore<StreamTableRowId, StreamTableRow> {
    pub fn cleanup_expired_rows(
        &self,
        _namespace_id: &str,
        _table_name: &str,
    ) -> std::result::Result<usize, KalamDbError> {
        let all_rows = EntityStore::scan_all(self)?;
        let mut deleted_count = 0;
        for (key, row) in all_rows {
            if let Some(ttl) = row.ttl_seconds {
                let inserted_at = chrono::DateTime::parse_from_rfc3339(&row.inserted_at)
                    .map_err(|e| KalamDbError::Other(e.to_string()))?;
                if chrono::Utc::now().timestamp() > inserted_at.timestamp() + ttl as i64 {
                    EntityStore::delete(self, &key)?;
                    deleted_count += 1;
                }
            }
        }
        Ok(deleted_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct TestEntity {
        id: String,
        name: String,
        value: i32,
    }

    #[test]
    fn test_system_table_store_put_get() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<String, TestEntity>::new(backend, "test_system");

        let entity = TestEntity {
            id: "e1".to_string(),
            name: "Test Entity".to_string(),
            value: 42,
        };

        // Put entity
        store.put("e1", &entity).unwrap();

        // Get entity
        let retrieved = store.get("e1").unwrap().unwrap();
        assert_eq!(retrieved, entity);

        // Get non-existent entity
        let missing = store.get("e999").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_system_table_store_delete() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<String, TestEntity>::new(backend, "test_system");

        let entity = TestEntity {
            id: "e1".to_string(),
            name: "Test Entity".to_string(),
            value: 42,
        };

        let key = "e1".to_string();
        store.put(&key, &entity).unwrap();
        assert!(store.get(&key).unwrap().is_some());

        store.delete(&key).unwrap();
        assert!(store.get(&key).unwrap().is_none());

        // Idempotent delete
        store.delete(&key).unwrap();
    }

    #[test]
    fn test_system_table_store_scan_all() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<String, TestEntity>::new(backend, "test_system");

        let entities = vec![
            TestEntity {
                id: "e1".to_string(),
                name: "Entity 1".to_string(),
                value: 10,
            },
            TestEntity {
                id: "e2".to_string(),
                name: "Entity 2".to_string(),
                value: 20,
            },
        ];

        for entity in &entities {
            let key = entity.id.clone();
            store.put(&key, entity).unwrap();
        }

        let all = store.scan_all().unwrap();
        assert_eq!(all.len(), 2);

        // Verify entities are returned
        let keys: std::collections::HashSet<_> = all.iter().map(|(k, _)| k.clone()).collect();
        assert!(keys.contains(&"e1".as_bytes().to_vec()));
        assert!(keys.contains(&"e2".as_bytes().to_vec()));
    }

    #[test]
    fn test_system_table_provider_ext() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<String, TestEntity>::new(backend, "system_test_table");

        // Test table name extraction
        assert_eq!(store.table_name(), "test_table");

        // Test schema (empty for system tables)
        let schema = store.schema_ref();
        assert_eq!(schema.fields().len(), 0);

        // Test load_batch (should fail for system tables)
        let result = store.load_batch();
        assert!(result.is_err());
    }
}