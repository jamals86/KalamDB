//! Flush Job Executor
//!
//! **Phase 9 (T146)**: JobExecutor implementation for flush operations
//!
//! Handles flushing buffered writes from RocksDB to Parquet files.
//!
//! ## Responsibilities
//! - Flush user table data (UserTableFlushJob)
//! - Flush shared table data (SharedTableFlushJob)
//! - Flush stream table data (StreamTableFlushJob)
//! - Track flush metrics (rows flushed, files created, bytes written)
//!
//! ## Parameters Format
//! ```json
//! {
//!   "namespace_id": "default",
//!   "table_name": "users",
//!   "table_type": "User",
//!   "flush_threshold": 10000
//! }
//! ```

use crate::error::KalamDbError;
use crate::jobs::executors::{JobContext, JobDecision, JobExecutor};
use crate::tables::{SharedTableFlushJob, UserTableFlushJob};
use crate::tables::base_flush::TableFlush;
use async_trait::async_trait;
use kalamdb_commons::{JobType, NamespaceId, TableName, TableId};
use std::sync::Arc;

/// Flush Job Executor
///
/// Executes flush operations for buffered table data.
pub struct FlushExecutor;

impl FlushExecutor {
    /// Create a new FlushExecutor
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl JobExecutor for FlushExecutor {
    fn job_type(&self) -> JobType {
        JobType::Flush
    }

    fn name(&self) -> &'static str {
        "FlushExecutor"
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

        Ok(())
    }

    async fn execute(&self, ctx: &JobContext, job: &Job) -> Result<JobDecision, KalamDbError> {
        ctx.log_info("Starting flush operation");

        // Validate parameters
        self.validate_params(job).await?;

        // Parse parameters
        let params = job.parameters.as_ref().unwrap();
        let params_obj: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to parse parameters: {}", e)))?;

        let namespace_id_str = params_obj["namespace_id"].as_str().unwrap();
        let table_name_str = params_obj["table_name"].as_str().unwrap();
        let table_type = params_obj["table_type"].as_str().unwrap();

        ctx.log_info(&format!(
            "Flushing {}.{} (type: {})",
            namespace_id_str, table_name_str, table_type
        ));

        // Get dependencies from AppContext
        let app_ctx = &ctx.app_ctx;
        let schema_cache = app_ctx.schema_cache();
        let schema_registry = app_ctx.schema_registry();
        let live_query_manager = app_ctx.live_query_manager();

        let namespace_id = NamespaceId::new(namespace_id_str.to_string());
        let table_name = TableName::new(table_name_str.to_string());

        // Create TableId for cache lookups
        let table_id = Arc::new(TableId::new(namespace_id.clone(), table_name.clone()));

        // Get table definition and schema
        let _table_def = schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Table {}.{} not found", namespace_id_str, table_name_str)))?;
        let schema = schema_registry
            .get_arrow_schema(&table_id)
            .map_err(|e| KalamDbError::NotFound(format!("Arrow schema not found for {}.{}: {}", namespace_id_str, table_name_str, e)))?;

        // Execute flush based on table type
        let result = match table_type {
            "User" => {
                ctx.log_info("Executing UserTableFlushJob");
                let store = app_ctx.user_table_store();
                let flush_job = UserTableFlushJob::new(
                    table_id.clone(),
                    store,
                    namespace_id.clone(),
                    table_name.clone(),
                    schema.clone(),
                    schema_cache.clone(),
                )
                .with_live_query_manager(live_query_manager);

                flush_job.execute()
                    .map_err(|e| KalamDbError::Other(format!("User table flush failed: {}", e)))?
            }
            "Shared" => {
                ctx.log_info("Executing SharedTableFlushJob");
                let store = app_ctx.shared_table_store();
                let flush_job = SharedTableFlushJob::new(
                    table_id.clone(),
                    store,
                    namespace_id.clone(),
                    table_name.clone(),
                    schema.clone(),
                    schema_cache.clone(),
                )
                .with_live_query_manager(live_query_manager);

                flush_job.execute()
                    .map_err(|e| KalamDbError::Other(format!("Shared table flush failed: {}", e)))?
            }
            "Stream" => {
                ctx.log_info("Stream table flush not yet implemented");
                // TODO: Implement StreamTableFlushJob when stream tables support flush
                return Ok(JobDecision::Completed {
                    message: Some(format!(
                        "Stream flush not yet implemented for {}.{}",
                        namespace_id_str, table_name_str
                    )),
                });
            }
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Unknown table type: {}",
                    table_type
                )));
            }
        };

        ctx.log_info(&format!(
            "Flush operation completed: {} rows flushed, {} files created",
            result.rows_flushed, result.parquet_files.len()
        ));

        Ok(JobDecision::Completed {
            message: Some(format!(
                "Flushed {}.{} successfully ({} rows, {} files)",
                namespace_id_str,
                table_name_str,
                result.rows_flushed,
                result.parquet_files.len()
            )),
        })
    }

    async fn cancel(&self, ctx: &JobContext, _job: &Job) -> Result<(), KalamDbError> {
        ctx.log_warn("Flush job cancellation requested");
        // Flush jobs are typically fast, so cancellation is best-effort
        Ok(())
    }
}

impl Default for FlushExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{JobId, NamespaceId, NodeId};

    #[tokio::test]
    async fn test_validate_params_success() {
        let executor = FlushExecutor::new();

        let job = Job::new(
            JobId::new("FL-test123"),
            JobType::Flush,
            NamespaceId::new("default"),
            NodeId::default_node(),
        );

        let mut job = job;
        job.parameters = Some(
            serde_json::json!({
                "namespace_id": "default",
                "table_name": "users",
                "table_type": "User",
                "flush_threshold": 10000
            })
            .to_string(),
        );

        assert!(executor.validate_params(&job).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_params_missing_fields() {
        let executor = FlushExecutor::new();

        let job = Job::new(
            JobId::new("FL-test123"),
            JobType::Flush,
            NamespaceId::new("default"),
            NodeId::default_node(),
        );

        let mut job = job;
        job.parameters = Some(
            serde_json::json!({
                "namespace_id": "default"
                // Missing table_name and table_type
            })
            .to_string(),
        );

        let result = executor.validate_params(&job).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("table_name"));
    }

    #[test]
    fn test_executor_properties() {
        let executor = FlushExecutor::new();
        assert_eq!(executor.job_type(), JobType::Flush);
        assert_eq!(executor.name(), "FlushExecutor");
    }
}
