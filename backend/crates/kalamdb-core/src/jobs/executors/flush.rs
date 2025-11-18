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
use crate::jobs::executors::{JobContext, JobDecision, JobExecutor, JobParams};
use crate::providers::arrow_json_conversion; // For schema_with_system_columns
use crate::providers::flush::{SharedTableFlushJob, TableFlush, UserTableFlushJob};
use async_trait::async_trait;
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::{JobType, TableId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Typed parameters for flush operations (T189)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlushParams {
    /// Table identifier (required)
    #[serde(flatten)]
    pub table_id: TableId,
    /// Table type (required)
    pub table_type: TableType,
    /// Flush threshold in rows (optional, defaults to config)
    #[serde(default)]
    pub flush_threshold: Option<u64>,
}

impl JobParams for FlushParams {
    fn validate(&self) -> Result<(), KalamDbError> {
        // TableId and TableType validation is handled by their constructors
        Ok(())
    }
}

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
    type Params = FlushParams;

    fn job_type(&self) -> JobType {
        JobType::Flush
    }

    fn name(&self) -> &'static str {
        "FlushExecutor"
    }

    async fn execute(&self, ctx: &JobContext<Self::Params>) -> Result<JobDecision, KalamDbError> {
        ctx.log_info("Starting flush operation");

        // Parameters already validated in JobContext - type-safe access
        let params = ctx.params();
        let table_id = Arc::new(params.table_id.clone());
        let table_type = params.table_type;

        ctx.log_info(&format!("Flushing {} (type: {:?})", table_id, table_type));

        // Get dependencies from AppContext
        let app_ctx = &ctx.app_ctx;
        let schema_registry = app_ctx.schema_registry();
        let live_query_manager = app_ctx.live_query_manager();

        // Get table definition and schema
        let table_def = schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Table {} not found", table_id)))?;
        // The authoritative Arrow schema should include system columns already
        // because SystemColumnsService injected them into the TableDefinition.
        // Use schema from schema_history if available to avoid stale cache.
        let base_schema = if let Some(latest) = table_def.schema_history.last() {
            crate::schema_registry::arrow_schema::ArrowSchemaWithOptions::from_json_string(
                &latest.arrow_schema_json,
            )
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to parse Arrow schema from history: {}", e))
            })?
            .schema
        } else {
            schema_registry.get_arrow_schema(&table_id).map_err(|e| {
                KalamDbError::NotFound(format!("Arrow schema not found for {}: {}", table_id, e))
            })?
        };

        // Ensure system columns are present or add if missing (idempotent)
        let schema = arrow_json_conversion::schema_with_system_columns(&base_schema);

        // Execute flush based on table type
        let result = match table_type {
            TableType::User => {
                ctx.log_info("Executing UserTableFlushJob");

                // IMPORTANT: Use the per-table UserTableStore (created at table registration)
                // instead of the generic prefix-only user_table_store() created in AppContext.
                // The generic store points to partition "user_" (no namespace/table suffix) and
                // cannot see actual row data stored under per-table partitions like
                // "user_<namespace>:<table>". Using it caused runtime errors:
                //   Not found: user_
                // Retrieve the UserTableProvider instance to access the correct store.
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::NotFound(format!(
                        "User table provider not registered for {} (id={})",
                        table_id, table_id
                    ))
                })?;

                // Downcast to UserTableProvider to access store
                let provider = provider_arc
                    .as_any()
                    .downcast_ref::<crate::providers::UserTableProvider>()
                    .ok_or_else(|| {
                        KalamDbError::InvalidOperation(
                            "Cached provider type mismatch for user table".into(),
                        )
                    })?;

                let store = provider.store.clone();

                let flush_job = UserTableFlushJob::new(
                    table_id.clone(),
                    store,
                    schema.clone(),
                    schema_registry.clone(),
                    app_ctx.manifest_service(),
                    app_ctx.manifest_cache_service(),
                )
                .with_live_query_manager(live_query_manager);

                flush_job
                    .execute()
                    .map_err(|e| KalamDbError::Other(format!("User table flush failed: {}", e)))?
            }
            TableType::Shared => {
                ctx.log_info("Executing SharedTableFlushJob");

                // Get the SharedTableProvider from the schema registry to reuse the cached store
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::NotFound(format!(
                        "Shared table provider not registered for {} (id={})",
                        table_id, table_id
                    ))
                })?;

                // Downcast to SharedTableProvider to access store
                let provider = provider_arc
                    .as_any()
                    .downcast_ref::<crate::providers::SharedTableProvider>()
                    .ok_or_else(|| {
                        KalamDbError::InvalidOperation(
                            "Cached provider type mismatch for shared table".into(),
                        )
                    })?;

                let store = provider.store.clone();

                let flush_job = SharedTableFlushJob::new(
                    table_id.clone(),
                    store,
                    schema.clone(),
                    schema_registry.clone(),
                    app_ctx.manifest_service(),
                    app_ctx.manifest_cache_service(),
                )
                .with_live_query_manager(live_query_manager);

                flush_job
                    .execute()
                    .map_err(|e| KalamDbError::Other(format!("Shared table flush failed: {}", e)))?
            }
            TableType::Stream => {
                ctx.log_info("Stream table flush not yet implemented");
                // Streams: return Completed (no-op) for idempotency and clarity
                return Ok(JobDecision::Completed {
                    message: Some(format!(
                        "Stream flush skipped (not implemented) for {}",
                        table_id
                    )),
                });
            }
            TableType::System => {
                return Err(KalamDbError::InvalidOperation(
                    "Cannot flush SYSTEM tables".to_string(),
                ));
            }
        };

        ctx.log_info(&format!(
            "Flush operation completed: {} rows flushed, {} files created",
            result.rows_flushed,
            result.parquet_files.len()
        ));

        Ok(JobDecision::Completed {
            message: Some(format!(
                "Flushed {} successfully ({} rows, {} files)",
                table_id,
                result.rows_flushed,
                result.parquet_files.len()
            )),
        })
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
    use kalamdb_commons::NamespaceId;

    #[test]
    fn test_executor_properties() {
        let executor = FlushExecutor::new();
        assert_eq!(executor.job_type(), JobType::Flush);
        assert_eq!(executor.name(), "FlushExecutor");
    }

    #[test]
    fn test_flush_params_validate() {
        let params = FlushParams {
            table_id: TableId::new(
                NamespaceId::new("default"),
                kalamdb_commons::TableName::new("users"),
            ),
            table_type: TableType::User,
            flush_threshold: Some(10000),
        };

        assert!(params.validate().is_ok());
    }
}
