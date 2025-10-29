//! Job execution framework
//!
//! This module provides the infrastructure for executing background jobs with:
//! - Status tracking (running, completed, failed)
//! - Resource usage monitoring (CPU, memory)
//! - Parameter and result serialization
//! - Trace logging for debugging
//! - Node identification for distributed systems

use crate::error::KalamDbError;
use crate::tables::system::JobsTableProvider;
use kalamdb_commons::system::Job;
use kalamdb_commons::{JobId, JobStatus, JobType, NamespaceId, TableName};
use std::sync::Arc;
use std::time::Instant;

/// Job execution result
#[derive(Debug, Clone)]
pub enum JobResult {
    Success(String),
    Failure(String),
}

/// Job executor that manages job lifecycle and metrics
pub struct JobExecutor {
    jobs_provider: Arc<JobsTableProvider>,
    node_id: String,
}

impl JobExecutor {
    /// Create a new job executor
    ///
    /// # Arguments
    /// * `jobs_provider` - Provider for system.jobs table
    /// * `node_id` - Unique identifier for this node
    pub fn new(jobs_provider: Arc<JobsTableProvider>, node_id: String) -> Self {
        Self {
            jobs_provider,
            node_id,
        }
    }

    /// Execute a job with full lifecycle management
    ///
    /// This method handles:
    /// - T101: Job registration with status='running'
    /// - T102: Job completion recording (status, end_time, result)
    /// - T103: Resource usage tracking (memory, CPU)
    /// - T104: Parameter serialization
    /// - T105: Result and trace recording
    /// - T108: Node ID tracking
    ///
    /// # Arguments
    /// * `job_id` - Unique identifier for the job
    /// * `job_type` - Type of job (e.g., "flush", "compact", "cleanup")
    /// * `table_name` - Optional table name if job is table-specific
    /// * `parameters` - Job parameters as strings
    /// * `job_fn` - The actual job function to execute
    ///
    /// # Returns
    /// Result of job execution
    pub fn execute_job<F>(
        &self,
        job_id: String,
        job_type: String,
        table_name: Option<String>,
        parameters: Vec<String>,
        job_fn: F,
    ) -> Result<JobResult, KalamDbError>
    where
        F: FnOnce() -> Result<String, String>,
    {
        // Parse job_type string to enum
        let job_type_enum = match job_type.as_str() {
            "flush" => JobType::Flush,
            "compact" => JobType::Compact,
            "cleanup" => JobType::Cleanup,
            "backup" => JobType::Backup,
            "restore" => JobType::Restore,
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Unknown job type: {}",
                    job_type
                )));
            }
        };

        // T101: Register job with status='running'
        let namespace_id = NamespaceId::new("default".to_string()); // TODO: Get from context
        let mut job = Job::new(
            JobId::new(job_id.clone()),
            job_type_enum,
            namespace_id,
            self.node_id.clone(),
        );

        if let Some(tn) = table_name {
            job = job.with_table_name(TableName::new(tn));
        }

        // T104: Store parameters as JSON array
        if !parameters.is_empty() {
            let params_json =
                serde_json::to_string(&parameters).unwrap_or_else(|_| "[]".to_string());
            job = job.with_parameters(params_json);
        }

        // Insert initial job record
        self.jobs_provider.insert_job(job.clone())?;

        // T103: Start resource tracking
        let start_time = Instant::now();
        let initial_memory = Self::get_memory_usage_mb();

        // Execute the job function
        let result = job_fn();

        // T103: Calculate resource usage
        let duration = start_time.elapsed();
        let final_memory = Self::get_memory_usage_mb();
        let memory_delta_mb = (final_memory - initial_memory).max(0.0);
        let memory_used_bytes = (memory_delta_mb * 1024.0 * 1024.0) as i64;

        // Estimate CPU usage based on execution time (simplified)
        // In a real system, this would use OS-specific APIs
        let cpu_used_micros = duration.as_micros() as i64;

        // T105: Build trace information
        let trace = format!(
            "Job executed in {:.2}s on node {}",
            duration.as_secs_f64(),
            self.node_id
        );

        // T102: Update job with completion status and metrics
        let final_job = match &result {
            Ok(success_message) => job
                .with_metrics(Some(memory_used_bytes), Some(cpu_used_micros))
                .with_trace(trace)
                .complete(Some(success_message.clone())),
            Err(error_message) => job
                .with_metrics(Some(memory_used_bytes), Some(cpu_used_micros))
                .with_trace(trace)
                .fail(error_message.clone()),
        };

        // T102: Persist final job state
        self.jobs_provider.update_job(final_job)?;

        // Return result
        match result {
            Ok(msg) => Ok(JobResult::Success(msg)),
            Err(msg) => Ok(JobResult::Failure(msg)),
        }
    }

    /// Execute a job asynchronously (returns immediately, job runs in background)
    ///
    /// This is useful for long-running jobs that shouldn't block the caller.
    /// The job status can be monitored via the system.jobs table.
    ///
    /// # Arguments
    /// * `job_id` - Unique identifier for the job
    /// * `job_type` - Type of job
    /// * `table_name` - Optional table name
    /// * `parameters` - Job parameters
    /// * `job_fn` - The job function (must be Send + 'static)
    pub fn execute_async<F>(
        &self,
        job_id: String,
        job_type: String,
        table_name: Option<String>,
        parameters: Vec<String>,
        job_fn: F,
    ) -> Result<(), KalamDbError>
    where
        F: FnOnce() -> Result<String, String> + Send + 'static,
    {
        // Parse job_type string to enum
        let job_type_enum = match job_type.as_str() {
            "flush" => JobType::Flush,
            "compact" => JobType::Compact,
            "cleanup" => JobType::Cleanup,
            "backup" => JobType::Backup,
            "restore" => JobType::Restore,
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Unknown job type: {}",
                    job_type
                )));
            }
        };

        // Register job immediately
        let namespace_id = NamespaceId::new("default".to_string()); // TODO: Get from context
        let mut job = Job::new(
            job_id.clone(),
            job_type_enum,
            namespace_id,
            self.node_id.clone(),
        );

        if let Some(tn) = table_name {
            job = job.with_table_name(TableName::new(tn));
        }

        if !parameters.is_empty() {
            let params_json =
                serde_json::to_string(&parameters).unwrap_or_else(|_| "[]".to_string());
            job = job.with_parameters(params_json);
        }

        self.jobs_provider.insert_job(job)?;

        // Spawn background execution
        let jobs_provider = Arc::clone(&self.jobs_provider);
        let node_id = self.node_id.clone();

        std::thread::spawn(move || {
            let start_time = Instant::now();
            let initial_memory = Self::get_memory_usage_mb();

            let result = job_fn();

            let duration = start_time.elapsed();
            let final_memory = Self::get_memory_usage_mb();
            let memory_delta_mb = (final_memory - initial_memory).max(0.0);
            let memory_used_bytes = (memory_delta_mb * 1024.0 * 1024.0) as i64;
            let cpu_used_micros = duration.as_micros() as i64;

            let trace = format!(
                "Async job executed in {:.2}s on node {}",
                duration.as_secs_f64(),
                node_id
            );

            // Fetch current job state and update
            if let Ok(Some(mut job)) = jobs_provider.get_job(&job_id) {
                let final_job = match result {
                    Ok(success_message) => {
                        job.memory_used = Some(memory_used_bytes);
                        job.cpu_used = Some(cpu_used_micros);
                        job.trace = Some(trace);
                        job.status = JobStatus::Completed;
                        job.completed_at = Some(chrono::Utc::now().timestamp_millis());
                        job.result = Some(success_message);
                        job
                    }
                    Err(error_message) => {
                        job.memory_used = Some(memory_used_bytes);
                        job.cpu_used = Some(cpu_used_micros);
                        job.trace = Some(trace);
                        job.status = JobStatus::Failed;
                        job.completed_at = Some(chrono::Utc::now().timestamp_millis());
                        job.error_message = Some(error_message);
                        job
                    }
                };

                let _ = jobs_provider.update_job(final_job);
            }
        });

        Ok(())
    }

    /// Get current memory usage in MB
    ///
    /// This is a simplified implementation. In production, use OS-specific APIs:
    /// - Linux: /proc/self/status
    /// - macOS: task_info
    /// - Windows: GetProcessMemoryInfo
    fn get_memory_usage_mb() -> f64 {
        // Placeholder: In real implementation, query OS for process memory
        // For now, return 0.0 to avoid platform-specific dependencies
        0.0
    }

    /// Get the node ID for this executor
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_sql::KalamSql;
    use kalamdb_store::RocksDbInit;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn setup_executor() -> (JobExecutor, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let backend: Arc<dyn kalamdb_commons::storage::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());
        let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql));
        let executor = JobExecutor::new(jobs_provider, "test-node-1".to_string());
        (executor, temp_dir)
    }

    #[test]
    fn test_successful_job_execution() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_job(
            "test-job-1".to_string(),
            "test".to_string(),
            None,
            vec![],
            || Ok("Job completed successfully".to_string()),
        );

        assert!(result.is_ok());
        match result.unwrap() {
            JobResult::Success(msg) => assert_eq!(msg, "Job completed successfully"),
            JobResult::Failure(_) => panic!("Expected success"),
        }

        // Verify job record in database
        let job = executor
            .jobs_provider
            .get_job("test-job-1")
            .unwrap()
            .unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
        assert_eq!(job.result, Some("Job completed successfully".to_string()));
    }

    #[test]
    fn test_failed_job_execution() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_job(
            "test-job-2".to_string(),
            "test".to_string(),
            None,
            vec![],
            || Err("Job failed with error".to_string()),
        );

        assert!(result.is_ok());
        match result.unwrap() {
            JobResult::Success(_) => panic!("Expected failure"),
            JobResult::Failure(msg) => assert_eq!(msg, "Job failed with error"),
        }

        // Verify job record in database
        let job = executor
            .jobs_provider
            .get_job("test-job-2")
            .unwrap()
            .unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.completed_at.is_some());
        assert_eq!(job.error_message, Some("Job failed with error".to_string()));
    }

    #[test]
    fn test_job_with_table_name() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_job(
            "test-job-3".to_string(),
            "flush".to_string(),
            Some("test_table".to_string()),
            vec![],
            || Ok("Flushed successfully".to_string()),
        );

        assert!(result.is_ok());

        let job = executor
            .jobs_provider
            .get_job("test-job-3")
            .unwrap()
            .unwrap();
        assert_eq!(
            job.table_name.as_ref().map(|name| name.as_str()),
            Some("test_table")
        );
    }

    #[test]
    fn test_job_with_parameters() {
        let (executor, _temp_dir) = setup_executor();

        let params = vec!["param1".to_string(), "param2".to_string()];

        let result = executor.execute_job(
            "test-job-4".to_string(),
            "backup".to_string(),
            None,
            params.clone(),
            || Ok("Backup completed".to_string()),
        );

        assert!(result.is_ok());

        let job = executor
            .jobs_provider
            .get_job("test-job-4")
            .unwrap()
            .unwrap();
        assert!(job.parameters.is_some());
        let parsed: Vec<String> = serde_json::from_str(&job.parameters.unwrap()).unwrap();
        assert_eq!(parsed, params);
    }

    #[test]
    fn test_job_metrics_recorded() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_job(
            "test-job-5".to_string(),
            "test".to_string(),
            None,
            vec![],
            || {
                // Simulate some work
                std::thread::sleep(std::time::Duration::from_millis(10));
                Ok("Done".to_string())
            },
        );

        assert!(result.is_ok());

        let job = executor
            .jobs_provider
            .get_job("test-job-5")
            .unwrap()
            .unwrap();
        assert!(job.memory_used.is_some());
        assert!(job.cpu_used.is_some());
        assert!(job.trace.is_some());
    }

    #[test]
    fn test_node_id_tracking() {
        let (executor, _temp_dir) = setup_executor();

        assert_eq!(executor.node_id(), "test-node-1");

        let result = executor.execute_job(
            "test-job-6".to_string(),
            "test".to_string(),
            None,
            vec![],
            || Ok("Success".to_string()),
        );

        assert!(result.is_ok());

        let job = executor
            .jobs_provider
            .get_job("test-job-6")
            .unwrap()
            .unwrap();
        assert_eq!(job.node_id, "test-node-1");
    }

    #[test]
    fn test_async_job_execution() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_async(
            "async-job-1".to_string(),
            "background".to_string(),
            None,
            vec![],
            || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok("Async completed".to_string())
            },
        );

        assert!(result.is_ok());

        // Wait for async job to complete
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Verify job was updated
        let job = executor
            .jobs_provider
            .get_job("async-job-1")
            .unwrap()
            .unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert_eq!(job.result, Some("Async completed".to_string()));
    }
}
