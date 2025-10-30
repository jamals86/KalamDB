//! Stream table store implementation using in-memory EntityStore pattern
//!
//! This module provides a SystemTableStore-based in-memory implementation for ephemeral stream tables.

use crate::stores::SystemTableStore;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_store::{EntityStoreV2 as EntityStore, test_utils::InMemoryBackend, StorageBackend};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Stream table row ID (simple string)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamTableRowId(String);

impl StreamTableRowId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(String::from_utf8_lossy(bytes).to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<[u8]> for StreamTableRowId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Stream table row data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamTableRow {
    pub row_id: String,
    pub fields: serde_json::Value,
    pub inserted_at: String,
    pub _updated: String,
    pub _deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
}

/// Type alias for stream table store (extends SystemTableStore, in-memory only)
pub type StreamTableStore = SystemTableStore<StreamTableRowId, StreamTableRow>;

/// Helper function to create a new stream table store (always in-memory)
///
/// # Arguments
/// * `namespace_id` - Namespace identifier
/// * `table_name` - Table name
///
/// # Returns
/// A new SystemTableStore instance configured for the stream table (in-memory backend)
pub fn new_stream_table_store(
    namespace_id: &NamespaceId,
    table_name: &TableName,
) -> StreamTableStore {
    let partition_name = format!(
        "{}{}:{}",
        kalamdb_commons::constants::ColumnFamilyNames::STREAM_TABLE_PREFIX,
        namespace_id.as_str(),
        table_name.as_str()
    );
    // Always use in-memory backend for stream tables
    let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    SystemTableStore::new(backend, partition_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::EntityStoreV2;

    fn create_test_store() -> StreamTableStore {
        new_stream_table_store(&NamespaceId::new("test_ns"), &TableName::new("test_stream"))
    }

    fn create_test_row(row_id: &str, ttl: Option<u64>) -> StreamTableRow {
        StreamTableRow {
            row_id: row_id.to_string(),
            fields: serde_json::json!({"event": "click"}),
            inserted_at: chrono::Utc::now().to_rfc3339(),
            _updated: chrono::Utc::now().to_rfc3339(),
            _deleted: false,
            ttl_seconds: ttl,
        }
    }

    #[test]
    fn test_stream_table_store_create() {
        let store = create_test_store();
        assert!(store.partition().contains("stream_"));
    }

    #[test]
    fn test_stream_table_store_put_get() {
        let store = create_test_store();
        let key = StreamTableRowId::new("row1");
        let row = create_test_row("row1", Some(3600));

        // Put and get
        store.put(&key, &row).unwrap();
        let retrieved = store.get(&key).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), row);
    }

    #[test]
    fn test_stream_table_store_delete() {
        let store = create_test_store();
        let key = StreamTableRowId::new("row1");
        let row = create_test_row("row1", Some(3600));

        // Put, delete, verify
        store.put(&key, &row).unwrap();
        store.delete(&key).unwrap();
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_stream_table_store_scan_all() {
        let store = create_test_store();

        // Insert multiple rows
        for i in 1..=5 {
            let key = StreamTableRowId::new(format!("row{}", i));
            let row = create_test_row(&format!("row{}", i), Some(3600));
            store.put(&key, &row).unwrap();
        }

        // Scan all
        let all_rows = store.scan_all().unwrap();
        assert_eq!(all_rows.len(), 5);
    }

    #[test]
    fn test_stream_table_ttl_eviction() {
        let store = create_test_store();

        // Insert expired row (1 second ago with 0 second TTL)
        let past = chrono::Utc::now() - chrono::Duration::seconds(1);
        let expired_key = StreamTableRowId::new("expired");
        let mut expired_row = create_test_row("expired", Some(0));
        expired_row.inserted_at = past.to_rfc3339();
        expired_row._updated = past.to_rfc3339();
        store.put(&expired_key, &expired_row).unwrap();

        // Insert non-expired row
        let fresh_key = StreamTableRowId::new("fresh");
        let fresh_row = create_test_row("fresh", Some(3600));
        store.put(&fresh_key, &fresh_row).unwrap();

        // Manual eviction logic (would be in a separate service)
        let now = chrono::Utc::now();
        let all_rows = store.scan_all().unwrap();
        let mut evicted = 0;

        for (key, row) in all_rows {
            if let Some(ttl) = row.ttl_seconds {
                if let Ok(inserted) = chrono::DateTime::parse_from_rfc3339(&row.inserted_at) {
                    let expiry = inserted + chrono::Duration::seconds(ttl as i64);
                    if now > expiry {
                        store.delete(&StreamTableRowId::new(&row.row_id)).unwrap();
                        evicted += 1;
                    }
                }
            }
        }

        assert_eq!(evicted, 1);

        // Check remaining
        let remaining = store.scan_all().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].1.row_id, "fresh");
    }

    #[test]
    fn test_stream_table_in_memory() {
        // Verify that stream tables use in-memory storage
        let store1 = create_test_store();
        let store2 = create_test_store();

        // Store1 writes
        let key = StreamTableRowId::new("row1");
        let row = create_test_row("row1", Some(3600));
        store1.put(&key, &row).unwrap();

        // Store2 can't see it (different in-memory backend instances)
        assert!(store2.get(&key).unwrap().is_none());
    }
}
