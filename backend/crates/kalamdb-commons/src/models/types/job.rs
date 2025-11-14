//! Job entity for system.jobs table.
//!
//! Represents a background job (flush, retention, cleanup, etc.).

use crate::models::{ids::{JobId, NamespaceId, NodeId}, JobStatus, JobType, TableName};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};


/// Job entity for system.jobs table.
///
/// Represents a background job (flush, retention, cleanup, etc.).
///
/// ## Fields
/// - `job_id`: Unique job identifier (e.g., "job_123456")
/// - `job_type`: Type of job (Flush, Compact, Cleanup, Backup, Restore)
/// - `namespace_id`: Namespace this job operates on
/// - `table_name`: Optional table name for table-specific jobs
/// - `status`: Job status (Running, Completed, Failed, Cancelled)
/// - `parameters`: Optional JSON array of job parameters
/// - `result`: Optional result message (for completed jobs)
/// - `trace`: Optional stack trace (for failed jobs)
/// - `memory_used`: Optional memory usage in bytes
/// - `cpu_used`: Optional CPU time in microseconds
/// - `created_at`: Unix timestamp in milliseconds when job was created
/// - `started_at`: Optional Unix timestamp in milliseconds when job started
/// - `completed_at`: Optional Unix timestamp in milliseconds when job completed
/// - `node_id`: Node/server that owns this job
/// - `error_message`: Optional error message (for failed jobs)
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust,ignore
/// use kalamdb_commons::types::Job;
/// use kalamdb_commons::{NamespaceId, TableName, JobType, JobStatus, JobId, NodeId};
///
/// let job = Job {
///     job_id: JobId::new("job_123456"),
///     job_type: JobType::Flush,
///     namespace_id: NamespaceId::new("default"),
///     table_name: Some(TableName::new("events")),
///     status: JobStatus::Running,
///     parameters: None,
///     message: None,
///     exception_trace: None,
///     idempotency_key: None,
///     retry_count: 0,
///     max_retries: 3,
///     memory_used: None,
///     cpu_used: None,
///     created_at: 1730000000000,
///     updated_at: 1730000000000,
///     started_at: Some(1730000000000),
///     finished_at: None,
///     node_id: NodeId::from("server-01"),
///     queue: None,
///     priority: None,
///     result: None,
///     error_message: None,
///     trace: None,
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct Job {
    pub job_id: JobId,
    pub job_type: JobType,
    pub namespace_id: NamespaceId,
    pub table_name: Option<TableName>,
    pub status: JobStatus,
    pub parameters: Option<String>, // JSON object (migrated from array)
    pub message: Option<String>,    // Unified field replacing result/error_message
    pub exception_trace: Option<String>, // Full stack trace on failures
    pub idempotency_key: Option<String>, // For preventing duplicate jobs
    pub retry_count: u8,            // Number of retries attempted (default 0)
    pub max_retries: u8,            // Maximum retries allowed (default 3)
    pub memory_used: Option<i64>,   // bytes
    pub cpu_used: Option<i64>,      // microseconds
    pub created_at: i64,            // Unix timestamp in milliseconds
    pub updated_at: i64,            // Unix timestamp in milliseconds
    pub started_at: Option<i64>,    // Unix timestamp in milliseconds
    pub finished_at: Option<i64>,   // Unix timestamp in milliseconds (renamed from completed_at)
    #[bincode(with_serde)]
    pub node_id: NodeId,
    pub queue: Option<String>,      // Queue name (future use)
    pub priority: Option<i32>,      // Priority value (future use)
}

impl Job {
    /// Mark job as cancelled
    pub fn cancel(mut self) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Cancelled;
        self.updated_at = now;
        self.finished_at = Some(now);
        self
    }

    /// Queue the job (transition from New to Queued)
    pub fn queue(mut self) -> Self {
        self.status = JobStatus::Queued;
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self
    }

    /// Start the job (transition to Running)
    pub fn start(mut self) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Running;
        self.updated_at = now;
        self.started_at = Some(now);
        self
    }

    /// Check if job can be retried
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Set table name
    pub fn with_table_name(mut self, table_name: TableName) -> Self {
        self.table_name = Some(table_name);
        self
    }

    /// Set parameters (JSON object)
    pub fn with_parameters(mut self, parameters: String) -> Self {
        self.parameters = Some(parameters);
        self
    }

    /// Set idempotency key for duplicate prevention
    pub fn with_idempotency_key(mut self, key: String) -> Self {
        self.idempotency_key = Some(key);
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u8) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set queue name
    pub fn with_queue(mut self, queue: String) -> Self {
        self.queue = Some(queue);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Set resource metrics (memory and CPU usage)
    pub fn with_metrics(mut self, memory_used: Option<i64>, cpu_used: Option<i64>) -> Self {
        self.memory_used = memory_used;
        self.cpu_used = cpu_used;
        self
    }
}

/// Options for job creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOptions {
    /// Maximum number of retries (default: 3)
    pub max_retries: Option<u8>,
    /// Queue name for job routing (future use)
    pub queue: Option<String>,
    /// Priority value (higher = more priority, future use)
    pub priority: Option<i32>,
    /// Idempotency key to prevent duplicate job creation
    pub idempotency_key: Option<String>,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            max_retries: Some(3),
            queue: None,
            priority: None,
            idempotency_key: None,
        }
    }
}

/// Filter criteria for job queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobFilter {
    /// Filter by job type
    pub job_type: Option<JobType>,
    /// Filter by job status
    pub status: Option<JobStatus>,
    /// Filter by namespace
    pub namespace_id: Option<NamespaceId>,
    /// Filter by table name
    pub table_name: Option<TableName>,
    /// Filter by idempotency key
    pub idempotency_key: Option<String>,
    /// Limit number of results
    pub limit: Option<usize>,
    /// Start from created_at timestamp (inclusive)
    pub created_after: Option<i64>,
    /// End at created_at timestamp (exclusive)
    pub created_before: Option<i64>,
}

impl Default for JobFilter {
    fn default() -> Self {
        Self {
            job_type: None,
            status: None,
            namespace_id: None,
            table_name: None,
            idempotency_key: None,
            limit: Some(100),
            created_after: None,
            created_before: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn test_job_serialization() {
        let job = Job {
            job_id: "job_123".into(),
            job_type: JobType::Flush,
            namespace_id: NamespaceId::new("default"),
            table_name: Some(TableName::new("events")),
            status: JobStatus::Completed,
            parameters: None,
            message: Some("Job completed successfully".to_string()),
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: 1730000000000,
            updated_at: 1730000300000,
            started_at: Some(1730000000000),
            finished_at: Some(1730000300000),
            node_id: NodeId::from("server-01"),
            queue: None,
            priority: None,
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&job, config).unwrap();
        let (deserialized, _): (Job, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(job, deserialized);
    }

    #[test]
    fn test_job_cancel() {
        let job = Job {
            job_id: JobId::new("job_123"),
            job_type: JobType::Flush,
            namespace_id: NamespaceId::new("default"),
            table_name: Some(TableName::new("events")),
            status: JobStatus::Running,
            parameters: None,
            message: None,
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: 1730000000000,
            updated_at: 1730000000000,
            started_at: Some(1730000000000),
            finished_at: None,
            node_id: NodeId::from("server-01"),
            queue: None,
            priority: None,
        };

        let cancelled = job.cancel();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert!(cancelled.finished_at.is_some());
    }
}
