//! System.jobs table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.jobs table.
//! Uses the new EntityStore architecture with String keys (job_id).

use super::super::SystemTableProviderExt;
use super::{new_jobs_store, JobsStore, JobsTableSchema};
use crate::error::KalamDbError;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int64Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::system::Job;
use kalamdb_commons::JobId;
use kalamdb_store::EntityStoreV2;
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.jobs table provider using EntityStore architecture
pub struct JobsTableProvider {
    store: JobsStore,
    schema: SchemaRef,
}

impl std::fmt::Debug for JobsTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobsTableProvider").finish()
    }
}

impl JobsTableProvider {
    /// Create a new jobs table provider
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new JobsTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_jobs_store(backend),
            schema: JobsTableSchema::schema(),
        }
    }

    /// Create a new job entry
    pub fn create_job(&self, job: Job) -> Result<(), KalamDbError> {
        self.store.put(&job.job_id, &job)?;
        Ok(())
    }

    /// Alias for create_job (for backward compatibility)
    pub fn insert_job(&self, job: Job) -> Result<(), KalamDbError> {
        self.create_job(job)
    }

    /// Get a job by ID
    pub fn get_job_by_id(&self, job_id: &JobId) -> Result<Option<Job>, KalamDbError> {
        Ok(self.store.get(job_id)?)
    }

    /// Alias for get_job_by_id (for backward compatibility)
    pub fn get_job(&self, job_id: &JobId) -> Result<Option<Job>, KalamDbError> {
        self.get_job_by_id(job_id)
    }

    /// Update an existing job entry
    pub fn update_job(&self, job: Job) -> Result<(), KalamDbError> {
        // Check if job exists
        if self.store.get(&job.job_id)?.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Job not found: {}",
                job.job_id
            )));
        }

        self.store.put(&job.job_id, &job)?;
        Ok(())
    }

    /// Delete a job entry
    pub fn delete_job(&self, job_id: &JobId) -> Result<(), KalamDbError> {
        self.store.delete(job_id)?;
        Ok(())
    }

    /// List all jobs
    pub fn list_jobs(&self) -> Result<Vec<Job>, KalamDbError> {
        let jobs = self.store.scan_all()?;
        Ok(jobs.into_iter().map(|(_, job)| job).collect())
    }

    /// Cancel a running job
    pub fn cancel_job(&self, job_id: &JobId) -> Result<(), KalamDbError> {
        use kalamdb_commons::JobStatus;

        // Get current job
        let job = self
            .get_job(job_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Job not found: {}", job_id)))?;

        // Check if job is still running
        if job.status != JobStatus::Running {
            return Err(KalamDbError::Other(format!(
                "Cannot cancel job {} with status '{}'",
                job_id, job.status
            )));
        }

        // Update to cancelled status
        let cancelled_job = job.cancel();
        self.update_job(cancelled_job)?;

        Ok(())
    }

    /// Cancel a running job (string version for backward compatibility)
    pub fn cancel_job_str(&self, job_id: &str) -> Result<(), KalamDbError> {
        let job_id_typed = JobId::new(job_id);
        self.cancel_job(&job_id_typed)
    }

    /// Get a job by string ID (for backward compatibility)
    pub fn get_job_str(&self, job_id: &str) -> Result<Option<Job>, KalamDbError> {
        let job_id_typed = JobId::new(job_id);
        self.get_job(&job_id_typed)
    }

    /// Delete jobs older than retention period (in days)
    pub fn cleanup_old_jobs(&self, retention_days: i64) -> Result<usize, KalamDbError> {
        use kalamdb_commons::JobStatus;

        let now = chrono::Utc::now().timestamp_millis();
        let retention_ms = retention_days * 24 * 60 * 60 * 1000;

        let jobs = self.list_jobs()?;
        let mut deleted = 0;

        for job in jobs {
            if job.status == JobStatus::Running {
                continue;
            }

            let reference_time = job
                .finished_at
                .or(job.started_at)
                .unwrap_or(job.created_at);
            if now - reference_time > retention_ms {
                self.delete_job(&job.job_id)?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    /// Scan all jobs and return as RecordBatch
    pub fn scan_all_jobs(&self) -> Result<RecordBatch, KalamDbError> {
        let jobs = self.store.scan_all()?;

        let mut job_ids = StringBuilder::new();
        let mut job_types = StringBuilder::new();
        let mut namespace_ids = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut statuses = StringBuilder::new();
        let mut parameters = StringBuilder::new();
        let mut results = StringBuilder::new();
        let mut traces = StringBuilder::new();
        let mut memory_useds = Vec::new();
        let mut cpu_useds = Vec::new();
        let mut created_ats = Vec::new();
        let mut started_ats = Vec::new();
        let mut finished_ats = Vec::new();
        let mut node_ids = StringBuilder::new();
        let mut error_messages = StringBuilder::new();

        for (_key, job) in jobs {
            job_ids.append_value(job.job_id.as_str());
            job_types.append_value(job.job_type.as_str());
            namespace_ids.append_value(job.namespace_id.as_str());
            table_names.append_option(job.table_name.as_ref().map(|t| t.as_str()));
            statuses.append_value(job.status.as_str());
            parameters.append_option(job.parameters.as_deref());
            // Note: Job struct uses 'message' and 'exception_trace' instead of 'result'/'trace'/'error_message'
            results.append_option(job.message.as_deref());
            traces.append_option(job.exception_trace.as_deref());
            memory_useds.push(job.memory_used);
            cpu_useds.push(job.cpu_used);
            created_ats.push(Some(job.created_at));
            started_ats.push(job.started_at);
            finished_ats.push(job.finished_at);
            node_ids.append_value(&job.node_id);
            error_messages.append_option(job.message.as_deref()); // message field contains error messages for failed jobs
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(job_ids.finish()) as ArrayRef,
                Arc::new(job_types.finish()) as ArrayRef,
                Arc::new(namespace_ids.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(statuses.finish()) as ArrayRef,
                Arc::new(parameters.finish()) as ArrayRef,
                Arc::new(results.finish()) as ArrayRef,
                Arc::new(traces.finish()) as ArrayRef,
                Arc::new(Int64Array::from(memory_useds)) as ArrayRef,
                Arc::new(Int64Array::from(cpu_useds)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(started_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(finished_ats)) as ArrayRef,
                Arc::new(node_ids.finish()) as ArrayRef,
                Arc::new(error_messages.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Arrow error: {}", e)))?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for JobsTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let schema = self.schema.clone();
        let batch = self.scan_all_jobs().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build jobs batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], _limit).await
    }
}

impl SystemTableProviderExt for JobsTableProvider {
    fn table_name(&self) -> &str {
        "system.jobs"
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_jobs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{JobStatus, JobType, NamespaceId, NodeId, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn make_job(job_id: &str, job_type: JobType, ns: &str) -> Job {
        let now = chrono::Utc::now().timestamp_millis();
        Job {
            job_id: JobId::new(job_id),
            job_type,
            namespace_id: NamespaceId::new(ns),
            table_name: None,
            status: JobStatus::Running,
            parameters: None,
            message: None,
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: now,
            updated_at: now,
            started_at: Some(now),
            finished_at: None,
            node_id: NodeId::from("server-01"),
            queue: None,
            priority: None,
        }
    }

    fn create_test_provider() -> JobsTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        JobsTableProvider::new(backend)
    }

    fn create_test_job(job_id: &str) -> Job {
        make_job(job_id, JobType::Flush, "default").with_table_name(TableName::new("events"))
    }

    #[test]
    fn test_create_and_get_job() {
        let provider = create_test_provider();
        let job = create_test_job("job1");

        provider.create_job(job.clone()).unwrap();

        let job_id = JobId::new("job1");
        let retrieved = provider.get_job_by_id(&job_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.job_id, job_id);
        assert_eq!(retrieved.status, JobStatus::Running);
    }

    #[test]
    fn test_update_job() {
        let provider = create_test_provider();
        let mut job = create_test_job("job1");
        provider.create_job(job.clone()).unwrap();

        // Update
        job.status = JobStatus::Completed;
        provider.update_job(job.clone()).unwrap();

        // Verify
        let job_id = JobId::new("job1");
        let retrieved = provider.get_job_by_id(&job_id).unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Completed);
    }

    #[test]
    fn test_delete_job() {
        let provider = create_test_provider();
        let job = create_test_job("job1");

        provider.create_job(job).unwrap();

        let job_id = JobId::new("job1");
        provider.delete_job(&job_id).unwrap();

        let retrieved = provider.get_job_by_id(&job_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_jobs() {
        let provider = create_test_provider();

        // Insert multiple jobs
        for i in 1..=3 {
            let job = create_test_job(&format!("job{}", i));
            provider.create_job(job).unwrap();
        }

        // Scan
        let batch = provider.scan_all_jobs().unwrap();
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 15);
    }

    #[tokio::test]
    async fn test_datafusion_scan() {
        let provider = create_test_provider();

        // Insert test data
        let job = create_test_job("job1");
        provider.create_job(job).unwrap();

        // Create DataFusion session
        let ctx = datafusion::execution::context::SessionContext::new();
        let state = ctx.state();

        // Scan via DataFusion
        let plan = provider.scan(&state, None, &[], None).await.unwrap();
        assert!(plan.schema().fields().len() > 0);
    }
}
