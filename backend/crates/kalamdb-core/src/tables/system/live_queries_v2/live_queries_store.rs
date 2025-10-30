//! System.live_queries table store
//!
//! Provides typed storage for LiveQuery entities using SystemTableStore.

use crate::stores::SystemTableStore;
use kalamdb_commons::system::LiveQuery;
use kalamdb_commons::LiveQueryId;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Type alias for the live queries store
pub type LiveQueriesStore = SystemTableStore<LiveQueryId, LiveQuery>;

/// Create a new live queries store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore for live queries
pub fn new_live_queries_store(backend: Arc<dyn StorageBackend>) -> LiveQueriesStore {
    SystemTableStore::new(backend, "system_live_queries")
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{NamespaceId, TableName, UserId};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_store() -> LiveQueriesStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_live_queries_store(backend)
    }

    fn create_test_live_query(live_id: &str, user_id: &str, table_name: &str) -> LiveQuery {
        LiveQuery {
            live_id: LiveQueryId::new(live_id),
            connection_id: "conn123".to_string(),
            namespace_id: NamespaceId::new("default"),
            table_name: TableName::new(table_name),
            query_id: "query123".to_string(),
            user_id: UserId::new(user_id),
            query: "SELECT * FROM test".to_string(),
            options: Some("{}".to_string()),
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        }
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert!(EntityStore::scan_all(&store).unwrap().is_empty());
    }

    #[test]
    fn test_put_and_get_live_query() {
        let store = create_test_store();
        let live_query = create_test_live_query("user1-conn1-test-q1", "user1", "test");

        EntityStore::put(&store, &live_query.live_id, &live_query).unwrap();

        let retrieved = EntityStore::get(&store, &live_query.live_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.live_id, live_query.live_id);
        assert_eq!(retrieved.user_id.as_str(), "user1");
        assert_eq!(retrieved.table_name.as_str(), "test");
    }

    #[test]
    fn test_delete_live_query() {
        let store = create_test_store();
        let live_query = create_test_live_query("user1-conn1-test-q1", "user1", "test");

        EntityStore::put(&store, &live_query.live_id, &live_query).unwrap();
        EntityStore::delete(&store, &live_query.live_id).unwrap();

        let retrieved = EntityStore::get(&store, &live_query.live_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_live_queries() {
        let store = create_test_store();

        // Insert multiple live queries
        for i in 1..=3 {
            let live_query =
                create_test_live_query(&format!("user1-conn{}-test-q{}", i, i), "user1", "test");
            EntityStore::put(&store, &live_query.live_id, &live_query).unwrap();
        }

        let all = EntityStore::scan_all(&store).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_admin_only_access() {
        let store = create_test_store();
        let live_query = create_test_live_query("user1-conn1-test-q1", "user1", "test");

        // Admin can write
        EntityStore::put(&store, &live_query.live_id, &live_query).unwrap();

        // Admin can read
        let retrieved = EntityStore::get(&store, &live_query.live_id).unwrap();
        assert!(retrieved.is_some());

        // Admin can delete
        EntityStore::delete(&store, &live_query.live_id).unwrap();
        let deleted = EntityStore::get(&store, &live_query.live_id).unwrap();
        assert!(deleted.is_none());
    }
}
