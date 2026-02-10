//! System.job_nodes table provider

use super::JobNodesTableSchema;
use crate::error::{SystemError, SystemResultExt};
use crate::providers::base::IndexedProviderDefinition;
use crate::providers::job_nodes::models::JobNode;
use crate::JobStatus;
use chrono::Utc;
use datafusion::arrow::array::RecordBatch;
use kalamdb_commons::models::JobNodeId;
use kalamdb_commons::{JobId, NodeId, SystemTable};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
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
        self.store.insert(&key, &job_node).into_system_error("insert job_node error")?;
        Ok(format!("Job node {} created", key))
    }

    pub async fn create_job_node_async(&self, job_node: JobNode) -> Result<(), SystemError> {
        let key = job_node.id();
        self.store
            .insert_async(key, job_node)
            .await
            .into_system_error("insert_async job_node error")
    }

    pub fn get_job_node(
        &self,
        job_id: &kalamdb_commons::JobId,
        node_id: &NodeId,
    ) -> Result<Option<JobNode>, SystemError> {
        let key = JobNodeId::new(job_id, node_id);
        Ok(self.store.get(&key)?)
    }

    pub async fn get_job_node_async(
        &self,
        job_id: &kalamdb_commons::JobId,
        node_id: &NodeId,
    ) -> Result<Option<JobNode>, SystemError> {
        let key = JobNodeId::new(job_id, node_id);
        self.store.get_async(key).await.into_system_error("get_async job_node error")
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
        let scan_limit = if limit == 0 {
            10_000
        } else {
            limit.saturating_mul(10)
        };
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
        let scan_limit = if limit == 0 {
            10_000
        } else {
            limit.saturating_mul(10)
        };
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

    pub async fn list_for_job_id_async(&self, job_id: &JobId) -> Result<Vec<JobNode>, SystemError> {
        let rows = self
            .store
            .scan_all_async(None, None, None)
            .await
            .into_system_error("scan_async job_nodes error")?;

        let filtered: Vec<JobNode> =
            rows.into_iter().map(|(_, v)| v).filter(|node| &node.job_id == job_id).collect();

        Ok(filtered)
    }

    fn create_batch(&self, nodes: Vec<(JobNodeId, JobNode)>) -> Result<RecordBatch, SystemError> {
        crate::build_record_batch!(
            schema: JobNodesTableSchema::schema(),
            entries: nodes,
            columns: [
                job_ids => OptionalString(|entry| Some(entry.1.job_id.as_str())),
                node_ids => OptionalString(|entry| Some(entry.1.node_id.to_string())),
                statuses => OptionalString(|entry| Some(entry.1.status.as_str())),
                error_messages => OptionalString(|entry| entry.1.error_message.as_deref()),
                created_ats => Timestamp(|entry| Some(entry.1.created_at)),
                started_ats => OptionalTimestamp(|entry| entry.1.started_at),
                finished_ats => OptionalTimestamp(|entry| entry.1.finished_at),
                updated_ats => Timestamp(|entry| Some(entry.1.updated_at))
            ]
        )
        .into_arrow_error("Failed to create RecordBatch")
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
                JobStatus::Completed
                    | JobStatus::Failed
                    | JobStatus::Cancelled
                    | JobStatus::Skipped
            ) {
                continue;
            }

            let reference_time = node.finished_at.or(node.started_at).unwrap_or(node.updated_at);

            if reference_time < cutoff_time {
                self.store.delete(&node.id()).into_system_error("delete job_node error")?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }
    fn provider_definition() -> IndexedProviderDefinition<JobNodeId> {
        IndexedProviderDefinition {
            table_name: JobNodesTableSchema::table_name(),
            primary_key_column: "id",
            schema: JobNodesTableSchema::schema,
            parse_key: |value| JobNodeId::from_string(value).ok(),
        }
    }
}

crate::impl_indexed_system_table_provider!(
    provider = JobNodesTableProvider,
    key = JobNodeId,
    value = JobNode,
    store = store,
    definition = provider_definition,
    build_batch = create_batch,
    load_batch = scan_all_job_nodes
);
