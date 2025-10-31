//! Table deletion service
//!
//! Orchestrates complete table cleanup including:
//! - Active subscription checking
//! - RocksDB data deletion
//! - Parquet file cleanup
//! - Metadata removal
//! - Storage location usage tracking
//! - Job registration and tracking
//!
//! Uses three-layer architecture:
//! - kalamdb-store for data operations
//! - kalamdb-sql for metadata operations
//! - No direct RocksDB access

use crate::catalog::{NamespaceId, TableName, TableType};
use crate::error::KalamDbError;
use crate::stores::system_table::{SharedTableStoreExt, UserTableStoreExt};
use crate::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use kalamdb_commons::models::{JobId, JobStatus, JobType};
use kalamdb_sql::{Job, KalamSql};
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Result of table deletion operation
#[derive(Debug, Clone)]
pub struct TableDeletionResult {
    /// Number of Parquet files deleted
    pub files_deleted: usize,

    /// Total bytes freed from Parquet files
    pub bytes_freed: u64,

    /// Table type that was deleted
    pub table_type: TableType,

    /// Job ID tracking this deletion
    pub job_id: String,
}

/// Table deletion service
///
/// Coordinates table deletion using kalamdb-store and kalamdb-sql
pub struct TableDeletionService {
    user_table_store: Arc<UserTableStore>,
    shared_table_store: Arc<SharedTableStore>,
    stream_table_store: Arc<StreamTableStore>,
    kalam_sql: Arc<KalamSql>,
}

impl TableDeletionService {
    /// Create a new table deletion service
    pub fn new(
        user_table_store: Arc<UserTableStore>,
        shared_table_store: Arc<SharedTableStore>,
        stream_table_store: Arc<StreamTableStore>,
        kalam_sql: Arc<KalamSql>,
    ) -> Self {
        Self {
            user_table_store,
            shared_table_store,
            stream_table_store,
            kalam_sql,
        }
    }

    /// Drop a table with complete cleanup
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of table to drop
    /// * `table_type` - Type of table (User, Shared, Stream)
    /// * `if_exists` - If true, don't error if table doesn't exist
    ///
    /// # Returns
    /// * `Ok(Some(result))` - Table was deleted
    /// * `Ok(None)` - Table didn't exist and if_exists was true
    /// * `Err(_)` - Validation or cleanup error
    pub fn drop_table(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        table_type: TableType,
        if_exists: bool,
    ) -> Result<Option<TableDeletionResult>, KalamDbError> {
        // Generate table_id (format: namespace:table_name)
        let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());

        // Check if table exists
        let table_metadata = self
            .kalam_sql
            .get_table(&table_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get table: {}", e)))?;

        if table_metadata.is_none() {
            if if_exists {
                return Ok(None);
            } else {
                return Err(KalamDbError::NotFound(format!(
                    "Table '{}' not found in namespace '{}'",
                    table_name.as_str(),
                    namespace_id.as_str()
                )));
            }
        }

        let table = table_metadata.unwrap();

        // Step 1: Check for active subscriptions (T168)
        self.check_active_subscriptions(table_name)?;

        // Step 2: Create job record for tracking (T174)
        let job_id = self.create_deletion_job(&table_id, namespace_id, table_name, &table_type)?;

        // Step 3: Delete table data from RocksDB (T169)
        let data_cleanup_result = self.cleanup_table_data(namespace_id, table_name, &table_type);

        if let Err(e) = data_cleanup_result {
            // Update job as failed
            self.fail_deletion_job(&job_id, &e.to_string())?;
            return Err(e);
        }

        // Step 4: Delete Parquet files (T170)
        let parquet_result = self.cleanup_parquet_files(&table, &table_type);

        let (files_deleted, bytes_freed) = match parquet_result {
            Ok(stats) => stats,
            Err(e) => {
                // Attempt rollback: restore metadata
                log::error!("Parquet cleanup failed: {}, attempting rollback", e);
                self.fail_deletion_job(&job_id, &format!("Parquet cleanup failed: {}", e))?;

                // Note: Data deletion is idempotent, no need to restore
                // Just fail the operation
                return Err(e);
            }
        };

        // Step 5: Delete metadata (T171)
        let metadata_result = self.cleanup_metadata(&table_id);

        if let Err(e) = metadata_result {
            // Log warning but don't rollback (data is already deleted)
            log::warn!("Metadata cleanup failed but data is deleted: {}", e);
            self.fail_deletion_job(&job_id, &format!("Metadata cleanup failed: {}", e))?;
            return Err(e);
        }

        // Step 6: Update storage location usage count (T172)
        if !table.storage_location.is_empty() {
            if let Err(e) = self.decrement_storage_usage(&table.storage_location) {
                log::warn!("Failed to decrement storage usage count: {}", e);
                // Don't fail the operation for this
            }
        }

        // Step 7: Complete job successfully (T174)
        self.complete_deletion_job(&job_id, files_deleted, bytes_freed)?;

        Ok(Some(TableDeletionResult {
            files_deleted,
            bytes_freed,
            table_type,
            job_id,
        }))
    }

    /// Check for active subscriptions (T168)
    fn check_active_subscriptions(&self, table_name: &TableName) -> Result<(), KalamDbError> {
        let live_queries = self
            .kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::IoError(format!("Failed to scan live queries: {}", e)))?;

        let active_subscriptions: Vec<_> = live_queries
            .iter()
            .filter(|lq| lq.table_name == table_name.as_str().into())
            .collect();

        if !active_subscriptions.is_empty() {
            let subscription_details: Vec<String> = active_subscriptions
                .iter()
                .map(|lq| {
                    format!(
                        "connection_id={}, user_id={}, query_id={}",
                        lq.connection_id, lq.user_id, lq.query_id
                    )
                })
                .collect();

            return Err(KalamDbError::Conflict(format!(
                "Cannot drop table '{}': {} active subscription(s) exist: {}",
                table_name.as_str(),
                active_subscriptions.len(),
                subscription_details.join("; ")
            )));
        }

        Ok(())
    }

    /// Delete table data from RocksDB (T169)
    fn cleanup_table_data(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        table_type: &TableType,
    ) -> Result<(), KalamDbError> {
        match table_type {
            TableType::User => UserTableStoreExt::drop_table(
                self.user_table_store.as_ref(),
                namespace_id.as_str(),
                table_name.as_str()
            )
                .map_err(|e| {
                    KalamDbError::IoError(format!("Failed to drop user table data: {}", e))
                }),
            TableType::Shared => SharedTableStoreExt::drop_table(
                self.shared_table_store.as_ref(),
                namespace_id.as_str(),
                table_name.as_str()
            )
                .map_err(|e| {
                    KalamDbError::IoError(format!("Failed to drop shared table data: {}", e))
                }),
            TableType::Stream => self
                .stream_table_store
                .drop_table(namespace_id.as_str(), table_name.as_str())
                .map_err(|e| {
                    KalamDbError::IoError(format!("Failed to drop stream table data: {}", e))
                }),
            TableType::System => {
                // System tables cannot be dropped
                Err(KalamDbError::PermissionDenied(
                    "Cannot drop system tables".to_string(),
                ))
            }
        }
    }

    /// Delete Parquet files (T170)
    fn cleanup_parquet_files(
        &self,
        table: &kalamdb_sql::Table,
        table_type: &TableType,
    ) -> Result<(usize, u64), KalamDbError> {
        // Stream tables don't have Parquet files
        if matches!(table_type, TableType::Stream) {
            return Ok((0, 0));
        }

        let storage_path = Path::new(&table.storage_location);

        // Check if path exists
        if !storage_path.exists() {
            log::warn!("Storage path does not exist: {}", table.storage_location);
            return Ok((0, 0));
        }

        let mut files_deleted = 0;
        let mut bytes_freed = 0u64;

        match table_type {
            TableType::User => {
                // User tables: iterate directories and delete batch-*.parquet files
                // Pattern: ${storage_path}/${user_id}/batch-*.parquet
                self.cleanup_user_parquet_files(
                    storage_path,
                    &mut files_deleted,
                    &mut bytes_freed,
                )?;
            }
            TableType::Shared => {
                // Shared tables: delete files in shared/${table_name}/ directory
                // Pattern: ${storage_path}/shared/${table_name}/batch-*.parquet
                let table_path = storage_path.join("shared").join(table.table_name.as_str());
                self.cleanup_directory_parquet_files(
                    &table_path,
                    &mut files_deleted,
                    &mut bytes_freed,
                )?;
            }
            _ => {}
        }

        Ok((files_deleted, bytes_freed))
    }

    /// Cleanup Parquet files in user directories
    fn cleanup_user_parquet_files(
        &self,
        storage_path: &Path,
        files_deleted: &mut usize,
        bytes_freed: &mut u64,
    ) -> Result<(), KalamDbError> {
        if !storage_path.is_dir() {
            return Ok(());
        }

        // Iterate over user directories
        let entries = fs::read_dir(storage_path).map_err(|e| {
            KalamDbError::IoError(format!("Failed to read storage directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                KalamDbError::IoError(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();

            if path.is_dir() {
                // This should be a user_id directory
                self.cleanup_directory_parquet_files(&path, files_deleted, bytes_freed)?;
            }
        }

        Ok(())
    }

    /// Cleanup Parquet files in a specific directory
    fn cleanup_directory_parquet_files(
        &self,
        dir_path: &Path,
        files_deleted: &mut usize,
        bytes_freed: &mut u64,
    ) -> Result<(), KalamDbError> {
        if !dir_path.exists() {
            return Ok(());
        }

        if !dir_path.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir_path)
            .map_err(|e| KalamDbError::IoError(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| KalamDbError::IoError(format!("Failed to read entry: {}", e)))?;

            let path = entry.path();

            // Only delete .parquet files that match the batch-* pattern
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("parquet")
                && path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with("batch-"))
                    .unwrap_or(false)
            {
                // Get file size before deletion
                if let Ok(metadata) = fs::metadata(&path) {
                    *bytes_freed += metadata.len();
                }

                // Delete the file
                fs::remove_file(&path)
                    .map_err(|e| KalamDbError::IoError(format!("Failed to delete file: {}", e)))?;

                *files_deleted += 1;

                log::info!("Deleted Parquet file: {}", path.display());
            }
        }

        Ok(())
    }

    /// Delete metadata (T171)
    fn cleanup_metadata(&self, table_id: &str) -> Result<(), KalamDbError> {
        // TODO: Phase 2b - Schema history stored in TableDefinition.schema_history, no separate deletion needed
        // Delete table schemas
        // self.kalam_sql
        //     .delete_table_schemas_for_table(table_id)
        //     .map_err(|e| KalamDbError::IoError(format!("Failed to delete table schemas: {}", e)))?;

        // Delete table metadata
        self.kalam_sql
            .delete_table(table_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to delete table: {}", e)))?;

        Ok(())
    }

    /// Decrement storage location usage count (T172)
    fn decrement_storage_usage(&self, _location_name: &str) -> Result<(), KalamDbError> {
        // TODO: Phase 2b - system_storages no longer tracks usage_count, validation happens differently
        // Storage location usage tracking will be reimplemented when needed
        log::warn!(
            "Storage usage tracking temporarily disabled during migration to information_schema"
        );

        // Get current storage location
        // let mut location = self
        //     .kalam_sql
        //     .get_storage_location(location_name)
        //     .map_err(|e| KalamDbError::IoError(format!("Failed to get storage location: {}", e)))?
        //     .ok_or_else(|| {
        //         KalamDbError::NotFound(format!("Storage location '{}' not found", location_name))
        //     })?;

        // Decrement usage count (don't go below 0)
        // if location.usage_count > 0 {
        //     location.usage_count -= 1;
        // }

        // Update storage location
        // self.kalam_sql
        //     .update_storage_location(&location)
        //     .map_err(|e| {
        //         KalamDbError::IoError(format!("Failed to update storage location: {}", e))
        //     })?;

        Ok(())
    }

    /// Create deletion job (T174)
    fn create_deletion_job(
        &self,
        table_id: &str,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        table_type: &TableType,
    ) -> Result<String, KalamDbError> {
        let job_id = format!("drop_table:{}", table_id); //TODO: Use a function inside JobId to construct this
        let now_ms = chrono::Utc::now().timestamp_millis();

        let job: Job = Job {
            job_id: JobId::new(job_id.clone()),
            job_type: JobType::Cleanup,
            status: JobStatus::Running,
            namespace_id: namespace_id.clone(),
            table_name: Some(table_name.clone()),
            parameters: Some(format!(
                r#"[{{"namespace_id":"{}"}},{{"table_name":"{}"}},{{"table_type":"{:?}"}}]"#,
                namespace_id.as_str(),
                table_name.as_str(),
                table_type
            )),
            result: None,
            trace: None,
            memory_used: None,
            cpu_used: None,
            created_at: now_ms,
            started_at: Some(now_ms),
            completed_at: None,
            node_id: "localhost".to_string(), // TODO: Get actual node ID
            error_message: None,
        };

        self.kalam_sql
            .insert_job(&job)
            .map_err(|e| KalamDbError::IoError(format!("Failed to insert job: {}", e)))?;

        Ok(job_id)
    }

    /// Complete deletion job (T174)
    fn complete_deletion_job(
        &self,
        job_id: &str,
        files_deleted: usize,
        bytes_freed: u64,
    ) -> Result<(), KalamDbError> {
        let job = self
            .kalam_sql
            .get_job(job_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get job: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Job '{}' not found", job_id)))?;

        let now_ms = chrono::Utc::now().timestamp_millis();
        let result_json = serde_json::json!({
            "files_deleted": files_deleted,
            "bytes_freed": bytes_freed,
            "duration_ms": now_ms.saturating_sub(job.started_at.unwrap_or(job.created_at)),
        });

        let updated_job = Job {
            status: JobStatus::Completed,
            completed_at: Some(now_ms),
            result: Some(serde_json::to_string(&result_json).unwrap_or_default()),
            ..job
        };

        self.kalam_sql
            .update_job(&updated_job)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update job: {}", e)))?;

        Ok(())
    }

    /// Fail deletion job (T173, T174)
    fn fail_deletion_job(&self, job_id: &str, error_message: &str) -> Result<(), KalamDbError> {
        let job = self
            .kalam_sql
            .get_job(job_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get job: {}", e)))?;

        if let Some(job) = job {
            let updated_job = Job {
                status: JobStatus::Failed,
                completed_at: Some(chrono::Utc::now().timestamp_millis()),
                error_message: Some(error_message.to_string()),
                ..job
            };

            self.kalam_sql
                .update_job(&updated_job)
                .map_err(|e| KalamDbError::IoError(format!("Failed to update job: {}", e)))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::{StorageBackend, RocksDBBackend};

    fn create_test_service() -> (TableDeletionService, TestDb) {
        // Create test database with all required column families
        let cf_names = vec![
            "system_users",
            "system_namespaces",
            "system_storage_locations",
            "system_jobs",
            "system_live_queries",
            "system_tables",
            "system_table_schemas",
        ];

        let test_db = TestDb::new(&cf_names).unwrap();
        let db = Arc::clone(&test_db.db);
        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));

        let user_store = Arc::new(UserTableStore::new(backend.clone(), "user_table:app:users"));
        let shared_store = Arc::new(SharedTableStore::new(backend.clone(), "shared_table:app:config"));
        let stream_store = Arc::new(StreamTableStore::new(backend.clone(), "stream_table:app:events"));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());

        let service = TableDeletionService::new(user_store, shared_store, stream_store, kalam_sql);

        (service, test_db)
    }

    #[test]
    fn test_drop_nonexistent_table_with_if_exists() {
        let (service, _db) = create_test_service();

        let result = service.drop_table(
            &NamespaceId::new("test"),
            &TableName::new("nonexistent"),
            TableType::User,
            true,
        );

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_drop_nonexistent_table_without_if_exists() {
        let (service, _db) = create_test_service();

        let result = service.drop_table(
            &NamespaceId::new("test"),
            &TableName::new("nonexistent"),
            TableType::User,
            false,
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), KalamDbError::NotFound(_)));
    }

    #[test]
    fn test_check_active_subscriptions_empty() {
        let (service, _db) = create_test_service();

        let result = service.check_active_subscriptions(&TableName::new("messages"));

        assert!(result.is_ok());
    }

    #[test]
    fn test_cleanup_table_data_user_table() {
        let (service, _db) = create_test_service();

        // This will fail because the column family doesn't exist yet
        // In a real scenario, the table would be created first
        let result = service.cleanup_table_data(
            &NamespaceId::new("test"),
            &TableName::new("messages"),
            &TableType::User,
        );

        // We expect an error since the CF doesn't exist
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_drop_system_table() {
        let (service, _db) = create_test_service();

        let result = service.cleanup_table_data(
            &NamespaceId::new("system"),
            &TableName::new("users"),
            &TableType::System,
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            KalamDbError::PermissionDenied(_)
        ));
    }
}
