//! Live queries table index definitions
//!
//! This module defines secondary indexes for the system.live_queries table.
//!
//! ## Indexes
//!
//! 1. **TableIdIndex** - Queries by namespace_id + table_name
//!    - Key: `{namespace_id}:{table_name}\x00{live_query_id_bytes}`
//!    - Enables: "Find all subscribers to a table" efficiently
//!    - Used by: change broadcasting
//!
//! ## Note on ConnectionId lookups
//!
//! ConnectionId lookups do NOT need a secondary index because:
//! - The primary key format is `{user_id}-{connection_id}-{subscription_id}`
//! - We always have user_id when looking up connection_id
//! - With configurable `max_subscriptions_per_user` (default: 10-100), prefix scan is O(1)
//! - No extra write overhead from maintaining a secondary index

use kalamdb_commons::system::LiveQuery;
use kalamdb_commons::{LiveQueryId, TableId};
use crate::StoragePartition;
use kalamdb_store::IndexDefinition;
use std::sync::Arc;

/// Index position for TableIdIndex in the indexes array
pub const TABLE_ID_INDEX: usize = 0;

/// Index for querying live queries by table_id (namespace_id + table_name).
///
/// Key format: `{namespace_id}:{table_name}\x00{live_query_id_bytes}`
///
/// This index allows efficient queries like:
/// - "All live queries subscribed to a specific table"
/// - Used for broadcasting changes to subscribers
pub struct TableIdIndex;

impl IndexDefinition<LiveQueryId, LiveQuery> for TableIdIndex {
    fn partition(&self) -> &str {
        StoragePartition::SystemLiveQueriesTableIdx.name()
    }

    fn indexed_columns(&self) -> Vec<&str> {
        vec!["namespace_id", "table_name"]
    }

    fn extract_key(&self, primary_key: &LiveQueryId, lq: &LiveQuery) -> Option<Vec<u8>> {
        // Format: namespace_id:table_name + null separator + primary key bytes
        let table_id = TableId::new(lq.namespace_id.clone(), lq.table_name.clone());
        let table_key = table_id.to_string(); // Uses Display impl: "namespace:table"
        let mut key = Vec::with_capacity(table_key.len() + 1 + primary_key.as_bytes().len());
        key.extend_from_slice(table_key.as_bytes());
        key.push(0x00); // Null separator
        key.extend_from_slice(primary_key.as_bytes());
        Some(key)
    }

    fn filter_to_prefix(
        &self,
        _filter: &datafusion::logical_expr::Expr,
    ) -> Option<Vec<u8>> {
        // This filter is more complex (two columns), skip for now
        None
    }
}

/// Create all live queries indexes (only TableIdIndex)
pub fn create_live_queries_indexes() -> Vec<Arc<dyn IndexDefinition<LiveQueryId, LiveQuery>>> {
    vec![Arc::new(TableIdIndex)]
}

/// Build a table_id index prefix for scanning
///
/// Returns a byte vector suitable for prefix scanning the TableIdIndex.
pub fn table_id_index_prefix(table_id: &TableId) -> Vec<u8> {
    let mut prefix = table_id.to_string().into_bytes();
    prefix.push(0x00);
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::ConnectionId;
    use kalamdb_commons::{NamespaceId, NodeId, TableName, UserId};

    fn create_test_live_query() -> (LiveQueryId, LiveQuery) {
        let live_id = LiveQueryId::new(
            UserId::new("user123"),
            ConnectionId::new("conn456"),
            "sub789",
        );
        let lq = LiveQuery {
            live_id: live_id.clone(),
            connection_id: "conn456".to_string(),
            namespace_id: NamespaceId::new("default"),
            table_name: TableName::new("messages"),
            user_id: UserId::new("user123"),
            query: "SELECT * FROM messages".to_string(),
            options: Some("{}".to_string()),
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node_id: NodeId::new(1),
            subscription_id: "sub789".to_string(),
            status: kalamdb_commons::types::LiveQueryStatus::Active,
            last_ping_at: 1000,
        };
        (live_id, lq)
    }

    #[test]
    fn test_table_id_index_extract_key() {
        let (live_id, lq) = create_test_live_query();
        let index = TableIdIndex;

        let key = index.extract_key(&live_id, &lq).expect("Should extract key");
        
        // Should start with namespace:table + null separator
        assert!(key.starts_with(b"default:messages\x00"));
        // Should end with the live_id bytes
        assert!(key.ends_with(live_id.as_bytes()));
    }

    #[test]
    fn test_table_id_index_prefix() {
        let table_id = TableId::from_strings("default", "messages");
        let prefix = table_id_index_prefix(&table_id);
        assert_eq!(prefix, b"default:messages\x00".to_vec());
    }
}
