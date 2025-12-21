//! System.live_queries table store
//!
//! Provides typed storage for LiveQuery entities using IndexedEntityStore
//! with secondary indexes for efficient connection_id and table_id lookups.
//!
//! ## Indexes
//!
//! 1. **ConnectionIdIndex** - Queries by connection_id
//!    - Enables: Efficient cleanup when a connection disconnects
//!
//! 2. **TableIdIndex** - Queries by namespace_id + table_name
//!    - Enables: Broadcasting changes to subscribers of a table

use super::live_queries_indexes::create_live_queries_indexes;
use kalamdb_commons::system::LiveQuery;
use kalamdb_commons::LiveQueryId;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
use std::sync::Arc;

/// Type alias for the indexed live queries store
pub type LiveQueriesStore = IndexedEntityStore<LiveQueryId, LiveQuery>;

/// Create a new live queries store with secondary indexes
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new IndexedEntityStore for live queries with automatic index management
pub fn new_live_queries_store(backend: Arc<dyn StorageBackend>) -> LiveQueriesStore {
    IndexedEntityStore::new(backend, "system_live_queries", create_live_queries_indexes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::ConnectionId;
    use kalamdb_commons::{NamespaceId, TableName, UserId};
    use kalamdb_store::entity_store::EntityStore;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_store() -> LiveQueriesStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_live_queries_store(backend)
    }

    fn create_test_live_query(
        user_id: &str,
        connection_id: &str,
        subscription_id: &str,
        table_name: &str,
    ) -> LiveQuery {
        let live_id = LiveQueryId::new(
            UserId::new(user_id),
            ConnectionId::new(connection_id),
            subscription_id,
        );
        LiveQuery {
            live_id: live_id.clone(),
            connection_id: connection_id.to_string(),
            namespace_id: NamespaceId::new("default"),
            table_name: TableName::new(table_name),
            user_id: UserId::new(user_id),
            query: "SELECT * FROM test".to_string(),
            options: Some("{}".to_string()),
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
            subscription_id: subscription_id.to_string(),
            status: kalamdb_commons::types::LiveQueryStatus::Active,
        }
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert!(store.scan_all(None, None, None).unwrap().is_empty());
    }

    #[test]
    fn test_put_and_get_live_query() {
        let store = create_test_store();
        let live_query = create_test_live_query("user1", "conn1", "sub1", "test");

        store.insert(&live_query.live_id, &live_query).unwrap();

        let retrieved = store.get(&live_query.live_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.live_id, live_query.live_id);
        assert_eq!(retrieved.user_id.as_str(), "user1");
        assert_eq!(retrieved.table_name.as_str(), "test");
    }

    #[test]
    fn test_delete_live_query() {
        let store = create_test_store();
        let live_query = create_test_live_query("user1", "conn1", "sub1", "test");

        store.insert(&live_query.live_id, &live_query).unwrap();
        store.delete(&live_query.live_id).unwrap();

        let retrieved = store.get(&live_query.live_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_live_queries() {
        let store = create_test_store();

        // Insert multiple live queries with different connections
        for i in 1..=3 {
            let live_query =
                create_test_live_query("user1", &format!("conn{}", i), &format!("sub{}", i), "test");
            store.insert(&live_query.live_id, &live_query).unwrap();
        }

        let all = store.scan_all(None, None, None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_index_scan_by_table_id() {
        use super::super::live_queries_indexes::{table_id_index_prefix, TABLE_ID_INDEX};

        let store = create_test_store();

        // Insert live queries for different tables
        let lq1 = create_test_live_query("user1", "conn1", "sub1", "messages");
        let lq2 = create_test_live_query("user2", "conn2", "sub2", "messages");
        let lq3 = create_test_live_query("user1", "conn3", "sub3", "users");

        store.insert(&lq1.live_id, &lq1).unwrap();
        store.insert(&lq2.live_id, &lq2).unwrap();
        store.insert(&lq3.live_id, &lq3).unwrap();

        // Scan by table_id index - should find 2 for default:messages
        let prefix = table_id_index_prefix("default", "messages");
        let results = store
            .scan_by_index(TABLE_ID_INDEX, Some(&prefix), None)
            .unwrap();
        assert_eq!(results.len(), 2);

        // Scan by table_id index - should find 1 for default:users
        let prefix = table_id_index_prefix("default", "users");
        let results = store
            .scan_by_index(TABLE_ID_INDEX, Some(&prefix), None)
            .unwrap();
        assert_eq!(results.len(), 1);
    }
}
