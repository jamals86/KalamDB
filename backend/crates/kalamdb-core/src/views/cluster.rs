//! system.cluster virtual view
//!
//! **Type**: Virtual View (not backed by persistent storage)
//!
//! Provides live OpenRaft cluster status and metrics:
//! - Node ID, role (leader/follower/learner/candidate), status
//! - RPC and API addresses
//! - Raft group leadership counts
//! - OpenRaft metrics: term, log index, replication lag, snapshot index
//!
//! **DataFusion Pattern**: Implements VirtualView trait for consistent view behavior
//! - Fetches live data from OpenRaft metrics on each query
//! - Zero memory overhead when idle (no cached state)
//! - Only used in cluster mode
//!
//! **Performance Optimizations**:
//! - Pre-allocated vectors with known capacity
//! - Schema cached via `OnceLock` (zero-cost after first call)
//! - Direct Arrow array construction without intermediate allocations
//! - Minimal copies using references where possible
//!
//! **Schema**: TableDefinition provides consistent metadata for views

use super::view_base::{ViewTableProvider, VirtualView};
use crate::schema_registry::RegistryError;
use datafusion::arrow::array::{
    ArrayRef, BooleanArray, Int16Array, Int32Array, Int64Array, StringArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, TableName};
use kalamdb_raft::{ClusterInfo, CommandExecutor, ServerStateExt};
use kalamdb_system::SystemTable;
use std::sync::{Arc, OnceLock};

/// Get the cluster schema (memoized)
fn cluster_schema() -> SchemaRef {
    static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
    SCHEMA
        .get_or_init(|| {
            ClusterView::definition()
                .to_arrow_schema()
                .expect("Failed to convert cluster TableDefinition to Arrow schema")
        })
        .clone()
}

/// ClusterView - Displays live Raft cluster status
///
/// **Memory Efficiency**:
/// - No cached state (data fetched on-demand from OpenRaft)
/// - Only stores Arc to CommandExecutor (8 bytes pointer)
/// - Schema is memoized via `OnceLock`
///
/// **Query Performance**:
/// - O(N) where N = number of cluster nodes (typically 3-7)
/// - Single cluster info fetch per query
/// - Pre-allocated vectors avoid reallocation overhead
/// - Direct Arrow array construction
#[derive(Debug)]
pub struct ClusterView {
    /// Reference to the command executor (provides cluster info)
    /// Only 8 bytes - the actual executor is shared across the application
    executor: Arc<dyn CommandExecutor>,
}

impl ClusterView {
    /// Get the TableDefinition for system.cluster view
    ///
    /// Schema (16 columns):
    /// - cluster_id TEXT NOT NULL
    /// - node_id BIGINT NOT NULL
    /// - role TEXT NOT NULL
    /// - status TEXT NOT NULL
    /// - rpc_addr TEXT NOT NULL
    /// - api_addr TEXT NOT NULL
    /// - is_self BOOLEAN NOT NULL
    /// - is_leader BOOLEAN NOT NULL
    /// - groups_leading INT NOT NULL
    /// - total_groups INT NOT NULL
    /// - current_term BIGINT (nullable - OpenRaft metrics)
    /// - last_applied_log BIGINT (nullable)
    /// - leader_last_log_index BIGINT (nullable)
    /// - snapshot_index BIGINT (nullable)
    /// - catchup_progress_pct TINYINT (nullable)
    /// - replication_lag BIGINT (nullable)
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
                "cluster_id",
                1,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Cluster identifier".to_string()),
            ),
            ColumnDefinition::new(
                2,
                "node_id",
                2,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Node ID within the cluster".to_string()),
            ),
            ColumnDefinition::new(
                3,
                "role",
                3,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Node role (leader, follower, learner, candidate)".to_string()),
            ),
            ColumnDefinition::new(
                4,
                "status",
                4,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Node status".to_string()),
            ),
            ColumnDefinition::new(
                5,
                "rpc_addr",
                5,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("RPC address for Raft communication".to_string()),
            ),
            ColumnDefinition::new(
                6,
                "api_addr",
                6,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("API address for client requests".to_string()),
            ),
            ColumnDefinition::new(
                7,
                "is_self",
                7,
                KalamDataType::Boolean,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Whether this is the current node".to_string()),
            ),
            ColumnDefinition::new(
                8,
                "is_leader",
                8,
                KalamDataType::Boolean,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Whether this node is the leader".to_string()),
            ),
            ColumnDefinition::new(
                9,
                "groups_leading",
                9,
                KalamDataType::Int,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Number of Raft groups this node leads".to_string()),
            ),
            ColumnDefinition::new(
                10,
                "total_groups",
                10,
                KalamDataType::Int,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Total number of Raft groups".to_string()),
            ),
            // OpenRaft metrics (nullable - may not be available for all nodes)
            ColumnDefinition::new(
                11,
                "current_term",
                11,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Current Raft term".to_string()),
            ),
            ColumnDefinition::new(
                12,
                "last_applied_log",
                12,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Last applied log index".to_string()),
            ),
            ColumnDefinition::new(
                13,
                "leader_last_log_index",
                13,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Leader's last log index".to_string()),
            ),
            ColumnDefinition::new(
                14,
                "snapshot_index",
                14,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Last snapshot index".to_string()),
            ),
            ColumnDefinition::new(
                15,
                "catchup_progress_pct",
                15,
                KalamDataType::SmallInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Catchup progress percentage (0-100)".to_string()),
            ),
            ColumnDefinition::new(
                16,
                "replication_lag",
                16,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Replication lag (entries behind leader)".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new(SystemTable::Cluster.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Live OpenRaft cluster status and metrics (read-only view)".to_string()),
        )
        .expect("Failed to create system.cluster view definition")
    }

    /// Create a new cluster view
    ///
    /// **Cost**: O(1) - just stores an Arc pointer
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self { executor }
    }

    /// Get live cluster information from OpenRaft
    ///
    /// **Cost**: O(1) - CommandExecutor already has this data in memory
    fn get_cluster_info(&self) -> ClusterInfo {
        self.executor.get_cluster_info()
    }
}

impl VirtualView for ClusterView {
    fn system_table(&self) -> SystemTable {
        SystemTable::Cluster
    }

    fn schema(&self) -> SchemaRef {
        cluster_schema()
    }

    /// Compute a RecordBatch with current cluster status
    ///
    /// **Performance**: O(N) where N = number of nodes (typically 3-7)
    /// - Single cluster info fetch
    /// - Pre-allocated vectors with exact capacity
    /// - Zero intermediate string allocations (uses &str)
    /// - Direct Arrow array construction
    fn compute_batch(&self) -> Result<RecordBatch, RegistryError> {
        let info = self.get_cluster_info();
        let num_nodes = info.nodes.len();

        // Pre-allocate vectors with exact capacity to avoid reallocation
        let mut cluster_ids = Vec::with_capacity(num_nodes);
        let mut node_ids = Vec::with_capacity(num_nodes);
        let mut roles = Vec::with_capacity(num_nodes);
        let mut statuses = Vec::with_capacity(num_nodes);
        let mut rpc_addrs = Vec::with_capacity(num_nodes);
        let mut api_addrs = Vec::with_capacity(num_nodes);
        let mut is_selfs = Vec::with_capacity(num_nodes);
        let mut is_leaders = Vec::with_capacity(num_nodes);
        let mut groups_leading = Vec::with_capacity(num_nodes);
        let mut total_groups = Vec::with_capacity(num_nodes);
        let mut current_terms = Vec::with_capacity(num_nodes);
        let mut last_applied_logs = Vec::with_capacity(num_nodes);
        let mut leader_last_log_indexes = Vec::with_capacity(num_nodes);
        let mut snapshot_indexes = Vec::with_capacity(num_nodes);
        let mut catchup_progress_pcts = Vec::with_capacity(num_nodes);
        let mut replication_lags = Vec::with_capacity(num_nodes);

        // Single pass through nodes - collect all data
        for node in &info.nodes {
            cluster_ids.push(info.cluster_id.as_str());
            node_ids.push(node.node_id.as_u64() as i64);
            roles.push(node.role.as_str());
            statuses.push(node.status.as_str());
            rpc_addrs.push(node.rpc_addr.as_str());
            api_addrs.push(node.api_addr.as_str());
            is_selfs.push(node.is_self);
            is_leaders.push(node.is_leader);
            groups_leading.push(node.groups_leading as i32);
            total_groups.push(node.total_groups as i32);
            current_terms.push(node.current_term.map(|v| v as i64));
            last_applied_logs.push(node.last_applied_log.map(|v| v as i64));
            leader_last_log_indexes.push(node.leader_last_log_index.map(|v| v as i64));
            snapshot_indexes.push(node.snapshot_index.map(|v| v as i64));
            catchup_progress_pcts.push(node.catchup_progress_pct.map(|v| v as i16));
            replication_lags.push(node.replication_lag.map(|v| v as i64));
        }

        // Build RecordBatch with direct Arrow array construction
        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(StringArray::from(cluster_ids)) as ArrayRef,
                Arc::new(Int64Array::from(node_ids)) as ArrayRef,
                Arc::new(StringArray::from(roles)) as ArrayRef,
                Arc::new(StringArray::from(statuses)) as ArrayRef,
                Arc::new(StringArray::from(rpc_addrs)) as ArrayRef,
                Arc::new(StringArray::from(api_addrs)) as ArrayRef,
                Arc::new(BooleanArray::from(is_selfs)) as ArrayRef,
                Arc::new(BooleanArray::from(is_leaders)) as ArrayRef,
                Arc::new(Int32Array::from(groups_leading)) as ArrayRef,
                Arc::new(Int32Array::from(total_groups)) as ArrayRef,
                Arc::new(Int64Array::from(current_terms)) as ArrayRef,
                Arc::new(Int64Array::from(last_applied_logs)) as ArrayRef,
                Arc::new(Int64Array::from(leader_last_log_indexes)) as ArrayRef,
                Arc::new(Int64Array::from(snapshot_indexes)) as ArrayRef,
                Arc::new(Int16Array::from(catchup_progress_pcts)) as ArrayRef,
                Arc::new(Int64Array::from(replication_lags)) as ArrayRef,
            ],
        )
        .map_err(|e| RegistryError::Other(format!("Failed to build cluster batch: {}", e)))
    }
}

/// Type alias for the cluster table provider
pub type ClusterTableProvider = ViewTableProvider<ClusterView>;

/// Helper function to create a cluster table provider
///
/// **Usage**: Only called in cluster mode during AppContext initialization
pub fn create_cluster_provider(executor: Arc<dyn CommandExecutor>) -> ClusterTableProvider {
    ViewTableProvider::new(Arc::new(ClusterView::new(executor)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema() {
        let schema = cluster_schema();
        assert_eq!(schema.fields().len(), 16);
        assert_eq!(schema.field(0).name(), "cluster_id");
        assert_eq!(schema.field(1).name(), "node_id");
        assert_eq!(schema.field(2).name(), "role");
        assert_eq!(schema.field(7).name(), "is_leader");
    }

    #[test]
    fn test_schema_singleton() {
        // Verify schema is only created once
        let schema1 = cluster_schema();
        let schema2 = cluster_schema();
        assert!(Arc::ptr_eq(&schema1, &schema2));
    }
}
