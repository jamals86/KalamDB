//! Typed DDL handler for DROP TABLE statements
//!
//! This module provides both the DROP TABLE handler and reusable cleanup functions
//! for table deletion operations (used by both DDL handler and CleanupExecutor).

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::sql::executor::helpers::guards::block_system_namespace_modification;
use crate::jobs::executors::cleanup::{CleanupOperation, CleanupParams, StorageCleanupDetails};
use crate::schema_registry::SchemaRegistry;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::{StorageId, TableId};
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::JobType;
use kalamdb_raft::MetaCommand;
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
    log::info!(
        "[CleanupHelper] Cleaning up table data for {:?} (type: {:?})",
        table_id,
        table_type
    );

    let rows_deleted = match table_type {
        TableType::User => {
            // Fast path: drop the entire RocksDB partition for this user table
            // Partition format mirrors new_user_table_store():
            //   user_{namespace}:{tableName}
            use kalamdb_commons::constants::ColumnFamilyNames;
            use kalamdb_store::storage_trait::Partition as StorePartition;

            let partition_name = format!(
                "{}{}",
                ColumnFamilyNames::USER_TABLE_PREFIX,
                table_id // TableId Display: "namespace:table"
            );

            let backend = _app_context.storage_backend();
            let partition = StorePartition::new(partition_name.clone());

            match backend.drop_partition(&partition) {
                Ok(_) => {
                    log::info!(
                        "[CleanupHelper] Dropped partition '{}' for user table {:?}",
                        partition_name,
                        table_id
                    );
                    0usize // Unknown exact row count after drop
                }
                Err(e) => {
                    // If partition not found, treat as already clean
                    let msg = e.to_string();
                    if msg.to_lowercase().contains("not found") {
                        log::warn!(
                            "[CleanupHelper] Partition '{}' not found during cleanup (already clean)",
                            partition_name
                        );
                        0usize
                    } else {
                        return Err(KalamDbError::Other(format!(
                            "Failed to drop partition '{}' for table {}: {}",
                            partition_name, table_id, e
                        )));
                    }
                }
            }
        }
        TableType::Shared => {
            // Drop the entire RocksDB partition for this shared table
            // Partition format mirrors new_shared_table_store():
            //   shared_{namespace}:{tableName}
            use kalamdb_commons::constants::ColumnFamilyNames;
            use kalamdb_store::storage_trait::Partition as StorePartition;

            let partition_name = format!(
                "{}{}",
                ColumnFamilyNames::SHARED_TABLE_PREFIX,
                table_id // TableId Display: "namespace:table"
            );

            let backend = _app_context.storage_backend();
            let partition = StorePartition::new(partition_name.clone());

            match backend.drop_partition(&partition) {
                Ok(_) => {
                    log::info!(
                        "[CleanupHelper] Dropped partition '{}' for shared table {:?}",
                        partition_name,
                        table_id
                    );
                    0usize
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.to_lowercase().contains("not found") {
                        log::warn!(
                            "[CleanupHelper] Partition '{}' not found during cleanup (already clean)",
                            partition_name
                        );
                        0usize
                    } else {
                        return Err(KalamDbError::Other(format!(
                            "Failed to drop partition '{}' for table {}: {}",
                            partition_name, table_id, e
                        )));
                    }
                }
            }
        }
        TableType::Stream => {
            // Stream tables are in-memory by design. However, if a persistent
            // backend is configured, attempt to drop the partition best-effort.
            use kalamdb_commons::constants::ColumnFamilyNames;
            use kalamdb_store::storage_trait::Partition as StorePartition;

            let partition_name = format!(
                "{}{}",
                ColumnFamilyNames::STREAM_TABLE_PREFIX,
                table_id // TableId Display: "namespace:table"
            );

            let backend = _app_context.storage_backend();
            let partition = StorePartition::new(partition_name.clone());

            match backend.drop_partition(&partition) {
                Ok(_) => {
                    log::info!(
                        "[CleanupHelper] Dropped partition '{}' for stream table {:?}",
                        partition_name,
                        table_id
                    );
                    0usize
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.to_lowercase().contains("not found") {
                        log::debug!(
                            "[CleanupHelper] Stream partition '{}' not found (likely in-memory)",
                            partition_name
                        );
                        0usize
                    } else {
                        return Err(KalamDbError::Other(format!(
                            "Failed to drop partition '{}' for stream table {}: {}",
                            partition_name, table_id, e
                        )));
                    }
                }
            }
        }
        TableType::System => {
            // System tables cannot be dropped via DDL
            return Err(KalamDbError::InvalidOperation(
                "Cannot cleanup system table data".to_string(),
            ));
        }
    };

    log::info!(
        "[CleanupHelper] Deleted {} rows from table data",
        rows_deleted
    );
    Ok(rows_deleted)
}

/// Cleanup helper function: Delete Parquet files from storage backend
///
/// **Phase 8.5 (T146a)**: Public helper for CleanupExecutor
///
/// # Arguments
/// * `app_context` - Application context for accessing storage backend
/// * `table_id` - Table identifier (namespace:table_name)
///
/// # Returns
/// Number of bytes freed (sum of deleted file sizes)
pub async fn cleanup_parquet_files_internal(
    app_context: &Arc<AppContext>,
    table_id: &TableId,
    table_type: TableType,
    storage: &StorageCleanupDetails,
) -> Result<u64, KalamDbError> {
    log::info!(
        "[CleanupHelper] Cleaning up Parquet files for {:?} using storage {}",
        table_id,
        storage.storage_id.as_str()
    );

    // Get the Storage object from the registry (cached lookup)
    let storage_obj = app_context
        .storage_registry()
        .get_storage(&storage.storage_id)?
        .ok_or_else(|| {
            KalamDbError::InvalidOperation(format!(
                "Storage '{}' not found during cleanup",
                storage.storage_id.as_str()
            ))
        })?;

    // Build ObjectStore for this storage
    let object_store = kalamdb_filestore::build_object_store(&storage_obj)
        .into_kalamdb_error("Failed to build object store")?;

    let bytes_freed = kalamdb_filestore::delete_parquet_tree_for_table(
        object_store,
        &storage_obj,
        &storage.relative_path_template,
        table_type,
    )
    .into_kalamdb_error("Filestore delete failed")?;

    log::info!(
        "[CleanupHelper] Freed {} bytes from Parquet files",
        bytes_freed
    );
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

    if !schema_registry.table_exists(table_id)? {
        log::info!(
            "[CleanupHelper] Metadata already removed for {:?}, skipping",
            table_id
        );
        return Ok(());
    }

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

    fn capture_storage_cleanup_details(
        &self,
        table_id: &TableId,
        table_type: TableType,
    ) -> Result<StorageCleanupDetails, KalamDbError> {
        let registry = self.app_context.schema_registry();
        let cached = registry.get(table_id).ok_or_else(|| {
            KalamDbError::InvalidOperation(format!(
                "Table cache entry not found for {}",
                table_id
            ))
        })?;

        let storage_id = cached
            .storage_id
            .clone()
            .unwrap_or_else(StorageId::local);

        let relative_template = if cached.storage_path_template.is_empty() {
            use crate::schema_registry::PathResolver;
            PathResolver::resolve_storage_path_template(table_id, table_type, &storage_id)?
        } else {
            cached.storage_path_template.clone()
        };

        // Get storage from registry (cached lookup)
        let base_dir = match self.app_context.storage_registry().get_storage(&storage_id) {
            Ok(Some(storage)) => {
                let trimmed = storage.base_directory.trim();
                if trimmed.is_empty() {
                    self.app_context
                        .storage_registry()
                        .default_storage_path()
                        .to_string()
                } else {
                    storage.base_directory.clone()
                }
            }
            _ => self
                .app_context
                .storage_registry()
                .default_storage_path()
                .to_string(),
        };

        Ok(StorageCleanupDetails {
            storage_id,
            base_directory: base_dir,
            relative_path_template: relative_template,
        })
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

        // Block DROP on system tables - they are managed internally
        block_system_namespace_modification(
            &statement.namespace_id,
            "DROP",
            "TABLE",
            Some(statement.table_name.as_str()),
        )?;

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
                return Ok(ExecutionResult::Success {
                    message: format!(
                        "Table {}.{} does not exist (skipped)",
                        statement.namespace_id.as_str(),
                        statement.table_name.as_str()
                    ),
                });
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

        let storage_details = self.capture_storage_cleanup_details(&table_id, actual_type)?;

        // Cancel any active flush jobs for this table before dropping
        let job_manager = self.app_context.job_manager();
        let flush_filter = kalamdb_commons::system::JobFilter {
            job_type: Some(kalamdb_commons::JobType::Flush),
            status: None, // Check all non-completed statuses
            idempotency_key: None,
            limit: None,
            created_after: None,
            created_before: None,
            ..Default::default()
        };

        let flush_jobs = job_manager.list_jobs(flush_filter).await?;
        let mut cancelled_count = 0;

        // Filter jobs by namespace and table from parameters
        let target_namespace = statement.namespace_id.clone();
        let target_table = statement.table_name.clone();

        for job in flush_jobs {
            // Check if this job is for the target table (namespace_id and table_name in parameters)
            let matches_table = job.namespace_id().as_ref() == Some(&target_namespace)
                && job.table_name().as_ref() == Some(&target_table);

            if !matches_table {
                continue;
            }

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

        // Mark table as deleted in SchemaRegistry (soft delete)
        registry.delete_table_definition(&table_id)?;

        if self.app_context.executor().is_cluster_mode() {
            let cmd = MetaCommand::DropTable {
                table_id: table_id.clone(),
            };
            self.app_context
                .executor()
                .execute_meta(cmd)
                .await
                .map_err(|e| {
                    KalamDbError::ExecutionError(format!(
                        "Failed to replicate table drop via executor: {}",
                        e
                    ))
                })?;
        } else {
            let tables_provider = self.app_context.system_tables().tables();
            tables_provider.delete_table(&table_id)?;
        }

        // Create cleanup job for async data removal (with retry on failure)
        let params = CleanupParams {
            table_id: table_id.clone(),
            table_type: actual_type,
            operation: CleanupOperation::DropTable,
            storage: storage_details,
        };

        let idempotency_key = format!(
            "drop-{}-{}",
            statement.namespace_id.as_str(),
            statement.table_name.as_str()
        );

        let job_manager = self.app_context.job_manager();
        let job_id = job_manager
            .create_job_typed(
                JobType::Cleanup,
                params,
                Some(idempotency_key),
                None,
            )
            .await?;

        // Log DDL operation
        use crate::sql::executor::helpers::audit;
        let audit_entry = audit::log_ddl_operation(
            context,
            "DROP",
            "TABLE",
            &format!(
                "{}.{}",
                statement.namespace_id.as_str(),
                statement.table_name.as_str()
            ),
            Some(format!(
                "Type: {:?}, Cleanup Job: {}",
                actual_type,
                job_id.as_str()
            )),
            None,
        );
        audit::persist_audit_entry(&self.app_context, &audit_entry).await?;

        log::info!(
            "✅ DROP TABLE succeeded: {}.{} (type: {:?}) - Cleanup job: {}",
            statement.namespace_id.as_str(),
            statement.table_name.as_str(),
            actual_type,
            job_id.as_str()
        );

        Ok(ExecutionResult::Success {
            message: format!(
                "Table {}.{} dropped successfully. Cleanup job: {}",
                statement.namespace_id.as_str(),
                statement.table_name.as_str(),
                job_id.as_str()
            ),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &DropTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use crate::sql::executor::helpers::guards::block_anonymous_write;
        
        // T050: Block anonymous users from DDL operations
        block_anonymous_write(context, "DROP TABLE")?;
        
        // Coarse auth gate (fine-grained check performed in execute using actual table type)
        if context.is_system() || context.is_admin() {
            return Ok(());
        }
        // Allow users to attempt; execute() will enforce per-table RBAC
        Ok(())
    }
}
