//! Storages table store implementation
//!
//! This module provides a SystemTableStore<StorageId, Storage> wrapper for the system.storages table.

use crate::stores::SystemTableStore;
use kalamdb_store::StorageBackend;
use kalamdb_commons::system::Storage;
use kalamdb_commons::StorageId;
use std::sync::Arc;

/// Type alias for the storages table store
pub type StoragesStore = SystemTableStore<StorageId, Storage>;

/// Helper function to create a new storages table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the storages table
pub fn new_storages_store(backend: Arc<dyn StorageBackend>) -> StoragesStore {
    SystemTableStore::new(backend, "system_storages")
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::Role;
    use kalamdb_store::InMemoryBackend;

    fn create_test_store() -> StoragesStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_storages_store(backend)
    }

    fn create_test_storage(storage_id: &str, name: &str) -> Storage {
        Storage {
            storage_id: StorageId::new(storage_id),
            storage_name: name.to_string(),
            description: Some("Test storage".to_string()),
            storage_type: "filesystem".to_string(),
            base_directory: "/data".to_string(),
            credentials: None,
            shared_tables_template: "{base}/shared/{namespace}/{table}".to_string(),
            user_tables_template: "{base}/user/{namespace}/{table}/{user_id}".to_string(),
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert_eq!(store.partition(), "system_storages");
    }

    #[test]
    fn test_put_and_get_storage() {
        let store = create_test_store();
        let storage_id = StorageId::new("local");
        let storage = create_test_storage("local", "Local Storage");

        // Put storage
        store.put(&storage_id, &storage).unwrap();

        // Get storage
        let retrieved = store.get(&storage_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.storage_id, storage_id);
        assert_eq!(retrieved.storage_name, "Local Storage");
    }

    #[test]
    fn test_delete_storage() {
        let store = create_test_store();
        let storage_id = StorageId::new("local");
        let storage = create_test_storage("local", "Local Storage");

        // Put then delete
        store.put(&storage_id, &storage).unwrap();
        store.delete(&storage_id).unwrap();

        // Verify deleted
        let retrieved = store.get(&storage_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_storages() {
        let store = create_test_store();

        // Insert multiple storages
        for i in 1..=3 {
            let storage_id = StorageId::new(&format!("storage{}", i));
            let storage = create_test_storage(&format!("storage{}", i), &format!("Storage {}", i));
            store.put(&storage_id, &storage).unwrap();
        }

        // Scan all
        let storages = store.scan_all().unwrap();
        assert_eq!(storages.len(), 3);
    }

    #[test]
    fn test_admin_only_access() {
        let store = create_test_store();

        // System tables return None for table_access (admin-only)
        assert!(store.table_access().is_none());

        // Only Service, Dba, System roles can read
        assert!(!store.can_read(Role::User));
        assert!(store.can_read(Role::Service));
        assert!(store.can_read(Role::Dba));
        assert!(store.can_read(Role::System));
    }
}
