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
//! - Single schema lookup via OnceLock (zero-cost after first call)
//! - Direct Arrow array construction without intermediate allocations
//! - Minimal copies using references where possible

use super::view_base::{VirtualView, ViewTableProvider};
use crate::schema_registry::RegistryError;
use datafusion::arrow::array::{ArrayRef, BooleanArray, StringArray, UInt32Array, UInt64Array, UInt8Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_raft::{ClusterInfo, CommandExecutor, ServerStateExt};
use std::sync::{Arc, OnceLock};

/// Static schema for system.cluster (computed once, reused forever)
static CLUSTER_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Get or initialize the cluster schema (zero-cost after first call)
fn cluster_schema() -> SchemaRef {
    CLUSTER_SCHEMA
        .get_or_init(|| {
            Arc::new(Schema::new(vec![
                Field::new("cluster_id", DataType::Utf8, false),
                Field::new("node_id", DataType::UInt64, false),
                Field::new("role", DataType::Utf8, false),
                Field::new("status", DataType::Utf8, false),
                Field::new("rpc_addr", DataType::Utf8, false),
                Field::new("api_addr", DataType::Utf8, false),
                Field::new("is_self", DataType::Boolean, false),
                Field::new("is_leader", DataType::Boolean, false),
                Field::new("groups_leading", DataType::UInt32, false),
                Field::new("total_groups", DataType::UInt32, false),
                // OpenRaft metrics (nullable - may not be available for all nodes)
                Field::new("current_term", DataType::UInt64, true),
                Field::new("last_applied_log", DataType::UInt64, true),
                Field::new("leader_last_log_index", DataType::UInt64, true),
                Field::new("snapshot_index", DataType::UInt64, true),
                Field::new("catchup_progress_pct", DataType::UInt8, true),
                Field::new("replication_lag", DataType::UInt64, true),
            ]))
        })
        .clone()
}

/// ClusterView - Displays live Raft cluster status
///
/// **Memory Efficiency**:
/// - No cached state (data fetched on-demand from OpenRaft)
/// - Only stores Arc to CommandExecutor (8 bytes pointer)
/// - Schema is static and shared across all instances
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
            node_ids.push(node.node_id.as_u64());
            roles.push(node.role.as_str());
            statuses.push(node.status.as_str());
            rpc_addrs.push(node.rpc_addr.as_str());
            api_addrs.push(node.api_addr.as_str());
            is_selfs.push(node.is_self);
            is_leaders.push(node.is_leader);
            groups_leading.push(node.groups_leading);
            total_groups.push(node.total_groups);
            current_terms.push(node.current_term);
            last_applied_logs.push(node.last_applied_log);
            leader_last_log_indexes.push(node.leader_last_log_index);
            snapshot_indexes.push(node.snapshot_index);
            catchup_progress_pcts.push(node.catchup_progress_pct);
            replication_lags.push(node.replication_lag);
        }

        // Build RecordBatch with direct Arrow array construction
        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(StringArray::from(cluster_ids)) as ArrayRef,
                Arc::new(UInt64Array::from(node_ids)) as ArrayRef,
                Arc::new(StringArray::from(roles)) as ArrayRef,
                Arc::new(StringArray::from(statuses)) as ArrayRef,
                Arc::new(StringArray::from(rpc_addrs)) as ArrayRef,
                Arc::new(StringArray::from(api_addrs)) as ArrayRef,
                Arc::new(BooleanArray::from(is_selfs)) as ArrayRef,
                Arc::new(BooleanArray::from(is_leaders)) as ArrayRef,
                Arc::new(UInt32Array::from(groups_leading)) as ArrayRef,
                Arc::new(UInt32Array::from(total_groups)) as ArrayRef,
                Arc::new(UInt64Array::from(current_terms)) as ArrayRef,
                Arc::new(UInt64Array::from(last_applied_logs)) as ArrayRef,
                Arc::new(UInt64Array::from(leader_last_log_indexes)) as ArrayRef,
                Arc::new(UInt64Array::from(snapshot_indexes)) as ArrayRef,
                Arc::new(UInt8Array::from(catchup_progress_pcts)) as ArrayRef,
                Arc::new(UInt64Array::from(replication_lags)) as ArrayRef,
            ],
        )
        .map_err(|e| RegistryError::Other(format!("Failed to build cluster batch: {}", e)))
    }

    fn view_name(&self) -> &str {
        "system.cluster"
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
