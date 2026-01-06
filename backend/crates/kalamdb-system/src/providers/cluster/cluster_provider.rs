//! ClusterTableProvider - Virtual table showing Raft cluster status
//!
//! This is a dynamic view that fetches live data from OpenRaft metrics,
//! not a persisted table. It shows:
//! - Node ID, role (leader/follower/learner/candidate), status (active/offline/joining/unknown)
//! - RPC and API addresses
//! - Whether this is the current node
//! - Raft group leadership counts
//! - OpenRaft metrics: term, log index, replication lag, heartbeat timing

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use datafusion::arrow::array::{BooleanArray, StringArray, UInt32Array, UInt64Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::MemTable;
use datafusion::datasource::TableProvider;
use datafusion::datasource::TableType;
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_raft::{ClusterInfo, CommandExecutor};
use std::sync::OnceLock;

static CLUSTER_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

fn cluster_schema() -> SchemaRef {
    CLUSTER_SCHEMA
        .get_or_init(|| {
            Arc::new(Schema::new(vec![
                Field::new("node_id", DataType::UInt64, false),
                Field::new("role", DataType::Utf8, false),
                Field::new("status", DataType::Utf8, false),
                Field::new("rpc_addr", DataType::Utf8, false),
                Field::new("api_addr", DataType::Utf8, false),
                Field::new("is_self", DataType::Boolean, false),
                Field::new("is_leader", DataType::Boolean, false),
                Field::new("groups_leading", DataType::UInt32, false),
                Field::new("total_groups", DataType::UInt32, false),
                // OpenRaft metrics
                Field::new("current_term", DataType::UInt64, true),
                Field::new("last_applied_log", DataType::UInt64, true),
                Field::new("replication_lag", DataType::UInt64, true),
            ]))
        })
        .clone()
}

/// Provider for the system.cluster virtual table
/// 
/// This table shows live cluster status from OpenRaft metrics.
/// The data is never stored - it's always fetched fresh from memory.
pub struct ClusterTableProvider {
    /// Reference to the command executor (provides cluster info)
    executor: Arc<dyn CommandExecutor>,
    /// Cached schema
    schema: SchemaRef,
}

impl std::fmt::Debug for ClusterTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterTableProvider")
            .field("is_cluster_mode", &self.executor.is_cluster_mode())
            .finish()
    }
}

impl ClusterTableProvider {
    /// Create a new ClusterTableProvider
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self {
            executor,
            schema: cluster_schema(),
        }
    }

    /// Get cluster information from OpenRaft metrics
    pub fn get_cluster_info(&self) -> ClusterInfo {
        self.executor.get_cluster_info()
    }

    /// Build a RecordBatch from cluster info
    fn build_batch(&self) -> DataFusionResult<RecordBatch> {
        let info = self.get_cluster_info();

        let node_ids: Vec<u64> = info.nodes.iter().map(|n| n.node_id).collect();
        let roles: Vec<&str> = info.nodes.iter().map(|n| n.role.as_str()).collect();
        let statuses: Vec<&str> = info.nodes.iter().map(|n| n.status.as_str()).collect();
        let rpc_addrs: Vec<&str> = info.nodes.iter().map(|n| n.rpc_addr.as_str()).collect();
        let api_addrs: Vec<&str> = info.nodes.iter().map(|n| n.api_addr.as_str()).collect();
        let is_selfs: Vec<bool> = info.nodes.iter().map(|n| n.is_self).collect();
        let is_leaders: Vec<bool> = info.nodes.iter().map(|n| n.is_leader).collect();
        let groups_leading: Vec<u32> = info.nodes.iter().map(|n| n.groups_leading).collect();
        let total_groups: Vec<u32> = info.nodes.iter().map(|n| n.total_groups).collect();
        let current_terms: Vec<Option<u64>> = info.nodes.iter().map(|n| n.current_term).collect();
        let last_applied_logs: Vec<Option<u64>> = info.nodes.iter().map(|n| n.last_applied_log).collect();
        let replication_lags: Vec<Option<u64>> = info.nodes.iter().map(|n| n.replication_lag).collect();

        Ok(RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(UInt64Array::from(node_ids)),
                Arc::new(StringArray::from(roles)),
                Arc::new(StringArray::from(statuses)),
                Arc::new(StringArray::from(rpc_addrs)),
                Arc::new(StringArray::from(api_addrs)),
                Arc::new(BooleanArray::from(is_selfs)),
                Arc::new(BooleanArray::from(is_leaders)),
                Arc::new(UInt32Array::from(groups_leading)),
                Arc::new(UInt32Array::from(total_groups)),
                Arc::new(UInt64Array::from(current_terms)),
                Arc::new(UInt64Array::from(last_applied_logs)),
                Arc::new(UInt64Array::from(replication_lags)),
            ],
        )?)
    }
}

#[async_trait]
impl TableProvider for ClusterTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::View // Virtual table, not persisted
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let batch = self.build_batch()?;
        let table = MemTable::try_new(self.schema.clone(), vec![vec![batch]])?;
        table.scan(_state, projection, _filters, _limit).await
    }
}
