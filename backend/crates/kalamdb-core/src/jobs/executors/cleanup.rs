//! Cleanup Job Executor
//!
//! **Phase 9 (T147)**: JobExecutor implementation for cleanup operations
//!
//! Handles cleanup of deleted table data and metadata.
//!
//! ## Responsibilities
//! - Clean up soft-deleted table data
//! - Remove orphaned Parquet files
//! - Clean up metadata from system tables
//! - Track cleanup metrics (files deleted, bytes freed)
//!
//! ## Parameters Format
//! ```json
//! {
//!   "table_id": "default:users",
//!   "table_type": "User",
//!   "operation": "drop_table"
//! }
//! ```

use crate::jobs::executors::{JobContext, JobDecision, JobExecutor};
use crate::error::KalamDbError;
use crate::sql::executor::handlers::table::drop::{
    cleanup_table_data_internal, cleanup_parquet_files_internal, cleanup_metadata_internal,
};
use async_trait::async_trait;
use kalamdb_commons::system::Job;
use kalamdb_commons::{JobType, TableType};
use std::sync::Arc;

/// Cleanup Job Executor
///
/// Executes cleanup operations for deleted tables and orphaned data.
pub struct CleanupExecutor;

impl CleanupExecutor {
    /// Create a new CleanupExecutor
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl JobExecutor for CleanupExecutor {
    fn job_type(&self) -> JobType {
        JobType::Cleanup
    }

    fn name(&self) -> &'static str {
        "CleanupExecutor"
    }

    async fn validate_params(&self, job: &Job) -> Result<(), KalamDbError> {
        let params = job
            .parameters
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("Missing parameters".to_string()))?;

        let params_obj: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid JSON parameters: {}", e)))?;

        // Validate required fields
        if params_obj.get("table_id").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: table_id".to_string(),
            ));
        }
        if params_obj.get("table_type").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: table_type".to_string(),
            ));
        }
        if params_obj.get("operation").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: operation".to_string(),
            ));
        }

        Ok(())
    }

    async fn execute(&self, ctx: &JobContext, job: &Job) -> Result<JobDecision, KalamDbError> {
        ctx.log_info("Starting cleanup operation");

        // Validate parameters
        self.validate_params(job).await?;

        // Parse parameters
        let params = job.parameters.as_ref().unwrap();
        let params_obj: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to parse parameters: {}", e)))?;

        let table_id_str = params_obj["table_id"].as_str().ok_or_else(|| {
            KalamDbError::InvalidOperation("table_id must be a string".to_string())
        })?;
        let table_type_str = params_obj["table_type"].as_str().ok_or_else(|| {
            KalamDbError::InvalidOperation("table_type must be a string".to_string())
        })?;
        let operation = params_obj["operation"].as_str().unwrap();

        // Parse table_type
        let table_type = match table_type_str {
            "User" => TableType::User,
            "Shared" => TableType::Shared,
            "Stream" => TableType::Stream,
            "System" => TableType::System,
            _ => {
                return Err(KalamDbError::InvalidOperation(
                    format!("Unknown table type: {}", table_type_str)
                ));
            }
        };

        // Parse table_id (format: "namespace:table_name")
        let parts: Vec<&str> = table_id_str.split(':').collect();
        if parts.len() != 2 {
            return Err(KalamDbError::InvalidOperation(
                format!("Invalid table_id format: {}", table_id_str)
            ));
        }
        let namespace_id = kalamdb_commons::NamespaceId::from(parts[0].to_string());
        let table_name = kalamdb_commons::TableName::from(parts[1].to_string());
        let table_id = Arc::new(kalamdb_commons::TableId::new(namespace_id, table_name));

        ctx.log_info(&format!(
            "Cleaning up table {} (operation: {}, type: {:?})",
            table_id_str, operation, table_type
        ));

        // Execute cleanup in 3 phases:
        // 1. Clean up table data (rows) from RocksDB stores
        let rows_deleted = cleanup_table_data_internal(
            &ctx.app_ctx,
            &table_id,
            table_type.clone(),
        ).await?;

        ctx.log_info(&format!("Cleaned up {} rows from table data", rows_deleted));

        // 2. Clean up Parquet files from storage backend
        let bytes_freed = cleanup_parquet_files_internal(
            &ctx.app_ctx,
            &table_id,
        ).await?;

        ctx.log_info(&format!("Freed {} bytes from Parquet files", bytes_freed));

        // 3. Clean up metadata from SchemaRegistry
        let schema_registry = ctx.app_ctx.schema_registry();
        cleanup_metadata_internal(&schema_registry, &table_id).await?;

        ctx.log_info("Removed table metadata from SchemaRegistry");

        // Build success message with metrics
        let message = format!(
            "Cleaned up table {} successfully - {} rows deleted, {} bytes freed",
            table_id_str, rows_deleted, bytes_freed
        );

        ctx.log_info(&message);

        Ok(JobDecision::Completed {
            message: Some(message),
        })
    }

    async fn cancel(&self, ctx: &JobContext, _job: &Job) -> Result<(), KalamDbError> {
        ctx.log_warn("Cleanup job cancellation requested");
        // Cleanup jobs should complete to avoid orphaned data
        Err(KalamDbError::InvalidOperation(
            "Cleanup jobs cannot be safely cancelled".to_string(),
        ))
    }
}

impl Default for CleanupExecutor {
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
            node_id: NodeId::default_node(),
            queue: None,
            priority: None,
        }
    }

    #[tokio::test]
    async fn test_validate_params_success() {
        let executor = CleanupExecutor::new();

        let mut job = make_job("CL-test123", JobType::Cleanup, "default");
        job.parameters = Some(
            serde_json::json!({
                "table_id": "default:users",
                "table_type": "User",
                "operation": "drop_table"
            })
            .to_string(),
        );

        assert!(executor.validate_params(&job).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_params_missing_table_id() {
        let executor = CleanupExecutor::new();

        let mut job = make_job("CL-test123", JobType::Cleanup, "default");
        job.parameters = Some(
            serde_json::json!({
                "table_type": "User",
                "operation": "drop_table"
            })
            .to_string(),
        );

        let result = executor.validate_params(&job).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("table_id"));
    }

    #[tokio::test]
    async fn test_validate_params_missing_table_type() {
        let executor = CleanupExecutor::new();

        let mut job = make_job("CL-test123", JobType::Cleanup, "default");
        job.parameters = Some(
            serde_json::json!({
                "table_id": "default:users",
                "operation": "drop_table"
            })
            .to_string(),
        );

        let result = executor.validate_params(&job).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("table_type"));
    }

    #[tokio::test]
    async fn test_validate_params_missing_operation() {
        let executor = CleanupExecutor::new();

        let mut job = make_job("CL-test123", JobType::Cleanup, "default");
        job.parameters = Some(
            serde_json::json!({
                "table_id": "default:users",
                "table_type": "User"
            })
            .to_string(),
        );

        let result = executor.validate_params(&job).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("operation"));
    }

    #[tokio::test]
    async fn test_validate_params_missing_all_params() {
        let executor = CleanupExecutor::new();

        let mut job = make_job("CL-test123", JobType::Cleanup, "default");
        job.parameters = None;

        let result = executor.validate_params(&job).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Missing parameters"));
    }

    #[tokio::test]
    async fn test_validate_params_invalid_json() {
        let executor = CleanupExecutor::new();

        let mut job = make_job("CL-test123", JobType::Cleanup, "default");
        job.parameters = Some("{invalid json}".to_string());

        let result = executor.validate_params(&job).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_executor_properties() {
        let executor = CleanupExecutor::new();
        assert_eq!(executor.job_type(), JobType::Cleanup);
        assert_eq!(executor.name(), "CleanupExecutor");
    }

    #[test]
    fn test_default_constructor() {
        let executor = CleanupExecutor::default();
        assert_eq!(executor.job_type(), JobType::Cleanup);
    }
}
