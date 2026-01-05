//! ClusterNodesTableProvider - Virtual table showing Raft cluster node status
//!
//! This is a dynamic view that fetches live data from the CommandExecutor,
//! not a persisted table. It shows:
//! - Node ID, role (leader/follower), status (active/offline)
//! - RPC and API addresses
//! - Whether this is the current node
//! - Raft group leadership counts

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

static CLUSTER_NODES_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

fn cluster_nodes_schema() -> SchemaRef {
    CLUSTER_NODES_SCHEMA
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
            ]))
        })
        .clone()
}

/// Provider for the system.cluster_nodes virtual table
pub struct ClusterNodesTableProvider {
    /// Reference to the command executor (provides cluster info)
    executor: Arc<dyn CommandExecutor>,
    /// Cached schema
    schema: SchemaRef,
}

impl std::fmt::Debug for ClusterNodesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterNodesTableProvider")
            .field("is_cluster_mode", &self.executor.is_cluster_mode())
            .finish()
    }
}

impl ClusterNodesTableProvider {
    /// Create a new ClusterNodesTableProvider
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self {
            executor,
            schema: cluster_nodes_schema(),
        }
    }

    /// Get cluster information
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
            ],
        )?)
    }
}

#[async_trait]
impl TableProvider for ClusterNodesTableProvider {
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
