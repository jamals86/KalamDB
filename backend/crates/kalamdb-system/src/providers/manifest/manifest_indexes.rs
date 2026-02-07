//! Manifest table index definitions
//!
//! This module defines secondary indexes for the system.manifest table.

use crate::providers::manifest::{ManifestCacheEntry, SyncState};
use crate::StoragePartition;
use kalamdb_commons::storage::Partition;
use kalamdb_commons::{ManifestId, StorageKey};
use kalamdb_store::IndexDefinition;
use std::sync::Arc;

/// Index for querying manifests by PendingWrite state.
///
/// Key format: `{manifest_id_bytes}` (just the manifest ID)
/// Value: Empty (the index key IS the reference to the manifest)
///
/// This index allows efficient discovery of manifests that need flushing:
/// - O(1) lookup instead of O(N) scan of all manifests
/// - Flush jobs can iterate over this index to find pending tables
///
/// The index only contains manifests with sync_state == PendingWrite.
pub struct ManifestPendingWriteIndex;

impl IndexDefinition<ManifestId, ManifestCacheEntry> for ManifestPendingWriteIndex {
    fn partition(&self) -> Partition {
        Partition::new(StoragePartition::ManifestPendingWriteIdx.name())
    }

    fn indexed_columns(&self) -> Vec<&str> {
        vec!["sync_state"]
    }

    fn extract_key(&self, primary_key: &ManifestId, entry: &ManifestCacheEntry) -> Option<Vec<u8>> {
        // Only index entries with PendingWrite state
        if entry.sync_state == SyncState::PendingWrite {
            // The key IS the manifest ID (the reference itself)
            Some(primary_key.storage_key())
        } else {
            None
        }
    }

    fn filter_to_prefix(&self, _filter: &datafusion::logical_expr::Expr) -> Option<Vec<u8>> {
        // No prefix filtering needed - we want all pending writes
        None
    }
}

/// Create the default set of indexes for the manifest table.
pub fn create_manifest_indexes() -> Vec<Arc<dyn IndexDefinition<ManifestId, ManifestCacheEntry>>> {
    vec![Arc::new(ManifestPendingWriteIndex)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::manifest::{Manifest, SyncState};
    use kalamdb_commons::{NamespaceId, TableId, TableName, UserId};

    fn create_test_entry(
        table_id: TableId,
        user_id: Option<UserId>,
        sync_state: SyncState,
    ) -> ManifestCacheEntry {
        let manifest = Manifest::new(table_id, user_id);
        ManifestCacheEntry::new(manifest, None, chrono::Utc::now().timestamp_millis(), sync_state)
    }

    #[test]
    fn test_pending_write_index_only_indexes_pending() {
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("tbl1"));
        let manifest_id = ManifestId::new(table_id.clone(), None);

        let index = ManifestPendingWriteIndex;

        // PendingWrite entry should be indexed
        let entry = create_test_entry(table_id.clone(), None, SyncState::PendingWrite);
        let key = index.extract_key(&manifest_id, &entry);
        assert!(key.is_some());
        assert_eq!(key.unwrap(), manifest_id.storage_key());

        // InSync entry should NOT be indexed
        let entry = create_test_entry(table_id.clone(), None, SyncState::InSync);
        let key = index.extract_key(&manifest_id, &entry);
        assert!(key.is_none());

        // Syncing entry should NOT be indexed
        let entry = create_test_entry(table_id.clone(), None, SyncState::Syncing);
        let key = index.extract_key(&manifest_id, &entry);
        assert!(key.is_none());

        // Stale entry should NOT be indexed
        let entry = create_test_entry(table_id.clone(), None, SyncState::Stale);
        let key = index.extract_key(&manifest_id, &entry);
        assert!(key.is_none());

        // Error entry should NOT be indexed
        let entry = create_test_entry(table_id, None, SyncState::Error);
        let key = index.extract_key(&manifest_id, &entry);
        assert!(key.is_none());
    }

    #[test]
    fn test_pending_write_index_user_scoped() {
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("user_tbl"));
        let user_id = UserId::new("user1");
        let manifest_id = ManifestId::new(table_id.clone(), Some(user_id.clone()));

        let index = ManifestPendingWriteIndex;
        let entry = create_test_entry(table_id, Some(user_id), SyncState::PendingWrite);

        let key = index.extract_key(&manifest_id, &entry);
        assert!(key.is_some());
        assert_eq!(key.unwrap(), manifest_id.storage_key());
    }
}
