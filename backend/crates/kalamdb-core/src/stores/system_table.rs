//! Generic system table store implementation.
//!
//! This module provides `SystemTableStore<K, V>`, a generic implementation of
//! `EntityStore<K, V>` and `CrossUserTableStore<K, V>` for all system tables.
//!
//! ## Type Safety
//!
//! The store is generic over both key type (K) and value type (V):
//! - **K**: Type-safe key (UserId, TableId, JobId, etc.)
//! - **V**: Entity type (User, Job, Namespace, etc.)
//!
//! ## Usage Examples
//!
//! ```rust,ignore
//! use kalamdb_core::stores::SystemTableStore;
//! use kalamdb_commons::models::{UserId, User};
//! use kalamdb_store::StorageBackend;
//! use std::sync::Arc;
//!
//! // Create a store for system.users
//! let store = SystemTableStore::<UserId, User>::new(
//!     backend,
//!     "system_users"
//! );
//!
//! // Type-safe operations
//! let user_id = UserId::new("u1");
//! let user = User { id: user_id.clone(), ... };
//! store.put(&user_id, &user)?;
//! let retrieved = store.get(&user_id)?;
//! ```
//!
//! ## System Tables Using This Store
//!
//! - `system.users`: `SystemTableStore<UserId, User>`
//! - `system.tables`: `SystemTableStore<TableId, TableMetadata>`
//! - `system.jobs`: `SystemTableStore<JobId, Job>`
//! - `system.namespaces`: `SystemTableStore<NamespaceId, Namespace>`
//! - `system.storages`: `SystemTableStore<StorageId, Storage>`
//! - `system.live_queries`: `SystemTableStore<LiveQueryId, LiveQuery>`

use kalamdb_commons::models::TableAccess;
use kalamdb_store::{CrossUserTableStore, EntityStoreV2, StorageBackend};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Generic system table store implementation.
///
/// This struct provides a reusable implementation of `EntityStore<K, V>` and
/// `CrossUserTableStore<K, V>` for all system tables. System tables are:
/// - **Admin-only**: Only dba/system roles can access
/// - **Cross-user**: Not isolated to specific users
/// - **Metadata storage**: Store system configuration and state
///
/// ## Type Parameters
///
/// - `K`: Key type that implements `AsRef<[u8]> + Clone + Send + Sync`
///   - Examples: `UserId`, `TableId`, `JobId`, `NamespaceId`, `StorageId`, `LiveQueryId`
/// - `V`: Value/entity type that implements `Serialize + Deserialize + Send + Sync`
///   - Examples: `User`, `Job`, `Namespace`, `Storage`, `LiveQuery`
///
/// ## Access Control
///
/// All system tables return `None` from `table_access()`, indicating admin-only access.
/// Only users with `Role::Dba` or `Role::System` can read/write system tables.
///
/// ## Example: Users Table
///
/// ```rust,ignore
/// use kalamdb_commons::models::{UserId, User};
///
/// let users_store = SystemTableStore::<UserId, User>::new(
///     backend,
///     "system_users"
/// );
///
/// let user_id = UserId::new("admin");
/// let user = User {
///     id: user_id.clone(),
///     username: "admin".into(),
///     role: Role::Dba,
///     ...
/// };
///
/// users_store.put(&user_id, &user)?;
/// ```
pub struct SystemTableStore<K, V>
where
    K: AsRef<[u8]> + Clone + Send + Sync,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    /// Storage backend for persistence
    backend: Arc<dyn StorageBackend>,

    /// Partition name (e.g., "system_users", "system_jobs")
    partition: String,

    /// Phantom data for key and value types
    _marker: std::marker::PhantomData<(K, V)>,
}

impl<K, V> SystemTableStore<K, V>
where
    K: AsRef<[u8]> + Clone + Send + Sync,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    /// Creates a new system table store.
    ///
    /// ## Arguments
    ///
    /// - `backend`: Storage backend (typically RocksDB)
    /// - `partition`: Partition name (e.g., "system_users", "system_jobs")
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// use kalamdb_commons::models::{JobId, Job};
    ///
    /// let jobs_store = SystemTableStore::<JobId, Job>::new(
    ///     backend,
    ///     "system_jobs"
    /// );
    /// ```
    pub fn new(backend: Arc<dyn StorageBackend>, partition: impl Into<String>) -> Self {
        Self {
            backend,
            partition: partition.into(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns a reference to the partition name.
    pub fn partition_name(&self) -> &str {
        &self.partition
    }

    /// Returns a reference to the storage backend.
    pub fn storage_backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }
}

impl<K, V> EntityStoreV2<K, V> for SystemTableStore<K, V>
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

impl<K, V> CrossUserTableStore<K, V> for SystemTableStore<K, V>
where
    K: AsRef<[u8]> + Clone + Send + Sync,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    /// Returns `None` for all system tables, indicating admin-only access.
    ///
    /// System tables can only be accessed by users with `Role::Dba` or `Role::System`.
    fn table_access(&self) -> Option<TableAccess> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{Role, UserId};
    use kalamdb_store::test_utils::InMemoryBackend;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestEntity {
        id: String,
        name: String,
    }

    #[test]
    fn test_system_table_store_new() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<UserId, TestEntity>::new(backend, "test_partition");

        assert_eq!(store.partition_name(), "test_partition");
    }

    #[test]
    fn test_system_table_store_put_get() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<UserId, TestEntity>::new(backend, "test_partition");

        let key = UserId::new("test_key");
        let entity = TestEntity {
            id: "test_key".to_string(),
            name: "Test Entity".to_string(),
        };

        // Put entity
        store.put(&key, &entity).unwrap();

        // Get entity
        let retrieved = store.get(&key).unwrap();
        assert_eq!(retrieved, Some(entity));
    }

    #[test]
    fn test_system_table_store_delete() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<UserId, TestEntity>::new(backend, "test_partition");

        let key = UserId::new("test_key");
        let entity = TestEntity {
            id: "test_key".to_string(),
            name: "Test Entity".to_string(),
        };

        // Put and verify
        store.put(&key, &entity).unwrap();
        assert!(store.get(&key).unwrap().is_some());

        // Delete and verify
        store.delete(&key).unwrap();
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_system_table_store_scan_all() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<UserId, TestEntity>::new(backend, "test_partition");

        // Insert multiple entities
        for i in 0..5 {
            let key = UserId::new(format!("key_{}", i));
            let entity = TestEntity {
                id: format!("key_{}", i),
                name: format!("Entity {}", i),
            };
            store.put(&key, &entity).unwrap();
        }

        // Scan all
        let results = store.scan_all().unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_system_table_store_access_control() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<UserId, TestEntity>::new(backend, "test_partition");

        // System tables return None for table_access
        assert_eq!(store.table_access(), None);

        // Only admin roles can read
        assert!(!store.can_read(&Role::User));
        assert!(!store.can_read(&Role::Service));
        assert!(store.can_read(&Role::Dba));
        assert!(store.can_read(&Role::System));
    }

    #[test]
    fn test_system_table_store_type_safety() {
        // This test verifies compile-time type safety
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<UserId, TestEntity>::new(backend, "test_partition");

        let user_id = UserId::new("test");
        let entity = TestEntity {
            id: "test".to_string(),
            name: "Test".to_string(),
        };

        // Type-safe operations
        store.put(&user_id, &entity).unwrap();
        let _retrieved: Option<TestEntity> = store.get(&user_id).unwrap();

        // The following would fail to compile:
        // let table_id = TableId::new(...);
        // store.put(&table_id, &entity); // ✗ Compile error - wrong key type
    }

    #[test]
    fn test_system_table_store_get_nonexistent() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<UserId, TestEntity>::new(backend, "test_partition");

        let key = UserId::new("nonexistent");
        let result = store.get(&key).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_system_table_store_update() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<UserId, TestEntity>::new(backend, "test_partition");

        let key = UserId::new("test_key");
        let entity1 = TestEntity {
            id: "test_key".to_string(),
            name: "Original".to_string(),
        };
        let entity2 = TestEntity {
            id: "test_key".to_string(),
            name: "Updated".to_string(),
        };

        // Initial put
        store.put(&key, &entity1).unwrap();
        assert_eq!(store.get(&key).unwrap().unwrap().name, "Original");

        // Update
        store.put(&key, &entity2).unwrap();
        assert_eq!(store.get(&key).unwrap().unwrap().name, "Updated");
    }
}
