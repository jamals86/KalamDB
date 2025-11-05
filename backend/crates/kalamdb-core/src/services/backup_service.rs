//! Namespace backup service
//!
//! Orchestrates complete namespace backup including:
//! - Metadata backup (namespaces, tables, schemas)
//! - Parquet file copying (user/shared tables)
//! - Manifest generation
//! - Job tracking
//!
//! Uses three-layer architecture:
//! - kalamdb-sql for metadata operations
//! - File system operations for Parquet files
//! - No direct RocksDB access

use crate::schema::{NamespaceId, TableType};
use crate::error::KalamDbError;
use kalamdb_commons::models::{JobId, JobStatus, JobType, NodeId};
use kalamdb_sql::{Job, Namespace, Table, TableSchema};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Backup manifest containing all metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Backup version format
    pub version: String,

    /// Timestamp when backup was created
    pub created_at: i64,

    /// Namespace metadata
    pub namespace: Namespace,

    /// All tables in namespace
    pub tables: Vec<Table>,

    /// Schema versions for each table
    pub table_schemas: HashMap<String, Vec<TableSchema>>,

    /// Backup statistics
    pub statistics: BackupStatistics,
}

/// Statistics about backup operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatistics {
    /// Number of tables backed up
    pub tables_count: usize,

    /// Number of Parquet files copied
    pub files_count: usize,

    /// Total bytes backed up
    pub total_bytes: u64,

    /// Number of user tables
    pub user_tables_count: usize,

    /// Number of shared tables
    pub shared_tables_count: usize,

    /// Number of stream tables (metadata only, no data)
    pub stream_tables_count: usize,
}

/// Result of backup operation
#[derive(Debug, Clone)]
pub struct BackupResult {
    /// Path where backup was stored
    pub backup_path: String,

    /// Backup manifest
    pub manifest: BackupManifest,

    /// Job ID tracking this backup
    pub job_id: String,
}

/// Namespace backup service
pub struct BackupService {
    kalam_sql: Arc<KalamSql>,
}

impl BackupService {
    /// Create a new backup service
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self { kalam_sql }
    }

    /// Backup a namespace to the specified path
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace to backup
    /// * `backup_path` - Destination directory for backup
    /// * `if_exists` - If true, don't error if namespace doesn't exist
    ///
    /// # Returns
    /// * `Ok(Some(result))` - Namespace was backed up
    /// * `Ok(None)` - Namespace didn't exist and if_exists was true
    /// * `Err(_)` - Backup error
    pub fn backup_namespace(
        &self,
        namespace_id: &NamespaceId,
        backup_path: &str,
        if_exists: bool,
    ) -> Result<Option<BackupResult>, KalamDbError> {
        // Step 1: Check if namespace exists (T180)
        let namespace_metadata = self
            .kalam_sql
            .get_namespace(namespace_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get namespace: {}", e)))?;

        if namespace_metadata.is_none() {
            if if_exists {
                return Ok(None);
            } else {
                return Err(KalamDbError::NotFound(format!(
                    "Namespace '{}' not found",
                    namespace_id.as_str()
                )));
            }
        }

        let namespace = namespace_metadata.unwrap();

        // Step 2: Create job record for tracking (T188)
        let job_id = self.create_backup_job(namespace_id)?;

        // Step 3: Create backup directory structure
        let backup_dir = PathBuf::from(backup_path);
        if let Err(e) = fs::create_dir_all(&backup_dir) {
            self.fail_backup_job(
                &job_id,
                &format!("Failed to create backup directory: {}", e),
            )?;
            return Err(KalamDbError::IoError(format!(
                "Failed to create backup directory: {}",
                e
            )));
        }

        // Step 4: Backup metadata (T180)
        let (tables, table_schemas) = match self.backup_metadata(namespace_id) {
            Ok(data) => data,
            Err(e) => {
                self.fail_backup_job(&job_id, &e.to_string())?;
                return Err(e);
            }
        };

        // Step 5: Backup Parquet files (T181, T182, T183)
        let (files_count, total_bytes, table_type_counts) =
            match self.backup_parquet_files(&tables, &backup_dir) {
                Ok(stats) => stats,
                Err(e) => {
                    self.fail_backup_job(&job_id, &e.to_string())?;
                    return Err(e);
                }
            };

        // Step 6: Create manifest (T180)
        let statistics = BackupStatistics {
            tables_count: tables.len(),
            files_count,
            total_bytes,
            user_tables_count: table_type_counts.0,
            shared_tables_count: table_type_counts.1,
            stream_tables_count: table_type_counts.2,
        };

        let manifest = BackupManifest {
            version: "1.0".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            namespace: namespace.clone(),
            tables: tables.clone(),
            table_schemas: table_schemas.clone(),
            statistics: statistics.clone(),
        };

        // Step 7: Write manifest to file
        let manifest_path = backup_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| KalamDbError::IoError(format!("Failed to serialize manifest: {}", e)))?;

        if let Err(e) = fs::write(&manifest_path, manifest_json) {
            self.fail_backup_job(&job_id, &format!("Failed to write manifest: {}", e))?;
            return Err(KalamDbError::IoError(format!(
                "Failed to write manifest: {}",
                e
            )));
        }

        // Step 8: Complete job (T188)
        self.complete_backup_job(&job_id, files_count, total_bytes)?;

        Ok(Some(BackupResult {
            backup_path: backup_path.to_string(),
            manifest,
            job_id,
        }))
    }

    /// Backup metadata: fetch namespaces, tables, schemas (T180)
    fn backup_metadata(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<(Vec<Table>, HashMap<String, Vec<TableSchema>>), KalamDbError> {
        // Fetch all tables in namespace
        let all_tables = self
            .kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::IoError(format!("Failed to scan tables: {}", e)))?;

        let tables: Vec<Table> = all_tables
            .into_iter()
            .filter(|t| t.namespace.as_str() == namespace_id.as_str())
            .collect();

        // TODO: Phase 2b - Fetch schema from information_schema.tables (TableDefinition.schema_history)
        // For now, skip schema backup until information_schema is implemented
        let table_schemas = HashMap::new();

        // Fetch schema versions for each table
        // let mut table_schemas = HashMap::new();
        // for table in &tables {
        //     let schemas = self
        //         .kalam_sql
        //         .get_table_schemas_for_table(&table.table_id)
        //         .map_err(|e| {
        //             KalamDbError::IoError(format!("Failed to get schemas for table: {}", e))
        //         })?;

        //     table_schemas.insert(table.table_id.clone(), schemas);
        // }

        Ok((tables, table_schemas))
    }

    /// Backup Parquet files for user/shared tables (T181, T182, T183)
    ///
    /// Returns: (files_count, total_bytes, (user_count, shared_count, stream_count))
    fn backup_parquet_files(
        &self,
        tables: &[Table],
        backup_dir: &Path,
    ) -> Result<(usize, u64, (usize, usize, usize)), KalamDbError> {
        let mut files_count = 0;
        let mut total_bytes = 0u64;
        let mut user_tables_count = 0;
        let mut shared_tables_count = 0;
        let mut stream_tables_count = 0;

        for table in tables {
            let table_type = table.table_type;

            match table_type {
                TableType::User => {
                    user_tables_count += 1;
                    // T181: Copy user table Parquet files
                    let (count, bytes) = self.backup_user_table_files(table, backup_dir)?;
                    files_count += count;
                    total_bytes += bytes;
                }
                TableType::Shared => {
                    shared_tables_count += 1;
                    // T181: Copy shared table Parquet files
                    let (count, bytes) = self.backup_shared_table_files(table, backup_dir)?;
                    files_count += count;
                    total_bytes += bytes;
                }
                TableType::Stream => {
                    stream_tables_count += 1;
                    // T183: Skip stream tables (ephemeral data, metadata only in backup)
                    log::info!(
                        "Skipping Parquet files for stream table '{}' (ephemeral data)",
                        table.table_name
                    );
                }
                TableType::System => {
                    // System tables are not backed up (they contain runtime data)
                    log::debug!("Skipping system table '{}'", table.table_name);
                }
            }
        }

        Ok((
            files_count,
            total_bytes,
            (user_tables_count, shared_tables_count, stream_tables_count),
        ))
    }

    /// Backup Parquet files for a user table (T181, T182)
    ///
    /// T182: Soft-deleted rows (_deleted=true) are automatically preserved in Parquet files
    fn backup_user_table_files(
        &self,
        table: &Table,
        backup_dir: &Path,
    ) -> Result<(usize, u64), KalamDbError> {
        let mut files_count = 0;
        let mut total_bytes = 0u64;

        // Parse storage location to find Parquet files
        // Format: ${storage_path}/${user_id}/batch-*.parquet
        // TODO: Phase 9 - Use TableCache for dynamic path resolution
        let storage_id = table.storage_id.as_ref().map(|s| s.as_str()).unwrap_or("local");

        
    // Extract storage path from location (assuming format like "local:data" or path)
    let base_path = if storage_id.contains(':') {
        let parts: Vec<&str> = storage_id.split(':').collect();
        if parts.len() == 2 {
            parts[1]
        } else {
            storage_id
        }
    } else {
        storage_id
    };

    // Create backup destination for this user table
    let table_backup_dir = backup_dir
        .join("user_tables")
        .join(table.table_name.as_str());

    // Find all user directories (pattern: ${base_path}/*/batch-*.parquet)
    let base_dir = Path::new(base_path);
    if !base_dir.exists() {
            // No data files yet, just return
            return Ok((0, 0));
        }

        // Iterate through user directories
        let entries = fs::read_dir(base_dir).map_err(|e| {
            KalamDbError::IoError(format!("Failed to read user table directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                KalamDbError::IoError(format!("Failed to read directory entry: {}", e))
            })?;

            let user_dir = entry.path();
            if !user_dir.is_dir() {
                continue;
            }

            let user_id = user_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            // Copy Parquet files for this user
            let (count, bytes) =
                self.copy_parquet_files(&user_dir, &table_backup_dir.join(user_id))?;
            files_count += count;
            total_bytes += bytes;
        }

        Ok((files_count, total_bytes))
    }

    /// Backup Parquet files for a shared table (T181, T182)
    fn backup_shared_table_files(
        &self,
        table: &Table,
        backup_dir: &Path,
    ) -> Result<(usize, u64), KalamDbError> {
        // Parse storage location
        // TODO: Phase 9 - Use TableCache for dynamic path resolution
        let storage_id = table.storage_id.as_ref().map(|s| s.as_str()).unwrap_or("local");
        let base_path = if storage_id.contains(':') {
            let parts: Vec<&str> = storage_id.split(':').collect();
            if parts.len() == 2 {
                parts[1]
            } else {
                storage_id
            }
        } else {
            storage_id
        };

        // Shared table path format: ${storage_path}/shared/{table_name}/batch-*.parquet
        let shared_dir = Path::new(base_path)
            .join("shared")
            .join(table.table_name.as_str());

        if !shared_dir.exists() {
            // No data files yet
            return Ok((0, 0));
        }

        // Create backup destination
        let table_backup_dir = backup_dir
            .join("shared_tables")
            .join(table.table_name.as_str());

        // Copy all Parquet files
        self.copy_parquet_files(&shared_dir, &table_backup_dir)
    }

    /// Copy Parquet files from source to destination
    fn copy_parquet_files(
        &self,
        source_dir: &Path,
        dest_dir: &Path,
    ) -> Result<(usize, u64), KalamDbError> {
        if !source_dir.exists() {
            return Ok((0, 0));
        }

        // Create destination directory
        fs::create_dir_all(dest_dir).map_err(|e| {
            KalamDbError::IoError(format!("Failed to create destination directory: {}", e))
        })?;

        let mut files_count = 0;
        let mut total_bytes = 0u64;

        let entries = fs::read_dir(source_dir).map_err(|e| {
            KalamDbError::IoError(format!("Failed to read source directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                KalamDbError::IoError(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("parquet") {
                let file_name = path.file_name().unwrap();
                let dest_path = dest_dir.join(file_name);

                // Copy file
                let bytes = fs::copy(&path, &dest_path).map_err(|e| {
                    KalamDbError::IoError(format!("Failed to copy Parquet file: {}", e))
                })?;

                files_count += 1;
                total_bytes += bytes;

                log::debug!("Copied Parquet file: {} ({} bytes)", path.display(), bytes);
            }
        }

        Ok((files_count, total_bytes))
    }

    /// Create a backup job record (T188)
    fn create_backup_job(&self, namespace_id: &NamespaceId) -> Result<String, KalamDbError> {
        let job_id = format!(
            "backup-{}-{}",
            namespace_id.as_str(),
            chrono::Utc::now().timestamp_millis()
        );

        let now_ms = chrono::Utc::now().timestamp_millis();
        let job = Job {
            job_id: JobId::new(job_id.clone()),
            job_type: JobType::Backup,
            status: JobStatus::Running,
            namespace_id: namespace_id.clone(),
            table_name: None,
            parameters: Some(format!(r#"{{"namespace_id":"{}"}}"#, namespace_id.as_str())),
            result: None,
            trace: None,
            memory_used: None,
            cpu_used: None,
            created_at: now_ms,
            started_at: Some(now_ms),
            completed_at: None,
                node_id: NodeId::from("local"),
            error_message: None,
        };

        self.kalam_sql
            .insert_job(&job)
            .map_err(|e| KalamDbError::IoError(format!("Failed to create backup job: {}", e)))?;

        Ok(job_id)
    }

    /// Complete a backup job (T188)
    fn complete_backup_job(
        &self,
        job_id: &str,
        files_backed_up: usize,
        total_bytes: u64,
    ) -> Result<(), KalamDbError> {
        let mut job = self
            .kalam_sql
            .get_job(job_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get job: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Job '{}' not found", job_id)))?;

        job.status = JobStatus::Completed;
        job.completed_at = Some(chrono::Utc::now().timestamp_millis());
        job.result = Some(format!(
            r#"{{"files_backed_up":{},"total_bytes":{}}}"#,
            files_backed_up, total_bytes
        ));

        self.kalam_sql
            .update_job(&job)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update job: {}", e)))?;

        Ok(())
    }

    /// Mark a backup job as failed (T188)
    fn fail_backup_job(&self, job_id: &str, error: &str) -> Result<(), KalamDbError> {
        let mut job = self
            .kalam_sql
            .get_job(job_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get job: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Job '{}' not found", job_id)))?;

        job.status = JobStatus::Failed;
        job.completed_at = Some(chrono::Utc::now().timestamp_millis());
        job.error_message = Some(error.to_string());

        self.kalam_sql
            .update_job(&job)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update job: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_backup_service_creation() {
        // Cannot create actual service without RocksDB, just verify struct exists
        // Real tests would require integration test with running database
    }
}
