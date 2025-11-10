//! Typed DDL handler for DROP TABLE statements
//!
//! This module provides both the DROP TABLE handler and reusable cleanup functions
//! for table deletion operations (used by both DDL handler and CleanupExecutor).

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::SchemaRegistry;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableType;
use kalamdb_sql::ddl::DropTableStatement;
use std::sync::Arc;

/// Cleanup helper function: Delete table data from RocksDB
///
/// **Phase 8.5 (T146a)**: Public helper for CleanupExecutor
///
/// # Arguments
/// * `_app_context` - Application context for accessing stores (reserved for future use)
/// * `table_id` - Table identifier (namespace:table_name)
/// * `table_type` - Table type (User/Shared/Stream)
///
/// # Returns
/// Number of rows deleted
pub async fn cleanup_table_data_internal(
    _app_context: &Arc<AppContext>,
    table_id: &TableId,
    table_type: TableType,
) -> Result<usize, KalamDbError> {
    log::info!("[CleanupHelper] Cleaning up table data for {:?} (type: {:?})", table_id, table_type);

    let rows_deleted = match table_type {
        TableType::User => {
            // TODO: Implement delete_table_data() in UserTableStore
            // For now, return 0 (will be implemented when store.scan_iter() is added)
            log::warn!("[CleanupHelper] UserTable data cleanup not yet implemented");
            0
        }
        TableType::Shared => {
            // TODO: Implement delete_table_data() in SharedTableStore
            log::warn!("[CleanupHelper] SharedTable data cleanup not yet implemented");
            0
        }
        TableType::Stream => {
            // TODO: Implement delete_table_data() in StreamTableStore
            log::warn!("[CleanupHelper] StreamTable data cleanup not yet implemented");
            0
        }
        TableType::System => {
            // System tables cannot be dropped via DDL
            return Err(KalamDbError::InvalidOperation(
                "Cannot cleanup system table data".to_string()
            ));
        }
    };

    log::info!("[CleanupHelper] Deleted {} rows from table data", rows_deleted);
    Ok(rows_deleted)
}

/// Cleanup helper function: Delete Parquet files from storage backend
///
/// **Phase 8.5 (T146a)**: Public helper for CleanupExecutor
///
/// # Arguments
/// * `_app_context` - Application context for accessing storage backend (reserved for future use)
/// * `table_id` - Table identifier (namespace:table_name)
///
/// # Returns
/// Number of bytes freed (sum of deleted file sizes)
pub async fn cleanup_parquet_files_internal(
    _app_context: &Arc<AppContext>,
    table_id: &TableId,
) -> Result<u64, KalamDbError> {
    log::info!("[CleanupHelper] Cleaning up Parquet files for {:?}", table_id);

    // Note: Actual implementation would:
    // 1. Get storage backend from AppContext
    // 2. List all files matching pattern {namespace}/{table_name}/*.parquet
    // 3. Get file sizes before deletion
    // 4. Delete each file
    // 5. Sum total bytes freed
    // For now, return 0 as placeholder
    let bytes_freed = 0u64;

    log::info!("[CleanupHelper] Freed {} bytes from Parquet files", bytes_freed);
    Ok(bytes_freed)
}

/// Cleanup helper function: Remove table metadata from system tables
///
/// **Phase 8.5 (T146a)**: Public helper for CleanupExecutor
///
/// # Arguments
/// * `schema_registry` - Schema registry for metadata removal
/// * `table_id` - Table identifier (namespace:table_name)
///
/// # Returns
/// Ok(()) on success
pub async fn cleanup_metadata_internal(
    schema_registry: &Arc<SchemaRegistry>,
    table_id: &TableId,
) -> Result<(), KalamDbError> {
    log::info!("[CleanupHelper] Cleaning up metadata for {:?}", table_id);

    // Delete table definition from SchemaRegistry
    // This removes from both cache and persistent store (delete-through pattern)
    schema_registry.delete_table_definition(table_id)?;

    log::info!("[CleanupHelper] Metadata cleanup complete");
    Ok(())
}

/// Typed handler for DROP TABLE statements
pub struct DropTableHandler {
    app_context: Arc<AppContext>,
}

impl DropTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropTableStatement> for DropTableHandler {
    async fn execute(
        &self,
        statement: DropTableStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let table_id = TableId::from_strings(
            statement.namespace_id.as_str(),
            statement.table_name.as_str(),
        );

        log::info!(
            "🗑️  DROP TABLE request: {}.{} (if_exists: {}, user: {}, role: {:?})",
            statement.namespace_id.as_str(),
            statement.table_name.as_str(),
            statement.if_exists,
            context.user_id.as_str(),
            context.user_role
        );

        // RBAC: authorize based on actual table type if exists
        let registry = self.app_context.schema_registry();
        let actual_type = match registry.get_table_definition(&table_id)? {
            Some(def) => def.table_type,
            None => TableType::from(statement.table_type),
        };
        let is_owner = false;
        if !crate::auth::rbac::can_delete_table(context.user_role, actual_type, is_owner) {
            log::error!(
                "❌ DROP TABLE {}.{}: Insufficient privileges (user: {}, role: {:?}, table_type: {:?})",
                statement.namespace_id.as_str(),
                statement.table_name.as_str(),
                context.user_id.as_str(),
                context.user_role,
                actual_type
            );
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop this table".to_string(),
            ));
        }

        // Check existence via system.tables provider (for IF EXISTS behavior)
        let tables = self.app_context.system_tables().tables();
        let table_metadata = tables.get_table_by_id(&table_id)?;
        let exists = table_metadata.is_some();
        
        if !exists {
            if statement.if_exists {
                log::info!(
                    "ℹ️  DROP TABLE {}.{}: Table does not exist (IF EXISTS - skipping)",
                    statement.namespace_id.as_str(),
                    statement.table_name.as_str()
                );
                return Ok(ExecutionResult::Success { message: format!(
                    "Table {}.{} does not exist (skipped)",
                    statement.namespace_id.as_str(),
                    statement.table_name.as_str()
                )});
            } else {
                log::warn!(
                    "⚠️  DROP TABLE failed: Table '{}' not found in namespace '{}'",
                    statement.table_name.as_str(),
                    statement.namespace_id.as_str()
                );
                return Err(KalamDbError::NotFound(format!(
                    "Table '{}' not found in namespace '{}'",
                    statement.table_name.as_str(),
                    statement.namespace_id.as_str()
                )));
            }
        }

        // Log table details before dropping
        if let Some(metadata) = table_metadata {
            log::debug!(
                "📋 Dropping table: type={:?}, columns={}, created_at={:?}",
                actual_type,
                metadata.columns.len(),
                metadata.created_at
            );
        }

        // Cancel any active flush jobs for this table before dropping
        let job_manager = self.app_context.job_manager();
        let flush_filter = kalamdb_commons::system::JobFilter {
            job_type: Some(kalamdb_commons::JobType::Flush),
            status: None, // Check all non-completed statuses
            namespace_id: Some(statement.namespace_id.clone()),
            table_name: Some(statement.table_name.clone()),
            idempotency_key: None,
            limit: None,
            created_after: None,
            created_before: None,
        };
        
        let flush_jobs = job_manager.list_jobs(flush_filter).await?;
        let mut cancelled_count = 0;
        
        for job in flush_jobs {
            // Only cancel jobs that are not already completed/failed/cancelled
            if matches!(
                job.status,
                kalamdb_commons::JobStatus::New 
                | kalamdb_commons::JobStatus::Queued 
                | kalamdb_commons::JobStatus::Running
            ) {
                match job_manager.cancel_job(&job.job_id).await {
                    Ok(_) => {
                        log::info!(
                            "🛑 Cancelled flush job {} for table {}.{} before DROP",
                            job.job_id,
                            statement.namespace_id.as_str(),
                            statement.table_name.as_str()
                        );
                        cancelled_count += 1;
                    }
                    Err(e) => {
                        log::warn!(
                            "⚠️  Failed to cancel flush job {} for table {}.{}: {}",
                            job.job_id,
                            statement.namespace_id.as_str(),
                            statement.table_name.as_str(),
                            e
                        );
                    }
                }
            }
        }
        
        if cancelled_count > 0 {
            log::info!(
                "🛑 Cancelled {} active flush job(s) for table {}.{}",
                cancelled_count,
                statement.namespace_id.as_str(),
                statement.table_name.as_str()
            );
        }

        // TODO: Check active live queries/subscriptions before dropping (Phase 9 integration)

        // Unregister provider from SchemaRegistry (auto-unregisters from DataFusion)
        use crate::sql::executor::helpers::table_registration::unregister_table_provider;
        unregister_table_provider(&self.app_context, &table_id)?;

        // Remove definition via SchemaRegistry (delete-through) → invalidates cache
        registry.delete_table_definition(&table_id)?;

        log::info!(
            "✅ DROP TABLE succeeded: {}.{} (type: {:?})",
            statement.namespace_id.as_str(),
            statement.table_name.as_str(),
            actual_type
        );

        Ok(ExecutionResult::Success {
            message: format!(
                "Table {}.{} dropped successfully",
                statement.namespace_id.as_str(),
                statement.table_name.as_str()
            ),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &DropTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Coarse auth gate (fine-grained check performed in execute using actual table type)
        if context.is_system() || context.is_admin() {
            return Ok(());
        }
        // Allow users to attempt; execute() will enforce per-table RBAC
        Ok(())
    }
}
