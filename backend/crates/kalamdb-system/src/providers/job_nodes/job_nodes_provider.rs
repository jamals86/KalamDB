//! System.job_nodes table provider

use super::JobNodesTableSchema;
use crate::error::{SystemError, SystemResultExt};
use crate::providers::base::SystemTableScan;
use async_trait::async_trait;
use chrono::Utc;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::TableProviderFilterPushDown;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::RecordBatchBuilder;
use kalamdb_commons::models::JobNodeId;
use kalamdb_commons::system::JobNode;
use kalamdb_commons::{JobId, JobStatus, NodeId, SystemTable};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
use std::any::Any;
use std::sync::Arc;

pub type JobNodesStore = IndexedEntityStore<JobNodeId, JobNode>;

pub struct JobNodesTableProvider {
    store: JobNodesStore,
}

impl std::fmt::Debug for JobNodesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobNodesTableProvider").finish()
    }
}

impl JobNodesTableProvider {
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        let store = IndexedEntityStore::new(
            backend,
            SystemTable::JobNodes
                .column_family_name()
                .expect("JobNodes is a table, not a view"),
            Vec::new(),
        );
        Self { store }
    }

    pub fn create_job_node(&self, job_node: JobNode) -> Result<String, SystemError> {
        let key = job_node.id();
        self.store
            .insert(&key, &job_node)
            .into_system_error("insert job_node error")?;
        Ok(format!("Job node {} created", key))
    }

    pub async fn create_job_node_async(&self, job_node: JobNode) -> Result<(), SystemError> {
        let key = job_node.id();
        self.store
            .insert_async(key, job_node)
            .await
            .into_system_error("insert_async job_node error")
    }

    pub fn get_job_node(&self, job_id: &kalamdb_commons::JobId, node_id: &NodeId) -> Result<Option<JobNode>, SystemError> {
        let key = JobNodeId::new(job_id, node_id);
        Ok(self.store.get(&key)?)
    }

    pub async fn get_job_node_async(
        &self,
        job_id: &kalamdb_commons::JobId,
        node_id: &NodeId,
    ) -> Result<Option<JobNode>, SystemError> {
        let key = JobNodeId::new(job_id, node_id);
        self.store
            .get_async(key)
            .await
            .into_system_error("get_async job_node error")
    }

    pub async fn update_job_node_async(&self, job_node: JobNode) -> Result<(), SystemError> {
        let key = job_node.id();
        self.store
            .insert_async(key, job_node)
            .await
            .into_system_error("update_async job_node error")
    }

    pub fn list_for_node_with_statuses(
        &self,
        node_id: &NodeId,
        statuses: &[JobStatus],
        limit: usize,
    ) -> Result<Vec<JobNode>, SystemError> {
        let prefix = JobNodeId::prefix_for_node(node_id);
        let scan_limit = if limit == 0 { 10_000 } else { limit.saturating_mul(10) };
        let rows = self
            .store
            .scan_with_raw_prefix(&prefix, None, scan_limit)
            .into_system_error("scan job_nodes error")?;

        let mut filtered: Vec<JobNode> = rows
            .into_iter()
            .map(|(_, v)| v)
            .filter(|n| statuses.contains(&n.status))
            .collect();

        if limit > 0 && filtered.len() > limit {
            filtered.truncate(limit);
        }

        Ok(filtered)
    }

    pub async fn list_for_node_with_statuses_async(
        &self,
        node_id: &NodeId,
        statuses: &[JobStatus],
        limit: usize,
    ) -> Result<Vec<JobNode>, SystemError> {
        let prefix = JobNodeId::prefix_for_node(node_id);
        let scan_limit = if limit == 0 { 10_000 } else { limit.saturating_mul(10) };
        let rows = {
            let store = self.store.clone();
            tokio::task::spawn_blocking(move || {
                store.scan_with_raw_prefix(&prefix, None, scan_limit)
            })
            .await
            .into_system_error("scan_async job_nodes join error")?
            .into_system_error("scan_async job_nodes error")?
        };

        let mut filtered: Vec<JobNode> = rows
            .into_iter()
            .map(|(_, v)| v)
            .filter(|n| statuses.contains(&n.status))
            .collect();

        if limit > 0 && filtered.len() > limit {
            filtered.truncate(limit);
        }

        Ok(filtered)
    }

    pub async fn list_for_job_id_async(
        &self,
        job_id: &JobId,
    ) -> Result<Vec<JobNode>, SystemError> {
        let rows = self
            .store
            .scan_all_async(None, None, None)
            .await
            .into_system_error("scan_async job_nodes error")?;

        let filtered: Vec<JobNode> = rows
            .into_iter()
            .map(|(_, v)| v)
            .filter(|node| &node.job_id == job_id)
            .collect();

        Ok(filtered)
    }

    fn create_batch(&self, nodes: Vec<(JobNodeId, JobNode)>) -> Result<RecordBatch, SystemError> {
        let mut job_ids = Vec::with_capacity(nodes.len());
        let mut node_ids = Vec::with_capacity(nodes.len());
        let mut statuses = Vec::with_capacity(nodes.len());
        let mut error_messages = Vec::with_capacity(nodes.len());
        let mut created_ats = Vec::with_capacity(nodes.len());
        let mut started_ats = Vec::with_capacity(nodes.len());
        let mut finished_ats = Vec::with_capacity(nodes.len());
        let mut updated_ats = Vec::with_capacity(nodes.len());

        for (_key, node) in nodes {
            job_ids.push(Some(node.job_id.as_str().to_string()));
            node_ids.push(Some(node.node_id.to_string()));
            statuses.push(Some(node.status.as_str().to_string()));
            error_messages.push(node.error_message);
            created_ats.push(Some(node.created_at));
            started_ats.push(node.started_at);
            finished_ats.push(node.finished_at);
            updated_ats.push(Some(node.updated_at));
        }

        let mut builder = RecordBatchBuilder::new(JobNodesTableSchema::schema());
        builder
            .add_string_column_owned(job_ids)
            .add_string_column_owned(node_ids)
            .add_string_column_owned(statuses)
            .add_string_column_owned(error_messages)
            .add_timestamp_micros_column(created_ats)
            .add_timestamp_micros_column(started_ats)
            .add_timestamp_micros_column(finished_ats)
            .add_timestamp_micros_column(updated_ats);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;
        Ok(batch)
    }

    pub fn scan_all_job_nodes(&self) -> Result<RecordBatch, SystemError> {
        let nodes = self.store.scan_all_typed(None, None, None)?;
        self.create_batch(nodes)
    }

    /// Delete job_nodes older than retention period (in days).
    ///
    /// Only deletes terminal statuses: Completed, Failed, Cancelled.
    /// Uses finished_at/started_at/updated_at as reference time.
    pub fn cleanup_old_job_nodes(&self, retention_days: i64) -> Result<usize, SystemError> {
        let now = Utc::now().timestamp_millis();
        let retention_ms = retention_days * 24 * 60 * 60 * 1000;
        let cutoff_time = now - retention_ms;

        let rows = self
            .store
            .scan_all_typed(None, None, None)
            .into_system_error("scan job_nodes error")?;

        let mut deleted = 0;

        for (_key, node) in rows {
            if !matches!(
                node.status,
                JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled | JobStatus::Skipped
            ) {
                continue;
            }

            let reference_time = node
                .finished_at
                .or(node.started_at)
                .unwrap_or(node.updated_at);

            if reference_time < cutoff_time {
                self.store
                    .delete(&node.id())
                    .into_system_error("delete job_node error")?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }
}

impl SystemTableScan<JobNodeId, JobNode> for JobNodesTableProvider {
    fn store(&self) -> &kalamdb_store::IndexedEntityStore<JobNodeId, JobNode> {
        &self.store
    }

    fn table_name(&self) -> &str {
        JobNodesTableSchema::table_name()
    }

    fn primary_key_column(&self) -> &str {
        "id"
    }

    fn arrow_schema(&self) -> SchemaRef {
        JobNodesTableSchema::schema()
    }

    fn parse_key(&self, value: &str) -> Option<JobNodeId> {
        JobNodeId::from_string(value).ok()
    }

    fn create_batch_from_pairs(&self, pairs: Vec<(JobNodeId, JobNode)>) -> Result<RecordBatch, SystemError> {
        self.create_batch(pairs)
    }
}

#[async_trait]
impl TableProvider for JobNodesTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        JobNodesTableSchema::schema()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Use the common SystemTableScan implementation
        self.base_system_scan(state, projection, filters, limit).await
    }

    fn supports_filters_pushdown(
        &self,
        _filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        Ok(vec![TableProviderFilterPushDown::Unsupported; _filters.len()])
    }
}
