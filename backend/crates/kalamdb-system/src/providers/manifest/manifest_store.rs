//! System.manifest table store
//!
//! Provides typed storage for manifest cache entries using SystemTableStore.
//! This is a read-only view of the manifest cache managed by ManifestCacheService.

use crate::system_table_store::SystemTableStore;
use kalamdb_commons::types::ManifestCacheEntry;
use kalamdb_commons::SystemTable;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Cache key type (namespace:table:scope format)
///
/// Uses the same key format as ManifestCacheService for consistency.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ManifestCacheKey(String);

impl ManifestCacheKey {
    /// Get the key as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse cache key into components (namespace, table, scope)
    pub fn parse(&self) -> Option<(String, String, String)> {
        let parts: Vec<&str> = self.0.split(':').collect();
        if parts.len() == 3 {
            Some((
                parts[0].to_string(),
                parts[1].to_string(),
                parts[2].to_string(),
            ))
        } else {
            None
        }
    }
}

impl From<String> for ManifestCacheKey {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ManifestCacheKey {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl kalamdb_commons::StorageKey for ManifestCacheKey {
    fn storage_key(&self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }

    fn from_storage_key(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec())
            .map(ManifestCacheKey)
            .map_err(|e| e.to_string())
    }
}

/// Type alias for the manifest store
pub type ManifestStore = SystemTableStore<ManifestCacheKey, ManifestCacheEntry>;

/// Create a new manifest store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore for manifest cache entries
pub fn new_manifest_store(backend: Arc<dyn StorageBackend>) -> ManifestStore {
    SystemTableStore::new(backend, SystemTable::Manifest.column_family_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::types::SyncState;
    use kalamdb_store::entity_store::EntityStore;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_store() -> ManifestStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_manifest_store(backend)
    }

    fn create_test_entry() -> ManifestCacheEntry {
        ManifestCacheEntry::new(
            r#"{"table_id":"test","scope":"shared","version":1}"#.to_string(),
            Some("etag123".to_string()),
            1000,
            "s3://bucket/ns1/tbl1/manifest.json".to_string(),
            SyncState::InSync,
        )
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert!(EntityStore::scan_all(&store, None, None, None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_cache_key_parsing() {
        let key = ManifestCacheKey::from("ns1:tbl1:shared");
        let (namespace, table, scope) = key.parse().unwrap();
        assert_eq!(namespace, "ns1");
        assert_eq!(table, "tbl1");
        assert_eq!(scope, "shared");
    }

    #[test]
    fn test_put_and_get_entry() {
        let store = create_test_store();
        let key = ManifestCacheKey::from("ns1:tbl1:shared");
        let entry = create_test_entry();

        EntityStore::put(&store, &key, &entry).unwrap();

        let retrieved = EntityStore::get(&store, &key).unwrap();
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
            let key = ManifestCacheKey::from(format!("ns{}:tbl{}:shared", i, i));
            EntityStore::put(&store, &key, &entry).unwrap();
        }

        let all = EntityStore::scan_all(&store, None, None, None).unwrap();
        assert_eq!(all.len(), 3);
    }
}
