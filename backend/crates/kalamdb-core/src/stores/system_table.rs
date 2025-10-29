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

use crate::tables::system::SystemTableProviderExt;
use kalamdb_store::{EntityStore, StorageBackend, storage_trait::Result};
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

impl<K, V> EntityStore<V> for SystemTableStore<K, V>
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
impl<K, V> SystemTableProviderExt for SystemTableStore<K, V> {
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

    fn load_batch(&self) -> Result<arrow::record_batch::RecordBatch> {
        // System tables are accessed via typed EntityStore operations
        // Not via SQL/RecordBatch interface
        Err(kalamdb_store::storage_trait::StorageError::NotSupported(
            "System tables do not support RecordBatch loading".to_string(),
        ))
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

        store.put("e1", &entity).unwrap();
        assert!(store.get("e1").unwrap().is_some());

        store.delete("e1").unwrap();
        assert!(store.get("e1").unwrap().is_none());

        // Idempotent delete
        store.delete("e1").unwrap();
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
            store.put(&entity.id, entity).unwrap();
        }

        let all = store.scan_all().unwrap();
        assert_eq!(all.len(), 2);

        // Verify entities are returned
        let ids: std::collections::HashSet<_> = all.iter().map(|(k, _)| k.clone()).collect();
        assert!(ids.contains("e1"));
        assert!(ids.contains("e2"));
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