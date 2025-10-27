//! System.jobs table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.jobs table,
//! backed by RocksDB column family system_jobs.

use crate::error::KalamDbError;
use crate::tables::system::jobs::JobsTable;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int64Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::system::Job;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// System.jobs table provider backed by RocksDB
pub struct JobsTableProvider {
    pub(crate) kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl JobsTableProvider {
    /// Create a new jobs table provider
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            kalam_sql,
            schema: JobsTable::schema(),
        }
    }

    fn convert_sql_job(job: kalamdb_sql::Job) -> Job {
        use kalamdb_commons::{JobType, JobStatus, NamespaceId, TableName};
        
        let kalamdb_sql::Job {
            job_id,
            job_type,
            status,
            table_name,
            parameters,
            result,
            trace,
            memory_used,
            cpu_used,
            created_at,
            start_time,
            end_time,
            node_id,
            error_message,
        } = job;

        let parameters_json = if parameters.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&parameters).unwrap_or_else(|_| "[]".to_string()))
        };

        // Parse job_type string to enum (default to Flush if invalid)
        let job_type_enum = JobType::from_str(&job_type).unwrap_or(JobType::Flush);
        
        // Parse status string to enum (default to Failed if invalid)
        let status_enum = JobStatus::from_str(&status).unwrap_or(JobStatus::Failed);

        Job {
            job_id,
            job_type: job_type_enum,
            namespace_id: NamespaceId::new("default"), // TODO: Get from table_name or separate field
            table_name: table_name.map(TableName::new),
            status: status_enum,
            parameters: parameters_json,
            result,
            trace,
            memory_used,
            cpu_used,
            created_at: if created_at == 0 { start_time } else { created_at },
            started_at: Some(start_time),
            completed_at: end_time,
            node_id,
            error_message,
        }
    }

    /// List all jobs from the system.jobs table
    pub fn list_jobs(&self) -> Result<Vec<Job>, KalamDbError> {
        let jobs = self
            .kalam_sql
            .scan_all_jobs()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan jobs: {}", e)))?;

        Ok(jobs
            .into_iter()
            .map(Self::convert_sql_job)
            .collect::<Vec<_>>())
    }

    /// Insert a new job record
    pub fn insert_job(&self, job: Job) -> Result<(), KalamDbError> {
        let Job {
            job_id,
            job_type,
            namespace_id: _,
            table_name,
            status,
            parameters,
            result,
            trace,
            memory_used,
            cpu_used,
            created_at,
            started_at,
            completed_at,
            node_id,
            error_message,
        } = job;

        let parameters_vec = parameters
            .as_ref()
            .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
            .unwrap_or_default();

        // Convert to kalamdb_sql model (with string fields)
        let sql_job = kalamdb_sql::Job {
            job_id,
            job_type: job_type.as_str().to_string(),
            status: status.as_str().to_string(),
            table_name: table_name.map(|tn| tn.into_string()),
            parameters: parameters_vec,
            result,
            trace,
            memory_used,
            cpu_used,
            created_at,
            start_time: started_at.unwrap_or(created_at),
            end_time: completed_at,
            node_id,
            error_message,
        };

        self.kalam_sql
            .insert_job(&sql_job)
            .map_err(|e| KalamDbError::Other(format!("Failed to insert job: {}", e)))
    }

    /// Update an existing job record
    pub fn update_job(&self, job: Job) -> Result<(), KalamDbError> {
        // Check if job exists
        let existing = self
            .kalam_sql
            .get_job(&job.job_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get job: {}", e)))?;

        if existing.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Job not found: {}",
                job.job_id
            )));
        }

        // Convert to kalamdb_sql model
        let parameters_vec = job
            .parameters
            .as_ref()
            .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
            .unwrap_or_default();

        let sql_job = kalamdb_sql::Job {
            job_id: job.job_id,
            job_type: job.job_type.as_str().to_string(),
            status: job.status.as_str().to_string(),
            table_name: job.table_name.map(|tn| tn.into_string()),
            parameters: parameters_vec,
            result: job.result,
            trace: job.trace,
            memory_used: job.memory_used,
            cpu_used: job.cpu_used,
            created_at: job.created_at,
            start_time: job.started_at.unwrap_or(job.created_at),
            end_time: job.completed_at,
            node_id: job.node_id,
            error_message: job.error_message,
        };

        self.kalam_sql
            .insert_job(&sql_job)
            .map_err(|e| KalamDbError::Other(format!("Failed to update job: {}", e)))
    }

    /// Delete a job record
    pub fn delete_job(&self, job_id: &str) -> Result<(), KalamDbError> {
        self.kalam_sql
            .delete_job(job_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to delete job: {}", e)))
    }

    /// Get a job by ID
    pub fn get_job(&self, job_id: &str) -> Result<Option<Job>, KalamDbError> {
        let sql_job = self
            .kalam_sql
            .get_job(job_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get job: {}", e)))?;

        match sql_job {
            Some(j) => Ok(Some(Self::convert_sql_job(j))),
            None => Ok(None),
        }
    }

    /// Cancel a job by marking it as cancelled
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the job to cancel
    ///
    /// # Returns
    ///
    /// Ok(()) if job was cancelled, or error if:
    /// - Job doesn't exist
    /// - Job is already completed/failed/cancelled
    pub fn cancel_job(&self, job_id: &str) -> Result<(), KalamDbError> {
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
                .completed_at
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
        let jobs = self
            .kalam_sql
            .scan_all_jobs()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan jobs: {}", e)))?;

        let mut job_ids = StringBuilder::new();
        let mut job_types = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut statuses = StringBuilder::new();
        let mut parameters_vec = StringBuilder::new();
        let mut results = StringBuilder::new();
        let mut traces = StringBuilder::new();
        let mut memory_used: Vec<Option<i64>> = Vec::new();
        let mut cpu_used: Vec<Option<i64>> = Vec::new();
        let mut created_ats: Vec<Option<i64>> = Vec::new();
        let mut started_ats: Vec<Option<i64>> = Vec::new();
        let mut completed_ats: Vec<Option<i64>> = Vec::new();
        let mut node_ids = StringBuilder::new();
        let mut error_messages = StringBuilder::new();

        for job in jobs {
            let kalamdb_sql::Job {
                job_id,
                job_type,
                status,
                table_name,
                parameters,
                result,
                trace,
                memory_used: mem_used,
                cpu_used: cpu,
                created_at,
                start_time,
                end_time,
                node_id,
                error_message,
            } = job;

            job_ids.append_value(&job_id);
            job_types.append_value(&job_type);
            if let Some(name) = table_name {
                if name.is_empty() {
                    table_names.append_null();
                } else {
                    table_names.append_value(&name);
                }
            } else {
                table_names.append_null();
            }
            statuses.append_value(&status);

            if parameters.is_empty() {
                parameters_vec.append_null();
            } else {
                parameters_vec.append_value(
                    serde_json::to_string(&parameters).unwrap_or_else(|_| "[]".to_string()),
                );
            }

            if let Some(res) = result {
                results.append_value(res);
            } else {
                results.append_null();
            }

            if let Some(trace) = trace {
                traces.append_value(trace);
            } else {
                traces.append_null();
            }

            memory_used.push(mem_used);
            cpu_used.push(cpu);
            let created = if created_at == 0 { start_time } else { created_at };
            created_ats.push(Some(created));
            started_ats.push(Some(start_time));
            completed_ats.push(end_time);
            node_ids.append_value(&node_id);
            if let Some(err) = error_message {
                error_messages.append_value(err);
            } else {
                error_messages.append_null();
            }
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(job_ids.finish()) as ArrayRef,
                Arc::new(job_types.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(statuses.finish()) as ArrayRef,
                Arc::new(parameters_vec.finish()) as ArrayRef,
                Arc::new(results.finish()) as ArrayRef,
                Arc::new(traces.finish()) as ArrayRef,
                Arc::new(Int64Array::from(memory_used)) as ArrayRef,
                Arc::new(Int64Array::from(cpu_used)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(started_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(completed_ats)) as ArrayRef,
                Arc::new(node_ids.finish()) as ArrayRef,
                Arc::new(error_messages.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for JobsTableProvider {
    fn table_name(&self) -> &'static str {
        kalamdb_commons::constants::SystemTableNames::JOBS
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_jobs()
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
        _state: &SessionState,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        self.into_memory_exec(projection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use tempfile::TempDir;

    fn setup_test_provider() -> (JobsTableProvider, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let kalam_sql = Arc::new(KalamSql::new(db).unwrap());
        let provider = JobsTableProvider::new(kalam_sql);
        (provider, temp_dir)
    }

    #[test]
    fn test_insert_job() {
        use kalamdb_commons::{JobType, NamespaceId};
        
        let (provider, _temp_dir) = setup_test_provider();

        let job = Job::new(
            "job-001".to_string(),
            JobType::Flush,
            NamespaceId::new("default"),
            "node-1".to_string(),
        );

        provider.insert_job(job.clone()).unwrap();

        let retrieved = provider.get_job("job-001").unwrap().unwrap();
        assert_eq!(retrieved.job_id, "job-001");
        assert_eq!(retrieved.job_type, JobType::Flush);
        assert_eq!(retrieved.status, kalamdb_commons::JobStatus::Running);
    }

    #[test]
    fn test_job_completion() {
        use kalamdb_commons::{JobType, JobStatus, NamespaceId};
        
        let (provider, _temp_dir) = setup_test_provider();

        let job = Job::new(
            "job-002".to_string(),
            JobType::Compact,
            NamespaceId::new("default"),
            "node-1".to_string(),
        )
        .complete(Some("Success".to_string()));

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-002").unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Completed);
        assert!(retrieved.completed_at.is_some());
        assert_eq!(retrieved.result, Some("Success".to_string()));
    }

    #[test]
    fn test_job_failure() {
        use kalamdb_commons::{JobType, JobStatus, NamespaceId};
        
        let (provider, _temp_dir) = setup_test_provider();

        let job = Job::new(
            "job-003".to_string(),
            JobType::Cleanup,
            NamespaceId::new("default"),
            "node-1".to_string(),
        )
        .fail("Disk full".to_string());

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-003").unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Failed);
        assert!(retrieved.completed_at.is_some());
        assert_eq!(retrieved.error_message, Some("Disk full".to_string()));
    }

    #[test]
    fn test_job_with_table_name() {
        use kalamdb_commons::{JobType, NamespaceId, TableName};
        
        let (provider, _temp_dir) = setup_test_provider();

        let job = Job::new(
            "job-004".to_string(),
            JobType::Flush,
            NamespaceId::new("default"),
            "node-1".to_string(),
        )
        .with_table_name(TableName::new("users.messages"));

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-004").unwrap().unwrap();
        assert_eq!(retrieved.table_name, Some(TableName::new("users.messages")));
    }

    #[test]
    fn test_job_with_parameters() {
        use kalamdb_commons::{JobType, NamespaceId};
        
        let (provider, _temp_dir) = setup_test_provider();

        let params = vec!["param1".to_string(), "param2".to_string()];
        let params_json = serde_json::to_string(&params).unwrap();
        let job = Job::new(
            "job-005".to_string(),
            JobType::Backup,
            NamespaceId::new("default"),
            "node-1".to_string(),
        )
        .with_parameters(params_json);

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-005").unwrap().unwrap();
        assert!(retrieved.parameters.is_some());
        let parsed: Vec<String> = serde_json::from_str(&retrieved.parameters.unwrap()).unwrap();
        assert_eq!(parsed, params);
    }

    #[test]
    fn test_job_with_metrics() {
        use kalamdb_commons::{JobType, NamespaceId};
        
        let (provider, _temp_dir) = setup_test_provider();

        let mut job = Job::new(
            "job-006".to_string(),
            JobType::Flush,
            NamespaceId::new("default"),
            "node-1".to_string(),
        );
        job.memory_used = Some(268_435_456);
        job.cpu_used = Some(45_200);

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-006").unwrap().unwrap();
        assert_eq!(retrieved.memory_used, Some(268_435_456));
        assert_eq!(retrieved.cpu_used, Some(45_200));
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_delete_job() {
        use kalamdb_commons::{JobType, NamespaceId};
        
        let (provider, _temp_dir) = setup_test_provider();

        let job = Job::new(
            "job-007".to_string(),
            JobType::Flush,
            NamespaceId::new("default"),
            "node-1".to_string(),
        );

        provider.insert_job(job).unwrap();
        assert!(provider.get_job("job-007").unwrap().is_some());

        provider.delete_job("job-007").unwrap();
        assert!(provider.get_job("job-007").unwrap().is_none());
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_cleanup_old_jobs() {
        use kalamdb_commons::{JobType, JobStatus, NamespaceId};
        
        let (provider, _temp_dir) = setup_test_provider();

        // Create an old completed job (90 days ago)
        let old_time = chrono::Utc::now().timestamp_millis() - (90 * 24 * 60 * 60 * 1000);
        let old_job = Job {
            job_id: "old-job".to_string(),
            job_type: JobType::Flush,
            namespace_id: NamespaceId::new("default"),
            table_name: None,
            status: JobStatus::Completed,
            parameters: None,
            result: None,
            trace: None,
            memory_used: None,
            cpu_used: None,
            created_at: old_time,
            started_at: Some(old_time),
            completed_at: Some(old_time + 1000),
            node_id: "node-1".to_string(),
            error_message: None,
        };

        // Create a recent job
        let recent_job = Job::new(
            "recent-job".to_string(),
            JobType::Flush,
            NamespaceId::new("default"),
            "node-1".to_string(),
        )
        .complete(None);

        provider.insert_job(old_job).unwrap();
        provider.insert_job(recent_job).unwrap();

        // Cleanup jobs older than 30 days
        let deleted = provider.cleanup_old_jobs(30).unwrap();
        assert_eq!(deleted, 1);

        // Verify old job is gone, recent job remains
        assert!(provider.get_job("old-job").unwrap().is_none());
        assert!(provider.get_job("recent-job").unwrap().is_some());
    }

    #[test]
    fn test_scan_all_jobs() {
        use kalamdb_commons::{JobType, NamespaceId};
        
        let (provider, _temp_dir) = setup_test_provider();

        let job1 = Job::new(
            "job-scan-1".to_string(),
            JobType::Flush,
            NamespaceId::new("default"),
            "node-1".to_string(),
        );
        let job2 = Job::new(
            "job-scan-2".to_string(),
            JobType::Compact,
            NamespaceId::new("default"),
            "node-2".to_string(),
        )
        .complete(Some("OK".to_string()));

        provider.insert_job(job1).unwrap();
        provider.insert_job(job2).unwrap();

        let batch = provider.scan_all_jobs().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 14);
    }
}
