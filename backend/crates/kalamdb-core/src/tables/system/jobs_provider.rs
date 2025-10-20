//! System.jobs table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.jobs table,
//! backed by RocksDB column family system_jobs.

use crate::error::KalamDbError;
use crate::tables::system::jobs::JobsTable;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Float64Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

/// Job data structure stored in RocksDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub job_id: String,
    pub job_type: String, // "flush", "compact", "cleanup", etc.
    pub table_name: Option<String>,
    pub status: String,             // "running", "completed", "failed"
    pub start_time: i64,            // timestamp in milliseconds
    pub end_time: Option<i64>,      // timestamp in milliseconds
    pub parameters: Option<String>, // JSON array of strings
    pub result: Option<String>,     // JSON
    pub trace: Option<String>,
    pub memory_used_mb: Option<f64>,
    pub cpu_used_percent: Option<f64>,
    pub node_id: String,
    pub error_message: Option<String>,
}

impl JobRecord {
    /// Create a new job record with running status
    pub fn new(job_id: String, job_type: String, node_id: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            job_id,
            job_type,
            table_name: None,
            status: "running".to_string(),
            start_time: now,
            end_time: None,
            parameters: None,
            result: None,
            trace: None,
            memory_used_mb: None,
            cpu_used_percent: None,
            node_id,
            error_message: None,
        }
    }

    /// Mark job as completed with result
    pub fn complete(mut self, result: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = "completed".to_string();
        self.end_time = Some(now);
        self.result = result;
        self
    }

    /// Mark job as failed with error message
    pub fn fail(mut self, error_message: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = "failed".to_string();
        self.end_time = Some(now);
        self.error_message = Some(error_message);
        self
    }

    /// Set table name
    pub fn with_table_name(mut self, table_name: String) -> Self {
        self.table_name = Some(table_name);
        self
    }

    /// Set parameters
    pub fn with_parameters(mut self, parameters: Vec<String>) -> Self {
        self.parameters = Some(serde_json::to_string(&parameters).unwrap_or_default());
        self
    }

    /// Set trace
    pub fn with_trace(mut self, trace: String) -> Self {
        self.trace = Some(trace);
        self
    }

    /// Set resource usage metrics
    pub fn with_metrics(mut self, memory_used_mb: f64, cpu_used_percent: f64) -> Self {
        self.memory_used_mb = Some(memory_used_mb);
        self.cpu_used_percent = Some(cpu_used_percent);
        self
    }
}

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

    fn convert_sql_job(job: kalamdb_sql::Job) -> JobRecord {
        JobRecord {
            job_id: job.job_id,
            job_type: job.job_type,
            table_name: if job.table_name.is_empty() {
                None
            } else {
                Some(job.table_name)
            },
            status: job.status,
            start_time: job.start_time,
            end_time: job.end_time,
            parameters: if job.parameters.is_empty() {
                None
            } else {
                Some(job.parameters.join(","))
            },
            result: job.result,
            trace: job.trace,
            memory_used_mb: job.memory_used_mb,
            cpu_used_percent: job.cpu_used_percent,
            node_id: job.node_id,
            error_message: job.error_message,
        }
    }

    /// List all jobs from the system.jobs table
    pub fn list_jobs(&self) -> Result<Vec<JobRecord>, KalamDbError> {
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
    pub fn insert_job(&self, job: JobRecord) -> Result<(), KalamDbError> {
        // Convert to kalamdb_sql model
        let sql_job = kalamdb_sql::Job {
            job_id: job.job_id,
            job_type: job.job_type,
            table_name: job.table_name.unwrap_or_default(),
            status: job.status,
            start_time: job.start_time,
            end_time: job.end_time,
            parameters: job.parameters.map(|p| vec![p]).unwrap_or_default(),
            result: job.result,
            trace: job.trace,
            memory_used_mb: job.memory_used_mb,
            cpu_used_percent: job.cpu_used_percent,
            node_id: job.node_id,
            error_message: job.error_message,
        };

        self.kalam_sql
            .insert_job(&sql_job)
            .map_err(|e| KalamDbError::Other(format!("Failed to insert job: {}", e)))
    }

    /// Update an existing job record
    pub fn update_job(&self, job: JobRecord) -> Result<(), KalamDbError> {
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
        let sql_job = kalamdb_sql::Job {
            job_id: job.job_id,
            job_type: job.job_type,
            table_name: job.table_name.unwrap_or_default(),
            status: job.status,
            start_time: job.start_time,
            end_time: job.end_time,
            parameters: job.parameters.map(|p| vec![p]).unwrap_or_default(),
            result: job.result,
            trace: job.trace,
            memory_used_mb: job.memory_used_mb,
            cpu_used_percent: job.cpu_used_percent,
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
    pub fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, KalamDbError> {
        let sql_job = self
            .kalam_sql
            .get_job(job_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get job: {}", e)))?;

        match sql_job {
            Some(j) => {
                let job = JobRecord {
                    job_id: j.job_id,
                    job_type: j.job_type,
                    table_name: if j.table_name.is_empty() {
                        None
                    } else {
                        Some(j.table_name)
                    },
                    status: j.status,
                    start_time: j.start_time,
                    end_time: j.end_time,
                    parameters: if j.parameters.is_empty() {
                        None
                    } else {
                        Some(j.parameters.join(","))
                    },
                    result: j.result,
                    trace: j.trace,
                    memory_used_mb: j.memory_used_mb,
                    cpu_used_percent: j.cpu_used_percent,
                    node_id: j.node_id,
                    error_message: j.error_message,
                };
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Delete jobs older than retention period (in days)
    pub fn cleanup_old_jobs(&self, retention_days: i64) -> Result<usize, KalamDbError> {
        let now = chrono::Utc::now().timestamp_millis();
        let retention_ms = retention_days * 24 * 60 * 60 * 1000;

        let jobs = self.list_jobs()?;
        let mut deleted = 0;

        for job in jobs {
            if job.status == "running" {
                continue;
            }

            let reference_time = job.end_time.unwrap_or(job.start_time);
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
        let mut start_times = Vec::new();
        let mut end_times = Vec::new();
        let mut parameters_vec = StringBuilder::new();
        let mut results = StringBuilder::new();
        let mut traces = StringBuilder::new();
        let mut memory_used = Vec::new();
        let mut cpu_used = Vec::new();
        let mut node_ids = StringBuilder::new();
        let mut error_messages = StringBuilder::new();

        for job in jobs {
            job_ids.append_value(&job.job_id);
            job_types.append_value(&job.job_type);
            if !job.table_name.is_empty() {
                table_names.append_value(&job.table_name);
            } else {
                table_names.append_null();
            }
            statuses.append_value(&job.status);
            start_times.push(Some(job.start_time));
            end_times.push(job.end_time);
            if !job.parameters.is_empty() {
                parameters_vec.append_value(job.parameters.join(","));
            } else {
                parameters_vec.append_null();
            }
            if let Some(res) = &job.result {
                results.append_value(res);
            } else {
                results.append_null();
            }
            if let Some(trace) = &job.trace {
                traces.append_value(trace);
            } else {
                traces.append_null();
            }
            memory_used.push(job.memory_used_mb);
            cpu_used.push(job.cpu_used_percent);
            node_ids.append_value(&job.node_id);
            if let Some(err) = &job.error_message {
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
                Arc::new(TimestampMillisecondArray::from(start_times)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(end_times)) as ArrayRef,
                Arc::new(parameters_vec.finish()) as ArrayRef,
                Arc::new(results.finish()) as ArrayRef,
                Arc::new(traces.finish()) as ArrayRef,
                Arc::new(Float64Array::from(memory_used)) as ArrayRef,
                Arc::new(Float64Array::from(cpu_used)) as ArrayRef,
                Arc::new(node_ids.finish()) as ArrayRef,
                Arc::new(error_messages.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

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
        _state: &SessionState,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        Err(DataFusionError::NotImplemented(
            "System.jobs table scanning not yet implemented. Use get_job() or scan_all_jobs() methods instead.".to_string()
        ))
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
        let (provider, _temp_dir) = setup_test_provider();

        let job = JobRecord::new(
            "job-001".to_string(),
            "flush".to_string(),
            "node-1".to_string(),
        );

        provider.insert_job(job.clone()).unwrap();

        let retrieved = provider.get_job("job-001").unwrap().unwrap();
        assert_eq!(retrieved.job_id, "job-001");
        assert_eq!(retrieved.job_type, "flush");
        assert_eq!(retrieved.status, "running");
    }

    #[test]
    fn test_job_completion() {
        let (provider, _temp_dir) = setup_test_provider();

        let job = JobRecord::new(
            "job-002".to_string(),
            "compact".to_string(),
            "node-1".to_string(),
        )
        .complete(Some("Success".to_string()));

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-002").unwrap().unwrap();
        assert_eq!(retrieved.status, "completed");
        assert!(retrieved.end_time.is_some());
        assert_eq!(retrieved.result, Some("Success".to_string()));
    }

    #[test]
    fn test_job_failure() {
        let (provider, _temp_dir) = setup_test_provider();

        let job = JobRecord::new(
            "job-003".to_string(),
            "cleanup".to_string(),
            "node-1".to_string(),
        )
        .fail("Disk full".to_string());

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-003").unwrap().unwrap();
        assert_eq!(retrieved.status, "failed");
        assert!(retrieved.end_time.is_some());
        assert_eq!(retrieved.error_message, Some("Disk full".to_string()));
    }

    #[test]
    fn test_job_with_table_name() {
        let (provider, _temp_dir) = setup_test_provider();

        let job = JobRecord::new(
            "job-004".to_string(),
            "flush".to_string(),
            "node-1".to_string(),
        )
        .with_table_name("users.messages".to_string());

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-004").unwrap().unwrap();
        assert_eq!(retrieved.table_name, Some("users.messages".to_string()));
    }

    #[test]
    fn test_job_with_parameters() {
        let (provider, _temp_dir) = setup_test_provider();

        let params = vec!["param1".to_string(), "param2".to_string()];
        let job = JobRecord::new(
            "job-005".to_string(),
            "backup".to_string(),
            "node-1".to_string(),
        )
        .with_parameters(params.clone());

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-005").unwrap().unwrap();
        assert!(retrieved.parameters.is_some());
        let parsed: Vec<String> = serde_json::from_str(&retrieved.parameters.unwrap()).unwrap();
        assert_eq!(parsed, params);
    }

    #[test]
    fn test_job_with_metrics() {
        let (provider, _temp_dir) = setup_test_provider();

        let job = JobRecord::new(
            "job-006".to_string(),
            "flush".to_string(),
            "node-1".to_string(),
        )
        .with_metrics(256.5, 45.2);

        provider.insert_job(job).unwrap();

        let retrieved = provider.get_job("job-006").unwrap().unwrap();
        assert_eq!(retrieved.memory_used_mb, Some(256.5));
        assert_eq!(retrieved.cpu_used_percent, Some(45.2));
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_delete_job() {
        let (provider, _temp_dir) = setup_test_provider();

        let job = JobRecord::new(
            "job-007".to_string(),
            "flush".to_string(),
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
        let (provider, _temp_dir) = setup_test_provider();

        // Create an old completed job (90 days ago)
        let old_time = chrono::Utc::now().timestamp_millis() - (90 * 24 * 60 * 60 * 1000);
        let old_job = JobRecord {
            job_id: "old-job".to_string(),
            job_type: "flush".to_string(),
            table_name: None,
            status: "completed".to_string(),
            start_time: old_time,
            end_time: Some(old_time + 1000),
            parameters: None,
            result: None,
            trace: None,
            memory_used_mb: None,
            cpu_used_percent: None,
            node_id: "node-1".to_string(),
            error_message: None,
        };

        // Create a recent job
        let recent_job = JobRecord::new(
            "recent-job".to_string(),
            "flush".to_string(),
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
        let (provider, _temp_dir) = setup_test_provider();

        let job1 = JobRecord::new(
            "job-scan-1".to_string(),
            "flush".to_string(),
            "node-1".to_string(),
        );
        let job2 = JobRecord::new(
            "job-scan-2".to_string(),
            "compact".to_string(),
            "node-2".to_string(),
        )
        .complete(Some("OK".to_string()));

        provider.insert_job(job1).unwrap();
        provider.insert_job(job2).unwrap();

        let batch = provider.scan_all_jobs().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 13);
    }
}
