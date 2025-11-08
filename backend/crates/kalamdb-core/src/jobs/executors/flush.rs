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
use kalamdb_commons::system::Job;
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
        // Normalize table_type (handle lowercase stored values like "user")
        let table_type_raw = params_obj["table_type"].as_str().unwrap();
        let table_type = match kalamdb_commons::schemas::TableType::from_str(table_type_raw) {
            Some(tt) => tt, // use enum for matching
            None => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Unknown table type: {}",
                    table_type_raw
                )));
            }
        };

        ctx.log_info(&format!(
            "Flushing {}.{} (type: {})",
            namespace_id_str, table_name_str, table_type.as_str()
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
            kalamdb_commons::schemas::TableType::User => {
                ctx.log_info("Executing UserTableFlushJob");

                // IMPORTANT: Use the per-table UserTableStore (created at table registration)
                // instead of the generic prefix-only user_table_store() created in AppContext.
                // The generic store points to partition "user_" (no namespace/table suffix) and
                // cannot see actual row data stored under per-table partitions like
                // "user_<namespace>:<table>". Using it caused runtime errors:
                //   Not found: user_
                // Retrieve the UserTableShared instance to access the correct store.
                let user_shared = schema_registry
                    .get_user_table_shared(&table_id)
                    .ok_or_else(|| KalamDbError::NotFound(format!(
                        "User table provider not registered for {}.{} (id={})",
                        namespace_id_str, table_name_str, table_id
                    )))?;
                let store = user_shared.store().clone();

                let flush_job = UserTableFlushJob::new(
                    table_id.clone(),
                    store,
                    namespace_id.clone(),
                    table_name.clone(),
                    schema.clone(),
                    schema_cache.clone(),
                )
                .with_live_query_manager(live_query_manager);

                flush_job
                    .execute()
                    .map_err(|e| KalamDbError::Other(format!("User table flush failed: {}", e)))?
            }
            kalamdb_commons::schemas::TableType::Shared => {
                ctx.log_info("Executing SharedTableFlushJob");

                // Use a per-table SharedTableStore for the specific <namespace, table>
                // to ensure we read the correct partition ("shared_<ns>:<table>").
                let store = Arc::new(
                    crate::tables::shared_tables::shared_table_store::new_shared_table_store(
                        app_ctx.storage_backend(),
                        &namespace_id,
                        &table_name,
                    ),
                );

                let flush_job = SharedTableFlushJob::new(
                    table_id.clone(),
                    store,
                    namespace_id.clone(),
                    table_name.clone(),
                    schema.clone(),
                    schema_cache.clone(),
                )
                .with_live_query_manager(live_query_manager);

                flush_job
                    .execute()
                    .map_err(|e| KalamDbError::Other(format!("Shared table flush failed: {}", e)))?
            }
            kalamdb_commons::schemas::TableType::Stream => {
                ctx.log_info("Stream table flush not yet implemented");
                // Streams: return Completed (no-op) for idempotency and clarity
                return Ok(JobDecision::Completed {
                    message: Some(format!(
                        "Stream flush skipped (not implemented) for {}.{}",
                        namespace_id_str, table_name_str
                    )),
                });
            }
            kalamdb_commons::schemas::TableType::System => {
                return Err(KalamDbError::InvalidOperation("Cannot flush SYSTEM tables".to_string()));
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
        let executor = FlushExecutor::new();

        let job = make_job("FL-test123", JobType::Flush, "default");

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

        let job = make_job("FL-test123", JobType::Flush, "default");

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
