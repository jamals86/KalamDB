//! Namespaces table store implementation
//!
//! This module provides a SystemTableStore<NamespaceId, Namespace> wrapper for the system.namespaces table.

use crate::system_table_store::SystemTableStore;
use crate::SystemTable;
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
    SystemTableStore::new(backend, SystemTable::Namespaces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SystemTable;
    use kalamdb_commons::Role;
    use kalamdb_store::entity_store::EntityStore;
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_store::CrossUserTableStore;

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
        assert_eq!(
            store.partition(),
            SystemTable::Namespaces
                .column_family_name()
                .expect("Namespaces is a table, not a view")
                .into()
        );
    }

    #[test]
    fn test_put_and_get_namespace() {
        let store = create_test_store();
        let namespace_id = NamespaceId::new("app");
        let namespace = create_test_namespace("app", "app");

        // Put namespace
        store.put(&namespace_id, &namespace).unwrap();

        // Get namespace
        let retrieved = store.get(&namespace_id).unwrap();
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
        store.put(&namespace_id, &namespace).unwrap();
        store.delete(&namespace_id).unwrap();

        // Verify deleted
        let retrieved = store.get(&namespace_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_namespaces() {
        let store = create_test_store();

        // Insert multiple namespaces
        for i in 1..=3 {
            let namespace_id = NamespaceId::new(format!("ns{}", i));
            let namespace = create_test_namespace(&format!("ns{}", i), &format!("namespace{}", i));
            store.put(&namespace_id, &namespace).unwrap();
        }

        // Scan all
        let namespaces = store.scan_all(None, None, None).unwrap();
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
