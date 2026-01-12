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
//! ## Non-Blocking Execution
//! All flush operations are executed via `tokio::task::spawn_blocking` to avoid
//! blocking the async runtime. This is critical because:
//! - RocksDB operations are synchronous and can take 10-100ms+
//! - Parquet file writes involve I/O that can block
//! - High flush concurrency could starve the async executor
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
use crate::error_extensions::KalamDbResultExt;
use crate::jobs::executors::{JobContext, JobDecision, JobExecutor, JobParams};
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
        // Parameters already validated in JobContext - type-safe access
        let params = ctx.params();
        let table_id = Arc::new(params.table_id.clone());
        let table_type = params.table_type;

        ctx.log_debug(&format!("Flushing {} (type: {:?})", table_id, table_type));

        // Get dependencies from AppContext
        let app_ctx = &ctx.app_ctx;
        let schema_registry = app_ctx.schema_registry();
        let live_query_manager = app_ctx.live_query_manager();

        // // Get table definition and schema
        // let table_def = schema_registry
        //     .get_table_definition(&table_id)?
        //     .ok_or_else(|| KalamDbError::NotFound(format!("Table {} not found", table_id)))?;
        
        // Get current Arrow schema from the registry (already includes system columns)
        let schema = schema_registry.get_arrow_schema(&table_id)
            .into_kalamdb_error(&format!("Arrow schema not found for {}", table_id))?;

        // Get current schema version for manifest recording
        // Phase 16: Will be used when writing SegmentMetadata.schema_version
        // let _current_schema_version = table_def.schema_version;

        // Execute flush based on table type
        // Use spawn_blocking to avoid blocking the async runtime during RocksDB I/O
        let result = match table_type {
            TableType::User => {
                ctx.log_debug("Executing UserTableFlushJob (non-blocking)");

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
                )
                .with_live_query_manager(live_query_manager);

                // Execute in blocking thread pool to avoid starving async runtime
                tokio::task::spawn_blocking(move || flush_job.execute())
                    .await
                    .map_err(|e| KalamDbError::InvalidOperation(format!("Flush task panicked: {}", e)))?
                    .into_kalamdb_error("User table flush failed")?
            }
            TableType::Shared => {
                ctx.log_debug("Executing SharedTableFlushJob (non-blocking)");

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
                )
                .with_live_query_manager(live_query_manager);

                // Execute in blocking thread pool to avoid starving async runtime
                tokio::task::spawn_blocking(move || flush_job.execute())
                    .await
                    .map_err(|e| KalamDbError::InvalidOperation(format!("Flush task panicked: {}", e)))?
                    .into_kalamdb_error("Shared table flush failed")?
            }
            TableType::Stream => {
                ctx.log_debug("Stream table flush not yet implemented");
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

        ctx.log_debug(&format!(
            "Flush operation completed: {} rows flushed, {} files created",
            result.rows_flushed,
            result.parquet_files.len()
        ));

        // Compact RocksDB partition after flush to reclaim space from tombstones
        ctx.log_debug("Running RocksDB compaction to clean up tombstones...");
        let backend = app_ctx.storage_backend();
        let partition_name = match table_type {
            TableType::User => {
                use kalamdb_commons::constants::ColumnFamilyNames;
                format!(
                    "{}{}",
                    ColumnFamilyNames::USER_TABLE_PREFIX,
                    table_id // TableId Display: "namespace:table"
                )
            }
            TableType::Shared => {
                use kalamdb_commons::constants::ColumnFamilyNames;
                format!(
                    "{}{}",
                    ColumnFamilyNames::SHARED_TABLE_PREFIX,
                    table_id // TableId Display: "namespace:table"
                )
            }
            _ => {
                // For Stream/System tables, skip compaction
                return Ok(JobDecision::Completed {
                    message: Some(format!(
                        "Flushed {} successfully ({} rows, {} files)",
                        table_id, result.rows_flushed, result.parquet_files.len()
                    )),
                });
            }
        };

        use kalamdb_store::storage_trait::Partition;
        let partition = Partition::new(partition_name);
        
        // Run RocksDB compaction in blocking thread pool to avoid blocking async runtime
        let compact_result = tokio::task::spawn_blocking(move || {
            backend.compact_partition(&partition)
        })
        .await
        .map_err(|e| KalamDbError::InvalidOperation(format!("Compaction task panicked: {}", e)))?;

        match compact_result {
            Ok(()) => {
                ctx.log_debug("RocksDB compaction completed successfully");
            }
            Err(e) => {
                // Log compaction failure but don't fail the flush job
                ctx.log_warn(&format!("RocksDB compaction failed (non-critical): {}", e));
            }
        }

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
