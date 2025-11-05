//! Namespaces table store implementation
//!
//! This module provides a SystemTableStore<NamespaceId, Namespace> wrapper for the system.namespaces table.

use crate::tables::system::system_table_store::SystemTableStore;
use kalamdb_commons::system::Namespace;
use kalamdb_commons::NamespaceId;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Type alias for the namespaces table store
pub type NamespacesStore = SystemTableStore<NamespaceId, Namespace>;

/// Helper function to create a new namespaces table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the namespaces table
pub fn new_namespaces_store(backend: Arc<dyn StorageBackend>) -> NamespacesStore {
    SystemTableStore::new(backend, "system_namespaces")
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::Role;
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_store::CrossUserTableStore;
    use kalamdb_store::EntityStoreV2 as EntityStore;

    fn create_test_store() -> NamespacesStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_namespaces_store(backend)
    }

    fn create_test_namespace(namespace_id: &str, name: &str) -> Namespace {
        Namespace {
            namespace_id: NamespaceId::new(namespace_id),
            name: name.to_string(),
            created_at: 1000,
            options: Some("{}".to_string()),
            table_count: 0,
        }
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert_eq!(store.partition(), "system_namespaces");
    }

    #[test]
    fn test_put_and_get_namespace() {
        let store = create_test_store();
        let namespace_id = NamespaceId::new("app");
        let namespace = create_test_namespace("app", "app");

        // Put namespace
        EntityStore::put(&store, &namespace_id, &namespace).unwrap();

        // Get namespace
        let retrieved = EntityStore::get(&store, &namespace_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.namespace_id, namespace_id);
        assert_eq!(retrieved.name, "app");
    }

    #[test]
    fn test_delete_namespace() {
        let store = create_test_store();
        let namespace_id = NamespaceId::new("app");
        let namespace = create_test_namespace("app", "app");

        // Put then delete
        EntityStore::put(&store, &namespace_id, &namespace).unwrap();
        EntityStore::delete(&store, &namespace_id).unwrap();

        // Verify deleted
        let retrieved = EntityStore::get(&store, &namespace_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_namespaces() {
        let store = create_test_store();

        // Insert multiple namespaces
        for i in 1..=3 {
            let namespace_id = NamespaceId::new(&format!("ns{}", i));
            let namespace = create_test_namespace(&format!("ns{}", i), &format!("namespace{}", i));
            EntityStore::put(&store, &namespace_id, &namespace).unwrap();
        }

        // Scan all
        let namespaces = EntityStore::scan_all(&store).unwrap();
        assert_eq!(namespaces.len(), 3);
    }

    #[test]
    fn test_admin_only_access() {
        let store = create_test_store();

        // System tables return None for table_access (admin-only)
        assert!(store.table_access().is_none());

        // Only Service, Dba, System roles can read
        assert!(!store.can_read(&Role::User));
        assert!(store.can_read(&Role::Service));
        assert!(store.can_read(&Role::Dba));
        assert!(store.can_read(&Role::System));
    }
}
