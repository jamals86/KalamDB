//! System.manifest table store
//!
//! Provides typed storage for manifest cache entries using SystemTableStore.
//! This is a read-only view of the manifest cache managed by ManifestService.

use crate::system_table_store::SystemTableStore;
use crate::SystemTable;
use crate::providers::manifest::ManifestCacheEntry;
use kalamdb_commons::ManifestId;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Type alias for the manifest store
pub type ManifestStore = SystemTableStore<ManifestId, ManifestCacheEntry>;

/// Create a new manifest store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore for manifest cache entries
pub fn new_manifest_store(backend: Arc<dyn StorageBackend>) -> ManifestStore {
    SystemTableStore::new(backend, SystemTable::Manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::manifest::{Manifest, SyncState};
    use kalamdb_commons::{NamespaceId, TableId, TableName};
    use kalamdb_store::entity_store::EntityStore;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_store() -> ManifestStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_manifest_store(backend)
    }

    fn create_test_entry() -> ManifestCacheEntry {
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let manifest = Manifest::new(table_id, None);
        ManifestCacheEntry::new(
            manifest,
            Some("etag123".to_string()),
            1000,
            SyncState::InSync,
        )
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert!(store.scan_all_typed(None, None, None).unwrap().is_empty());
    }

    #[test]
    fn test_cache_key_parsing() {
        let key = ManifestId::from("ns1:tbl1:shared");
        assert_eq!(key.table_id().namespace_id().as_str(), "ns1");
        assert_eq!(key.table_id().table_name().as_str(), "tbl1");
        assert_eq!(key.scope_str(), "shared");
    }

    #[test]
    fn test_put_and_get_entry() {
        let store = create_test_store();
        let key = ManifestId::from("ns1:tbl1:shared");
        let entry = create_test_entry();

        store.put(&key, &entry).unwrap();

        let retrieved = store.get(&key).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.etag, Some("etag123".to_string()));
        assert_eq!(retrieved.sync_state, SyncState::InSync);
    }

    #[test]
    fn test_scan_all_entries() {
        let store = create_test_store();
        let entry = create_test_entry();

        // Insert multiple entries
        for i in 1..=3 {
            let key = ManifestId::from(format!("ns{}:tbl{}:shared", i, i));
            store.put(&key, &entry).unwrap();
        }

        let all = store.scan_all_typed(None, None, None).unwrap();
        assert_eq!(all.len(), 3);
    }
}
