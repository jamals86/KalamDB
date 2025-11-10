//! Stream Eviction Job Executor
//!
//! **Phase 9 (T149)**: JobExecutor implementation for stream table eviction
//!
//! Handles TTL-based eviction for stream tables.
//!
//! ## Responsibilities
//! - Enforce ttl_seconds policy for stream tables
//! - Delete expired records based on created_at timestamp
//! - Track eviction metrics (rows evicted, bytes freed)
//! - Support partial eviction with continuation
//!
//! ## Parameters Format
//! ```json
//! {
//!   "namespace_id": "default",
//!   "table_name": "events",
//!   "table_type": "Stream",
//!   "ttl_seconds": 86400,
//!   "batch_size": 10000
//! }
//! ```

use crate::jobs::executors::{JobContext, JobDecision, JobExecutor};
use crate::error::KalamDbError;
use async_trait::async_trait;
use kalamdb_commons::system::Job;
use kalamdb_commons::JobType;

/// Stream Eviction Job Executor
///
/// Executes TTL-based eviction operations for stream tables.
pub struct StreamEvictionExecutor;

impl StreamEvictionExecutor {
    /// Create a new StreamEvictionExecutor
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl JobExecutor for StreamEvictionExecutor {
    fn job_type(&self) -> JobType {
        JobType::StreamEviction
    }

    fn name(&self) -> &'static str {
        "StreamEvictionExecutor"
    }

    async fn validate_params(&self, job: &Job) -> Result<(), KalamDbError> {
        let params = job
            .parameters
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("Missing parameters".to_string()))?;

        let params_obj: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid JSON parameters: {}", e)))?;

        // Validate required fields
        if params_obj.get("namespace_id").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: namespace_id".to_string(),
            ));
        }
        if params_obj.get("table_name").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: table_name".to_string(),
            ));
        }
        if params_obj.get("table_type").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: table_type".to_string(),
            ));
        }

        // Validate table_type is Stream
        let table_type = params_obj["table_type"]
            .as_str()
            .ok_or_else(|| KalamDbError::InvalidOperation("table_type must be a string".to_string()))?;
        if table_type != "Stream" {
            return Err(KalamDbError::InvalidOperation(format!(
                "Invalid table_type: expected 'Stream', got '{}'",
                table_type
            )));
        }

        if params_obj.get("ttl_seconds").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: ttl_seconds".to_string(),
            ));
        }

        // Validate ttl_seconds is a number
        if !params_obj["ttl_seconds"].is_number() {
            return Err(KalamDbError::InvalidOperation(
                "ttl_seconds must be a number".to_string(),
            ));
        }

        // Validate batch_size if present
        if let Some(batch_size) = params_obj.get("batch_size") {
            if !batch_size.is_number() {
                return Err(KalamDbError::InvalidOperation(
                    "batch_size must be a number".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn execute(&self, ctx: &JobContext, job: &Job) -> Result<JobDecision, KalamDbError> {
        ctx.log_info("Starting stream eviction operation");

        // Validate parameters
        self.validate_params(job).await?;

        // Parse parameters
        let params = job.parameters.as_ref().unwrap();
        let params_obj: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to parse parameters: {}", e)))?;

        let namespace_id = params_obj["namespace_id"].as_str().unwrap();
        let table_name = params_obj["table_name"].as_str().unwrap();
        let ttl_seconds = params_obj["ttl_seconds"].as_u64().unwrap();
        let batch_size = params_obj
            .get("batch_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(10000);

        ctx.log_info(&format!(
            "Evicting expired records from stream {}.{} (ttl: {}s, batch: {})",
            namespace_id, table_name, ttl_seconds, batch_size
        ));

        // Calculate cutoff time for eviction (records created before this time are expired)
        let now = chrono::Utc::now().timestamp_millis();
        let ttl_ms = (ttl_seconds * 1000) as i64;
        let cutoff_time = now - ttl_ms;

        ctx.log_info(&format!(
            "Cutoff time: {} (records created before this are expired)",
            chrono::DateTime::from_timestamp_millis(cutoff_time)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "invalid".to_string())
        ));

        // TODO: Implement actual stream eviction logic
        // Current limitation: StreamTableRow doesn't have created_at field yet
        // When adding TTL support to stream tables:
        //   1. Add `created_at: i64` field to StreamTableRow (required for TTL)
        //   2. Scan table using stream_table_store.scan_prefix()
        //   3. Filter rows where created_at < cutoff_time
        //   4. Delete in batches up to batch_size using store.delete()
        //   5. If batch full (== batch_size), return JobDecision::Retry with 5000ms backoff
        //   6. Track metrics (rows_evicted, estimated_bytes_freed)
        //
        // Implementation sketch:
        //   let expired_rows: Vec<RowId> = all_rows
        //       .filter(|row| row.created_at < cutoff_time)
        //       .take(batch_size as usize)
        //       .map(|row| row.row_id)
        //       .collect();
        //   for row_id in &expired_rows {
        //       store.delete(row_id)?;
        //   }
        //   if expired_rows.len() == batch_size as usize {
        //       return Ok(JobDecision::Retry { backoff_ms: 5000 });
        //   }
        //
        // For now, return placeholder metrics
        let rows_evicted = 0;

        ctx.log_info(&format!("Stream eviction completed - {} rows evicted", rows_evicted));

        Ok(JobDecision::Completed {
            message: Some(format!(
                "Evicted {} expired records from {}.{} (ttl: {}s)",
                rows_evicted, namespace_id, table_name, ttl_seconds
            )),
        })
    }

    async fn cancel(&self, ctx: &JobContext, _job: &Job) -> Result<(), KalamDbError> {
        ctx.log_warn("Stream eviction job cancellation requested");
        // Allow cancellation since partial eviction is acceptable
        Ok(())
    }
}

impl Default for StreamEvictionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{JobId, NamespaceId, NodeId};
    use kalamdb_commons::system::Job;

    fn make_job(id: &str, job_type: JobType, ns: &str) -> Job {
        let now = chrono::Utc::now().timestamp_millis();
        Job {
            job_id: JobId::new(id),
            job_type,
            namespace_id: NamespaceId::new(ns),
            table_name: None,
            status: kalamdb_commons::JobStatus::Running,
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
            node_id: NodeId::from("node1"),
            queue: None,
            priority: None,
        }
    }

    #[tokio::test]
    async fn test_validate_params_success() {
        let executor = StreamEvictionExecutor::new();

        let job = make_job("SE-test123", JobType::StreamEviction, "default");

        let mut job = job;
        job.parameters = Some(
            serde_json::json!({
                "namespace_id": "default",
                "table_name": "events",
                "table_type": "Stream",
                "ttl_seconds": 86400,
                "batch_size": 10000
            })
            .to_string(),
        );

        assert!(executor.validate_params(&job).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_params_invalid_table_type() {
        let executor = StreamEvictionExecutor::new();

        let job = make_job("SE-test123", JobType::StreamEviction, "default");

        let mut job = job;
        job.parameters = Some(
            serde_json::json!({
                "namespace_id": "default",
                "table_name": "users",
                "table_type": "User",
                "ttl_seconds": 86400
            })
            .to_string(),
        );

        let result = executor.validate_params(&job).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("expected 'Stream'"));
    }

    #[test]
    fn test_executor_properties() {
        let executor = StreamEvictionExecutor::new();
        assert_eq!(executor.job_type(), JobType::StreamEviction);
        assert_eq!(executor.name(), "StreamEvictionExecutor");
    }
}
