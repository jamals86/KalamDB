//! Job entity for system.jobs table.
//!
//! Represents a background job (flush, retention, cleanup, etc.).

use kalamdb_commons::{
    datatypes::KalamDataType,
    models::{
        ids::{JobId, NamespaceId, NodeId},
        schemas::{ColumnDefinition, ColumnDefault, TableDefinition, TableOptions, TableType},
        JobStatus, JobType, TableName,
    },
    system_tables::SystemTable,
    KSerializable,
};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Job entity for system.jobs table.
///
/// Represents a background job (flush, retention, cleanup, etc.).
///
/// ## Fields
/// - `job_id`: Unique job identifier (e.g., "job_123456")
/// - `job_type`: Type of job (Flush, Compact, Cleanup, Backup, Restore)
/// - `status`: Job status (Running, Completed, Failed, Cancelled)
/// - `parameters`: Optional JSON object containing job parameters (includes namespace_id, table_name, etc.)
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
/// use kalamdb_system::providers::jobs::models::Job;
/// use kalamdb_commons::{JobType, JobStatus, JobId, NodeId};
///
/// let job = Job {
///     job_id: JobId::new("job_123456"),
///     job_type: JobType::Flush,
///     status: JobStatus::Running,
///     leader_status: None,
///     parameters: Some(r#"{"namespace_id":"default","table_name":"events"}"#.to_string()),
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
///     node_id: NodeId::from(1u64),
///     leader_node_id: None,
///     queue: None,
///     priority: None,
/// };
/// ```
/// Job struct with fields ordered for optimal memory alignment.
/// 8-byte aligned fields first (i64, pointers/String), then smaller types.
/// This minimizes struct padding and improves cache efficiency.
///
/// ## Distributed Job Execution Model
///
/// Jobs in a cluster have two execution phases:
/// - **Local work**: Runs on ALL nodes (e.g., RocksDB flush, local cache eviction)
/// - **Leader actions**: Runs ONLY on leader (e.g., Parquet upload to S3, shared metadata updates)
///
/// The `status` field tracks overall job status (local work on this node).
/// The `leader_status` field tracks leader-only actions when this node is the leader.
/// The `leader_node_id` field indicates which node performed leader actions.
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct Job {
    // 8-byte aligned fields first (i64, Option<i64>, String/pointer types)
    pub created_at: i64,          // Unix timestamp in milliseconds
    pub updated_at: i64,          // Unix timestamp in milliseconds
    pub started_at: Option<i64>,  // Unix timestamp in milliseconds
    pub finished_at: Option<i64>, // Unix timestamp in milliseconds
    pub memory_used: Option<i64>, // bytes
    pub cpu_used: Option<i64>,    // microseconds
    pub job_id: JobId,
    #[bincode(with_serde)]
    pub node_id: NodeId,
    /// Node that performed leader actions (if any). Only set when leader_status is Some.
    #[bincode(with_serde)]
    pub leader_node_id: Option<NodeId>,
    pub parameters: Option<String>, // JSON object containing namespace_id, table_name, and other params
    pub message: Option<String>,    // Unified field replacing result/error_message
    pub exception_trace: Option<String>, // Full stack trace on failures
    pub idempotency_key: Option<String>, // For preventing duplicate jobs
    pub queue: Option<String>,      // Queue name (future use)
    // 4-byte aligned fields (enums, i32)
    pub priority: Option<i32>, // Priority value (future use)
    pub job_type: JobType,
    /// Status of local work (runs on all nodes)
    pub status: JobStatus,
    /// Status of leader-only actions (only set on leader node for jobs with leader actions)
    pub leader_status: Option<JobStatus>,
    // 1-byte fields last
    pub retry_count: u8, // Number of retries attempted (default 0)
    pub max_retries: u8, // Maximum retries allowed (default 3)
}

impl Job {
    /// Mark job as cancelled
    #[inline]
    pub fn cancel(mut self) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Cancelled;
        self.updated_at = now;
        self.finished_at = Some(now);
        self
    }

    /// Queue the job (transition from New to Queued)
    #[inline]
    pub fn queue(mut self) -> Self {
        self.status = JobStatus::Queued;
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self
    }

    /// Start the job (transition to Running)
    #[inline]
    pub fn start(mut self) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Running;
        self.updated_at = now;
        self.started_at = Some(now);
        self
    }

    /// Check if job can be retried
    #[inline]
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Extract namespace_id from parameters JSON
    pub fn namespace_id(&self) -> Option<NamespaceId> {
        self.parameters.as_ref().and_then(|p| {
            serde_json::from_str::<serde_json::Value>(p)
                .ok()
                .and_then(|v| v.get("namespace_id")?.as_str().map(NamespaceId::new))
        })
    }

    /// Extract table_name from parameters JSON
    pub fn table_name(&self) -> Option<TableName> {
        self.parameters.as_ref().and_then(|p| {
            serde_json::from_str::<serde_json::Value>(p)
                .ok()
                .and_then(|v| v.get("table_name")?.as_str().map(TableName::new))
        })
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

    /// get the parameters as T if possible
    pub fn get_parameters_as<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        /*
               let cleanup_params: CleanupParams = serde_json::from_str(params)
           .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to parse parameters: {}", e)))?;

        */
        match &self.parameters {
            Some(params) => serde_json::from_str(params).ok(),
            None => None,
        }
    }

    /// Generate TableDefinition for system.jobs
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
                "job_id",
                1,
                KalamDataType::Text,
                false,
                true,
                false,
                ColumnDefault::None,
                Some("Unique job identifier".to_string()),
            ),
            ColumnDefinition::new(
                2,
                "job_type",
                2,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Type of job (Flush, Compact, Cleanup, Backup, Restore)".to_string()),
            ),
            ColumnDefinition::new(
                3,
                "status",
                3,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Job status (New, Queued, Running, Completed, Failed, Cancelled, Retrying)".to_string()),
            ),
            ColumnDefinition::new(
                4,
                "leader_status",
                4,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Status of leader-only actions".to_string()),
            ),
            ColumnDefinition::new(
                5,
                "parameters",
                5,
                KalamDataType::Json,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("JSON object containing job parameters".to_string()),
            ),
            ColumnDefinition::new(
                6,
                "message",
                6,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Result or error message".to_string()),
            ),
            ColumnDefinition::new(
                7,
                "exception_trace",
                7,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Full stack trace on failures".to_string()),
            ),
            ColumnDefinition::new(
                8,
                "idempotency_key",
                8,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Key for preventing duplicate jobs".to_string()),
            ),
            ColumnDefinition::new(
                9,
                "queue",
                9,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Queue name for job routing".to_string()),
            ),
            ColumnDefinition::new(
                10,
                "priority",
                10,
                KalamDataType::Int,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Priority value (higher = more priority)".to_string()),
            ),
            ColumnDefinition::new(
                11,
                "retry_count",
                11,
                KalamDataType::SmallInt,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Number of retries attempted".to_string()),
            ),
            ColumnDefinition::new(
                12,
                "max_retries",
                12,
                KalamDataType::SmallInt,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Maximum retries allowed".to_string()),
            ),
            ColumnDefinition::new(
                13,
                "memory_used",
                13,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Memory usage in bytes".to_string()),
            ),
            ColumnDefinition::new(
                14,
                "cpu_used",
                14,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("CPU time in microseconds".to_string()),
            ),
            ColumnDefinition::new(
                15,
                "created_at",
                15,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Unix timestamp in milliseconds when job was created".to_string()),
            ),
            ColumnDefinition::new(
                16,
                "updated_at",
                16,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Unix timestamp in milliseconds when job was last updated".to_string()),
            ),
            ColumnDefinition::new(
                17,
                "started_at",
                17,
                KalamDataType::Timestamp,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Unix timestamp in milliseconds when job started".to_string()),
            ),
            ColumnDefinition::new(
                18,
                "finished_at",
                18,
                KalamDataType::Timestamp,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Unix timestamp in milliseconds when job completed".to_string()),
            ),
            ColumnDefinition::new(
                19,
                "node_id",
                19,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Node/server that owns this job".to_string()),
            ),
            ColumnDefinition::new(
                20,
                "leader_node_id",
                20,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Node that performed leader actions".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new(SystemTable::Jobs.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Background jobs for maintenance and data management".to_string()),
        )
        .expect("Failed to create system.jobs table definition")
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

/// Sort order for job queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    Asc,
    Desc,
}

/// Fields to sort jobs by
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobSortField {
    CreatedAt,
    UpdatedAt,
    Priority,
}

/// Filter criteria for job queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobFilter {
    /// Filter by job type
    pub job_type: Option<JobType>,
    /// Filter by job status (single) - Deprecated, use statuses
    pub status: Option<JobStatus>,
    /// Filter by multiple job statuses
    pub statuses: Option<Vec<JobStatus>>,
    /// Filter by idempotency key
    pub idempotency_key: Option<String>,
    /// Limit number of results
    pub limit: Option<usize>,
    /// Start from created_at timestamp (inclusive)
    pub created_after: Option<i64>,
    /// End at created_at timestamp (exclusive)
    pub created_before: Option<i64>,
    /// Sort field
    pub sort_by: Option<JobSortField>,
    /// Sort order
    pub sort_order: Option<SortOrder>,
}

impl Default for JobFilter {
    fn default() -> Self {
        Self {
            job_type: None,
            status: None,
            statuses: None,
            idempotency_key: None,
            limit: Some(100),
            created_after: None,
            created_before: None,
            sort_by: None,
            sort_order: None,
        }
    }
}

// KSerializable implementation for EntityStore support
impl KSerializable for Job {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn test_job_serialization() {
        let job = Job {
            job_id: "job_123".into(),
            job_type: JobType::Flush,
            status: JobStatus::Completed,
            leader_status: Some(JobStatus::Completed),
            parameters: Some(r#"{"namespace_id":"default","table_name":"events"}"#.to_string()),
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
            node_id: NodeId::from(1u64),
            leader_node_id: Some(NodeId::from(1u64)),
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
            status: JobStatus::Running,
            leader_status: None,
            parameters: Some(r#"{"namespace_id":"default","table_name":"events"}"#.to_string()),
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
            node_id: NodeId::from(1u64),
            leader_node_id: None,
            queue: None,
            priority: None,
        };

        let cancelled = job.cancel();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert!(cancelled.finished_at.is_some());
    }
}
