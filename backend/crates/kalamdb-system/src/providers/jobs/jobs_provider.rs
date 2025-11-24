//! System.jobs table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.jobs table.
//! Uses the new EntityStore architecture with String keys (job_id).

use super::{new_jobs_store, JobsStore, JobsTableSchema};
use crate::error::SystemError;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int64Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::{
    system::{Job, JobFilter, JobSortField, SortOrder},
    JobId, JobStatus, StoragePartition,
};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{Partition, StorageBackend};
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
    pub fn create_job(&self, job: Job) -> Result<(), SystemError> {
        // Maintain index: Status + CreatedAt + JobId
        let index_key = make_status_index_key(job.status, job.created_at, &job.job_id);
        let partition = Partition::new(StoragePartition::SystemJobsStatusIdx.name());
        self.store
            .backend()
            .put(&partition, &index_key, job.job_id.as_bytes())?;

        self.store.put(&job.job_id, &job)?;
        Ok(())
    }

    /// Alias for create_job (for backward compatibility)
    pub fn insert_job(&self, job: Job) -> Result<(), SystemError> {
        self.create_job(job)
    }

    /// Get a job by ID
    pub fn get_job_by_id(&self, job_id: &JobId) -> Result<Option<Job>, SystemError> {
        Ok(self.store.get(job_id)?)
    }

    /// Alias for get_job_by_id (for backward compatibility)
    pub fn get_job(&self, job_id: &JobId) -> Result<Option<Job>, SystemError> {
        self.get_job_by_id(job_id)
    }

    /// Update an existing job entry
    pub fn update_job(&self, job: Job) -> Result<(), SystemError> {
        // Check if job exists
        let old_job = self.store.get(&job.job_id)?;
        if old_job.is_none() {
            return Err(SystemError::NotFound(format!(
                "Job not found: {}",
                job.job_id
            )));
        }
        let old_job = old_job.unwrap();

        validate_job_update(&job)?;

        // Maintain index if status or created_at changed (created_at shouldn't change, but just in case)
        if old_job.status != job.status || old_job.created_at != job.created_at {
            let partition = Partition::new(StoragePartition::SystemJobsStatusIdx.name());

            // Remove old index entry
            let old_index_key =
                make_status_index_key(old_job.status, old_job.created_at, &old_job.job_id);
            self.store.backend().delete(&partition, &old_index_key)?;

            // Add new index entry
            let new_index_key = make_status_index_key(job.status, job.created_at, &job.job_id);
            self.store
                .backend()
                .put(&partition, &new_index_key, job.job_id.as_bytes())?;
        }

        self.store.put(&job.job_id, &job)?;
        Ok(())
    }

    /// Delete a job entry
    pub fn delete_job(&self, job_id: &JobId) -> Result<(), SystemError> {
        // Get job to remove index entry
        if let Some(job) = self.store.get(job_id)? {
            let index_key = make_status_index_key(job.status, job.created_at, &job.job_id);
            let partition = Partition::new(StoragePartition::SystemJobsStatusIdx.name());
            self.store.backend().delete(&partition, &index_key)?;
        }

        self.store.delete(job_id)?;
        Ok(())
    }

    /// List all jobs
    pub fn list_jobs(&self) -> Result<Vec<Job>, SystemError> {
        self.list_jobs_filtered(&JobFilter::default())
    }

    /// List jobs with filter
    pub fn list_jobs_filtered(&self, filter: &JobFilter) -> Result<Vec<Job>, SystemError> {
        // Optimization: If filtering by status(es) and sorting by CreatedAt ASC, use the index
        let use_index = filter.sort_by == Some(JobSortField::CreatedAt)
            && filter.sort_order == Some(SortOrder::Asc)
            && (filter.status.is_some() || filter.statuses.is_some());

        if use_index {
            let mut jobs = Vec::new();
            let limit = filter.limit.unwrap_or(usize::MAX);

            // Collect statuses to scan
            let mut statuses = Vec::new();
            if let Some(s) = filter.status {
                statuses.push(s);
            }
            if let Some(ref s_list) = filter.statuses {
                statuses.extend(s_list.iter().cloned());
            }
            // Deduplicate and sort statuses to scan in order (New=0, Queued=1, etc.)
            statuses.sort_by_key(|s| status_to_u8(*s));
            statuses.dedup();

            let partition = Partition::new(StoragePartition::SystemJobsStatusIdx.name());

            for status in statuses {
                if jobs.len() >= limit {
                    break;
                }

                // Prefix for this status: [status_byte]
                let prefix = vec![status_to_u8(status)];

                // Scan index
                let index_entries = self.store.backend().scan(
                    &partition,
                    Some(&prefix),
                    None,                     // Start key (could optimize created_after here)
                    Some(limit - jobs.len()), // Remaining limit
                )?;

                for (_, job_id_bytes) in index_entries {
                    let job_id_str = String::from_utf8(job_id_bytes).map_err(|e| {
                        SystemError::Other(format!("Invalid JobId in index: {}", e))
                    })?;
                    let job_id = JobId::new(job_id_str);

                    if let Some(job) = self.store.get(&job_id)? {
                        // Apply other filters that index doesn't cover
                        if self.matches_filter(&job, filter) {
                            jobs.push(job);
                        }
                    }
                }
            }

            return Ok(jobs);
        }

        // Fallback to full scan
        let all_jobs = self.store.scan_all(None, None, None)?;
        let mut jobs: Vec<Job> = all_jobs.into_iter().map(|(_, job)| job).collect();

        // Apply filters in memory
        jobs.retain(|job| self.matches_filter(job, filter));

        // Sort
        if let Some(sort_by) = filter.sort_by {
            match sort_by {
                JobSortField::CreatedAt => jobs.sort_by_key(|j| j.created_at),
                JobSortField::UpdatedAt => jobs.sort_by_key(|j| j.updated_at),
                JobSortField::Priority => jobs.sort_by_key(|j| j.priority.unwrap_or(0)),
            }

            if filter.sort_order == Some(SortOrder::Desc) {
                jobs.reverse();
            }
        }

        // Limit
        if let Some(limit) = filter.limit {
            if jobs.len() > limit {
                jobs.truncate(limit);
            }
        }

        Ok(jobs)
    }

    fn matches_filter(&self, job: &Job, filter: &JobFilter) -> bool {
        // Filter by status (single)
        if let Some(ref status) = filter.status {
            if status != &job.status {
                return false;
            }
        }

        // Filter by statuses (multiple)
        if let Some(ref statuses) = filter.statuses {
            if !statuses.contains(&job.status) {
                return false;
            }
        }

        // Filter by job type
        if let Some(ref job_type) = filter.job_type {
            if job_type != &job.job_type {
                return false;
            }
        }

        // Filter by namespace
        if let Some(ref namespace) = filter.namespace_id {
            if namespace != &job.namespace_id {
                return false;
            }
        }

        // Filter by table name
        if let Some(ref table_name) = filter.table_name {
            if job.table_name.as_ref() != Some(table_name) {
                return false;
            }
        }

        // Filter by idempotency key
        if let Some(ref key) = filter.idempotency_key {
            if job.idempotency_key.as_ref() != Some(key) {
                return false;
            }
        }

        // Filter by created_after
        if let Some(after) = filter.created_after {
            if job.created_at < after {
                return false;
            }
        }

        // Filter by created_before
        if let Some(before) = filter.created_before {
            if job.created_at >= before {
                return false;
            }
        }

        true
    }

    /// Cancel a running job
    pub fn cancel_job(&self, job_id: &JobId) -> Result<(), SystemError> {
        // Get current job
        let job = self
            .get_job(job_id)?
            .ok_or_else(|| SystemError::NotFound(format!("Job not found: {}", job_id)))?;

        // Check if job is still running
        if job.status != JobStatus::Running {
            return Err(SystemError::Other(format!(
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
    pub fn cancel_job_str(&self, job_id: &str) -> Result<(), SystemError> {
        let job_id_typed = JobId::new(job_id);
        self.cancel_job(&job_id_typed)
    }

    /// Get a job by string ID (for backward compatibility)
    pub fn get_job_str(&self, job_id: &str) -> Result<Option<Job>, SystemError> {
        let job_id_typed = JobId::new(job_id);
        self.get_job(&job_id_typed)
    }

    /// Delete jobs older than retention period (in days)
    ///
    /// Optimized to use the status index to avoid full table scan.
    pub fn cleanup_old_jobs(&self, retention_days: i64) -> Result<usize, SystemError> {
        let now = chrono::Utc::now().timestamp_millis();
        let retention_ms = retention_days * 24 * 60 * 60 * 1000;
        let cutoff_time = now - retention_ms;

        let mut deleted = 0;
        let partition = Partition::new(StoragePartition::SystemJobsStatusIdx.name());

        // Only clean up terminal statuses
        let target_statuses = [
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ];

        for status in target_statuses {
            let status_byte = status_to_u8(status);
            let prefix = vec![status_byte];

            // Scan index for this status
            // Keys are [status_byte][created_at_be][job_id_bytes]
            // Sorted by created_at ASC
            let iter = self
                .store
                .backend()
                .scan(&partition, Some(&prefix), None, None)?;

            for (key_bytes, job_id_bytes) in iter {
                // Extract created_at (bytes 1..9)
                if key_bytes.len() < 9 {
                    continue;
                }

                let mut created_at_bytes = [0u8; 8];
                created_at_bytes.copy_from_slice(&key_bytes[1..9]);
                let created_at = i64::from_be_bytes(created_at_bytes);

                // Optimization: Since index is sorted by created_at, if we encounter
                // a job created AFTER the cutoff, we can stop scanning this status.
                // Note: We use a safety margin because we really want to check finished_at,
                // and a job created before cutoff might have finished after cutoff.
                // But if created_at is WAY after cutoff (e.g. > retention period), we can stop.
                // For safety, we just check all candidates <= cutoff_time based on created_at.
                if created_at > cutoff_time {
                    break;
                }

                let job_id_str = String::from_utf8(job_id_bytes)
                    .map_err(|e| SystemError::Other(format!("Invalid JobId in index: {}", e)))?;
                let job_id = JobId::new(job_id_str);

                // Load job to check actual finished_at
                if let Some(job) = self.store.get(&job_id)? {
                    let reference_time =
                        job.finished_at.or(job.started_at).unwrap_or(job.created_at);

                    if reference_time < cutoff_time {
                        self.delete_job(&job.job_id)?;
                        deleted += 1;
                    }
                }
            }
        }

        Ok(deleted)
    }

    /// Helper to create RecordBatch from jobs
    fn create_batch(&self, jobs: Vec<(Vec<u8>, Job)>) -> Result<RecordBatch, SystemError> {
        let row_count = jobs.len();

        // Pre-allocate builders for optimal performance
        let mut job_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut job_types = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut namespace_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut table_names = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut statuses = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut parameters = StringBuilder::with_capacity(row_count, row_count * 64);
        let mut results = StringBuilder::with_capacity(row_count, row_count * 64);
        let mut traces = StringBuilder::with_capacity(row_count, row_count * 128);
        let mut memory_useds = Vec::with_capacity(row_count);
        let mut cpu_useds = Vec::with_capacity(row_count);
        let mut created_ats = Vec::with_capacity(row_count);
        let mut started_ats = Vec::with_capacity(row_count);
        let mut finished_ats = Vec::with_capacity(row_count);
        let mut node_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut error_messages = StringBuilder::with_capacity(row_count, row_count * 64);

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
        .map_err(|e| SystemError::Other(format!("Arrow error: {}", e)))?;

        Ok(batch)
    }

    /// Scan all jobs and return as RecordBatch
    pub fn scan_all_jobs(&self) -> Result<RecordBatch, SystemError> {
        let jobs = self.store.scan_all(None, None, None)?;
        self.create_batch(jobs)
    }
}

fn validate_job_update(job: &Job) -> Result<(), SystemError> {
    let status = job.status;

    if matches!(
        status,
        JobStatus::Running
            | JobStatus::Retrying
            | JobStatus::Completed
            | JobStatus::Failed
            | JobStatus::Cancelled
    ) && job.started_at.is_none()
    {
        return Err(SystemError::Other(format!(
            "Job {}: started_at must be set before marking status {}",
            job.job_id, status
        )));
    }

    if matches!(
        status,
        JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
    ) && job.finished_at.is_none()
    {
        return Err(SystemError::Other(format!(
            "Job {}: finished_at must be set before marking status {}",
            job.job_id, status
        )));
    }

    if status == JobStatus::Completed && job.message.is_none() {
        return Err(SystemError::Other(format!(
            "Job {}: result/message must be set before marking status {}",
            job.job_id, status
        )));
    }

    Ok(())
}

fn status_to_u8(status: JobStatus) -> u8 {
    match status {
        JobStatus::New => 0,
        JobStatus::Queued => 1,
        JobStatus::Running => 2,
        JobStatus::Retrying => 3,
        JobStatus::Completed => 4,
        JobStatus::Failed => 5,
        JobStatus::Cancelled => 6,
    }
}

fn make_status_index_key(status: JobStatus, created_at: i64, job_id: &JobId) -> Vec<u8> {
    let status_byte = status_to_u8(status);

    let mut key = Vec::with_capacity(1 + 8 + job_id.as_bytes().len());
    key.push(status_byte);
    key.extend_from_slice(&created_at.to_be_bytes());
    key.extend_from_slice(job_id.as_bytes());
    key
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
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        use datafusion::logical_expr::Operator;
        use datafusion::scalar::ScalarValue;

        let mut start_key = None;
        let mut prefix = None;

        // Extract start_key/prefix from filters
        for expr in filters {
            if let Expr::BinaryExpr(binary) = expr {
                if let Expr::Column(col) = binary.left.as_ref() {
                    if let Expr::Literal(val, _) = binary.right.as_ref() {
                        if col.name == "job_id" {
                            if let ScalarValue::Utf8(Some(s)) = val {
                                match binary.op {
                                    Operator::Eq => {
                                        prefix = Some(JobId::new(s));
                                    }
                                    Operator::Gt | Operator::GtEq => {
                                        start_key = Some(JobId::new(s));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        let schema = self.schema.clone();
        let jobs = self
            .store
            .scan_all(limit, prefix.as_ref(), start_key.as_ref())
            .map_err(|e| DataFusionError::Execution(format!("Failed to scan jobs: {}", e)))?;

        let batch = self.create_batch(jobs).map_err(|e| {
            DataFusionError::Execution(format!("Failed to build jobs batch: {}", e))
        })?;

        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], limit).await
    }
}

impl SystemTableProviderExt for JobsTableProvider {
    fn table_name(&self) -> &str {
        "system.jobs"
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
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
        job.finished_at = Some(job.created_at + 1);
        job.message = Some("flush complete".to_string());
        provider.update_job(job.clone()).unwrap();

        // Verify
        let job_id = JobId::new("job1");
        let retrieved = provider.get_job_by_id(&job_id).unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Completed);
    }

    #[test]
    fn test_update_job_requires_started_at_for_completed() {
        let provider = create_test_provider();
        let mut job = create_test_job("job1");
        provider.create_job(job.clone()).unwrap();

        job.status = JobStatus::Completed;
        job.started_at = None;
        job.finished_at = Some(job.created_at + 1);
        job.message = Some("flush complete".to_string());

        let err = provider.update_job(job).unwrap_err();
        match err {
            SystemError::Other(msg) => assert!(msg.contains("started_at")),
            _ => panic!("unexpected error type"),
        }
    }

    #[test]
    fn test_update_job_requires_result_for_completed() {
        let provider = create_test_provider();
        let mut job = create_test_job("job1");
        provider.create_job(job.clone()).unwrap();

        job.status = JobStatus::Completed;
        job.finished_at = Some(job.created_at + 1);
        job.message = None;

        let err = provider.update_job(job).unwrap_err();
        match err {
            SystemError::Other(msg) => assert!(msg.contains("result/message")),
            _ => panic!("unexpected error type"),
        }
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
