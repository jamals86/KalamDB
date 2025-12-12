//! Stream table store implementation using EntityStore pattern
//!
//! This module provides an EntityStore-based implementation for stream tables.
//! Unlike system tables, stream tables use EntityStore directly (not SystemTableStore)
//! because they are user data (ephemeral events), not system metadata.
//!
//! **MVCC Architecture (Phase 13.2)**:
//! - StreamTableRowId: Composite struct with user_id and _seq fields
//! - StreamTableRow: Minimal structure with user_id, _seq, fields (JSON)
//! - Storage key format: {user_id}:{_seq} (big-endian bytes)

use crate::common::partition_name;
use kalamdb_commons::ids::{SeqId, StreamTableRowId};
use kalamdb_commons::models::row::Row;
use kalamdb_commons::models::{KTableRow, UserId};
use kalamdb_commons::TableId;
use kalamdb_store::entity_store::{EntityStore, KSerializable};
use kalamdb_store::{test_utils::InMemoryBackend, StorageBackend};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Stream table row entity (ephemeral, in-memory only)
///
/// **Design Notes**:
/// - Removed: event_id (redundant with _seq), timestamp (embedded in _seq Snowflake ID), row_id, inserted_at, _updated
/// - Kept: user_id (event owner), _seq (unique version ID with embedded timestamp), fields (all event data)
/// - Note: NO _deleted field (stream tables don't use soft deletes, only TTL eviction)
///
/// **Note on System Column Naming**:
/// The underscore prefix (`_seq`) follows SQL convention for system-managed columns.
/// This name matches the SQL column name exactly for consistency across the codebase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamTableRow {
    /// User who owns this event
    pub user_id: UserId,
    /// Monotonically increasing sequence ID (Snowflake ID with embedded timestamp)
    /// Maps to SQL column `_seq`
    pub _seq: SeqId,
    /// All event data (serialized as JSON map)
    pub fields: Row,
}

impl KSerializable for StreamTableRow {}

impl From<StreamTableRow> for KTableRow {
    fn from(row: StreamTableRow) -> Self {
        KTableRow {
            user_id: row.user_id,
            _seq: row._seq,
            _deleted: false, // Stream tables don't have soft deletes
            fields: row.fields,
        }
    }
}

/// Store for stream tables (ephemeral event data, in-memory only).
///
/// Uses composite StreamTableRowId keys (user_id:_seq) for user isolation.
/// Unlike SystemTableStore, this is a direct EntityStore implementation
/// for user event data, not system metadata.
#[derive(Clone)]
pub struct StreamTableStore {
    backend: Arc<dyn StorageBackend>,
    partition: String,
}

impl StreamTableStore {
    /// Create a new stream table store
    ///
    /// # Arguments
    /// * `backend` - Storage backend (always in-memory for stream tables)
    /// * `partition` - Partition name (e.g., "stream_default:events")
    pub fn new(backend: Arc<dyn StorageBackend>, partition: impl Into<String>) -> Self {
        Self {
            backend,
            partition: partition.into(),
        }
    }
}

/// Implement EntityStore trait for typed CRUD operations
impl EntityStore<StreamTableRowId, StreamTableRow> for StreamTableStore {
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    fn partition(&self) -> &str {
        &self.partition
    }
}

/// Helper function to create a new stream table store (always in-memory)
///
/// # Arguments
/// * `namespace_id` - Namespace identifier
/// * `table_name` - Table name
///
/// # Returns
/// A new StreamTableStore instance configured for the stream table (in-memory backend)
pub fn new_stream_table_store(
    table_id: &TableId,
) -> StreamTableStore {
    let partition_name = partition_name(
        kalamdb_commons::constants::ColumnFamilyNames::STREAM_TABLE_PREFIX,
        table_id,
    );
    // Always use in-memory backend for stream tables (ephemeral, no persistence)
    let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    StreamTableStore::new(backend, partition_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::scalar::ScalarValue;
    use kalamdb_commons::models::{NamespaceId, TableId, TableName};
    use std::collections::BTreeMap;

    fn create_test_store() -> StreamTableStore {
        let table_id = TableId::new(NamespaceId::new("test_ns"), TableName::new("test_stream"));
        new_stream_table_store(&table_id)
    }

    fn create_test_row(user_id: &str, seq: i64) -> StreamTableRow {
        let mut values = BTreeMap::new();
        values.insert("event".to_string(), ScalarValue::Utf8(Some("click".to_string())));
        values.insert("data".to_string(), ScalarValue::Int64(Some(123)));
        StreamTableRow {
            user_id: UserId::new(user_id),
            _seq: SeqId::new(seq),
            fields: Row::new(values),
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
        let key = StreamTableRowId::new(UserId::new("user1"), SeqId::new(100));
        let row = create_test_row("user1", 100);

        // Put and get
        store.put(&key, &row).unwrap();
        let retrieved = store.get(&key).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), row);
    }

    #[test]
    fn test_stream_table_store_delete() {
        let store = create_test_store();
        let key = StreamTableRowId::new(UserId::new("user1"), SeqId::new(200));
        let row = create_test_row("user1", 200);

        // Put, delete, verify
        store.put(&key, &row).unwrap();
        store.delete(&key).unwrap();
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_stream_table_store_scan_all() {
        let store = create_test_store();

        // Insert multiple rows for different users
        for user_i in 1..=2 {
            for seq_i in 1..=3 {
                let key = StreamTableRowId::new(
                    UserId::new(&format!("user{}", user_i)),
                    SeqId::new((user_i * 1000 + seq_i) as i64),
                );
                let row =
                    create_test_row(&format!("user{}", user_i), (user_i * 1000 + seq_i) as i64);
                store.put(&key, &row).unwrap();
            }
        }

        // Scan all
        let all_rows = store.scan_all(None, None, None).unwrap();
        assert_eq!(all_rows.len(), 6); // 2 users * 3 events
    }

    #[test]
    fn test_stream_table_user_isolation() {
        let store = create_test_store();

        // User1's event
        let key1 = StreamTableRowId::new(UserId::new("user1"), SeqId::new(100));
        let row1 = create_test_row("user1", 100);
        store.put(&key1, &row1).unwrap();

        // User2's event with same seq
        let key2 = StreamTableRowId::new(UserId::new("user2"), SeqId::new(100));
        let row2 = create_test_row("user2", 100);
        store.put(&key2, &row2).unwrap();

        // Both exist independently
        assert_eq!(
            store.get(&key1).unwrap().unwrap().user_id,
            UserId::new("user1")
        );
        assert_eq!(
            store.get(&key2).unwrap().unwrap().user_id,
            UserId::new("user2")
        );
    }

    #[test]
    fn test_stream_table_in_memory() {
        // Verify that stream tables use in-memory storage
        let store1 = create_test_store();
        let store2 = create_test_store();

        // Store1 writes
        let key = StreamTableRowId::new(UserId::new("user1"), SeqId::new(100));
        let row = create_test_row("user1", 100);
        store1.put(&key, &row).unwrap();

        // Store2 can't see it (different in-memory backend instances)
        assert!(store2.get(&key).unwrap().is_none());
    }
}
