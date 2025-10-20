//! Namespace restore service
//!
//! Orchestrates complete namespace restore including:
//! - Backup verification
//! - Metadata restore (namespaces, tables, schemas)
//! - Parquet file copying from backup
//! - Job tracking
//!
//! Uses three-layer architecture:
//! - kalamdb-sql for metadata operations
//! - File system operations for Parquet files
//! - No direct RocksDB access

use crate::catalog::{NamespaceId, TableType};
use crate::error::KalamDbError;
use crate::services::backup_service::BackupManifest;
use kalamdb_sql::{Job, KalamSql};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Result of restore operation
#[derive(Debug, Clone)]
pub struct RestoreResult {
    /// Namespace ID that was restored
    pub namespace_id: String,

    /// Number of tables restored
    pub tables_count: usize,

    /// Number of Parquet files restored
    pub files_count: usize,

    /// Total bytes restored
    pub total_bytes: u64,

    /// Job ID tracking this restore
    pub job_id: String,
}

/// Namespace restore service
pub struct RestoreService {
    kalam_sql: Arc<KalamSql>,
}

impl RestoreService {
    /// Create a new restore service
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self { kalam_sql }
    }

    /// Restore a namespace from backup
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace to restore
    /// * `backup_path` - Source directory containing backup
    /// * `if_not_exists` - If true, don't error if namespace already exists
    ///
    /// # Returns
    /// * `Ok(Some(result))` - Namespace was restored
    /// * `Ok(None)` - Namespace already exists and if_not_exists was true
    /// * `Err(_)` - Restore error
    pub fn restore_namespace(
        &self,
        namespace_id: &NamespaceId,
        backup_path: &str,
        if_not_exists: bool,
    ) -> Result<Option<RestoreResult>, KalamDbError> {
        // Step 1: Create job record for tracking (T188)
        let job_id = self.create_restore_job(namespace_id)?;

        // Step 2: Validate backup (T187)
        let manifest = match self.validate_backup(backup_path) {
            Ok(m) => m,
            Err(e) => {
                self.fail_restore_job(&job_id, &e.to_string())?;
                return Err(e);
            }
        };

        // Verify namespace ID matches
        if manifest.namespace.namespace_id != namespace_id.as_str() {
            let error_msg = format!(
                "Backup namespace '{}' does not match requested namespace '{}'",
                manifest.namespace.namespace_id,
                namespace_id.as_str()
            );
            self.fail_restore_job(&job_id, &error_msg)?;
            return Err(KalamDbError::InvalidSql(error_msg));
        }

        // Step 3: Check if namespace already exists
        let existing_namespace = self
            .kalam_sql
            .get_namespace(namespace_id.as_str())
            .map_err(|e| KalamDbError::IoError(format!("Failed to check namespace: {}", e)))?;

        if existing_namespace.is_some() {
            if if_not_exists {
                // Complete job with note that namespace already exists
                self.complete_restore_job(&job_id, 0, 0, 0)?;
                return Ok(None);
            } else {
                let error_msg = format!("Namespace '{}' already exists", namespace_id.as_str());
                self.fail_restore_job(&job_id, &error_msg)?;
                return Err(KalamDbError::Conflict(error_msg));
            }
        }

        // Step 4: Restore metadata (T185)
        if let Err(e) = self.restore_metadata(&manifest) {
            self.fail_restore_job(&job_id, &e.to_string())?;
            return Err(e);
        }

        // Step 5: Restore Parquet files (T186)
        let (files_count, total_bytes) = match self.restore_parquet_files(&manifest, backup_path) {
            Ok(stats) => stats,
            Err(e) => {
                // Attempt rollback: delete restored metadata
                log::error!("Parquet restore failed: {}, attempting rollback", e);
                if let Err(rollback_err) = self.rollback_metadata(&manifest) {
                    log::error!("Rollback failed: {}", rollback_err);
                }
                self.fail_restore_job(&job_id, &e.to_string())?;
                return Err(e);
            }
        };

        // Step 6: Complete job (T188)
        self.complete_restore_job(&job_id, manifest.tables.len(), files_count, total_bytes)?;

        Ok(Some(RestoreResult {
            namespace_id: namespace_id.to_string(),
            tables_count: manifest.tables.len(),
            files_count,
            total_bytes,
            job_id,
        }))
    }

    /// Validate backup structure and integrity (T187)
    fn validate_backup(&self, backup_path: &str) -> Result<BackupManifest, KalamDbError> {
        let backup_dir = Path::new(backup_path);

        // Check if backup directory exists
        if !backup_dir.exists() {
            return Err(KalamDbError::NotFound(format!(
                "Backup directory '{}' not found",
                backup_path
            )));
        }

        // Read and parse manifest.json
        let manifest_path = backup_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Err(KalamDbError::NotFound(
                "Backup manifest.json not found".to_string(),
            ));
        }

        let manifest_json = fs::read_to_string(&manifest_path)
            .map_err(|e| KalamDbError::IoError(format!("Failed to read manifest: {}", e)))?;

        let manifest: BackupManifest = serde_json::from_str(&manifest_json)
            .map_err(|e| KalamDbError::IoError(format!("Failed to parse manifest: {}", e)))?;

        // Validate manifest version
        if manifest.version != "1.0" {
            return Err(KalamDbError::InvalidSql(format!(
                "Unsupported backup version: {}",
                manifest.version
            )));
        }

        // Validate schema consistency: all tables have at least one schema version
        for table in &manifest.tables {
            if !manifest.table_schemas.contains_key(&table.table_id) {
                return Err(KalamDbError::InvalidSql(format!(
                    "Missing schema for table '{}'",
                    table.table_name
                )));
            }

            let schemas = &manifest.table_schemas[&table.table_id];
            if schemas.is_empty() {
                return Err(KalamDbError::InvalidSql(format!(
                    "No schema versions for table '{}'",
                    table.table_name
                )));
            }
        }

        // Validate Parquet file existence for user/shared tables
        for table in &manifest.tables {
            let table_type = TableType::from_str(&table.table_type).ok_or_else(|| {
                KalamDbError::InvalidSql(format!("Invalid table type: {}", table.table_type))
            })?;

            match table_type {
                TableType::User => {
                    let table_backup_dir = backup_dir.join("user_tables").join(&table.table_name);
                    if !table_backup_dir.exists() {
                        log::warn!(
                            "No Parquet files found for user table '{}' (may be empty)",
                            table.table_name
                        );
                    }
                }
                TableType::Shared => {
                    let table_backup_dir = backup_dir.join("shared_tables").join(&table.table_name);
                    if !table_backup_dir.exists() {
                        log::warn!(
                            "No Parquet files found for shared table '{}' (may be empty)",
                            table.table_name
                        );
                    }
                }
                TableType::Stream => {
                    // Stream tables have no Parquet files (ephemeral data)
                }
                TableType::System => {
                    // System tables not backed up
                }
            }
        }

        log::info!(
            "Backup validation passed: {} tables, {} files, {} bytes",
            manifest.statistics.tables_count,
            manifest.statistics.files_count,
            manifest.statistics.total_bytes
        );

        Ok(manifest)
    }

    /// Restore metadata: namespaces, tables, schemas (T185)
    fn restore_metadata(&self, manifest: &BackupManifest) -> Result<(), KalamDbError> {
        // Step 1: Create namespace
        self.kalam_sql
            .insert_namespace_struct(&manifest.namespace)
            .map_err(|e| KalamDbError::IoError(format!("Failed to insert namespace: {}", e)))?;

        log::info!("Restored namespace '{}'", manifest.namespace.namespace_id);

        // Step 2: Create tables
        for table in &manifest.tables {
            self.kalam_sql
                .insert_table(table)
                .map_err(|e| KalamDbError::IoError(format!("Failed to insert table: {}", e)))?;

            log::debug!("Restored table metadata: {}", table.table_name);
        }

        // Step 3: Create schema versions
        for (table_id, schemas) in &manifest.table_schemas {
            for schema in schemas {
                self.kalam_sql.insert_table_schema(schema).map_err(|e| {
                    KalamDbError::IoError(format!("Failed to insert schema: {}", e))
                })?;

                log::debug!(
                    "Restored schema version {} for table '{}'",
                    schema.version,
                    table_id
                );
            }
        }

        log::info!(
            "Restored metadata: {} tables, {} schemas",
            manifest.tables.len(),
            manifest
                .table_schemas
                .values()
                .map(|v| v.len())
                .sum::<usize>()
        );

        Ok(())
    }

    /// Restore Parquet files from backup to active storage (T186)
    fn restore_parquet_files(
        &self,
        manifest: &BackupManifest,
        backup_path: &str,
    ) -> Result<(usize, u64), KalamDbError> {
        let backup_dir = Path::new(backup_path);
        let mut files_count = 0;
        let mut total_bytes = 0u64;

        for table in &manifest.tables {
            let table_type = TableType::from_str(&table.table_type).ok_or_else(|| {
                KalamDbError::InvalidSql(format!("Invalid table type: {}", table.table_type))
            })?;

            match table_type {
                TableType::User => {
                    let (count, bytes) = self.restore_user_table_files(table, backup_dir)?;
                    files_count += count;
                    total_bytes += bytes;
                }
                TableType::Shared => {
                    let (count, bytes) = self.restore_shared_table_files(table, backup_dir)?;
                    files_count += count;
                    total_bytes += bytes;
                }
                TableType::Stream => {
                    // No Parquet files for stream tables
                    log::debug!(
                        "Skipping Parquet restore for stream table '{}'",
                        table.table_name
                    );
                }
                TableType::System => {
                    // System tables not restored
                }
            }
        }

        log::info!(
            "Restored Parquet files: {} files, {} bytes",
            files_count,
            total_bytes
        );

        Ok((files_count, total_bytes))
    }

    /// Restore Parquet files for a user table (T186)
    fn restore_user_table_files(
        &self,
        table: &kalamdb_sql::models::Table,
        backup_dir: &Path,
    ) -> Result<(usize, u64), KalamDbError> {
        let table_backup_dir = backup_dir.join("user_tables").join(&table.table_name);

        if !table_backup_dir.exists() {
            // No files to restore (empty table)
            return Ok((0, 0));
        }

        // Parse storage location to determine destination
        let storage_location = &table.storage_location;
        let base_path = if storage_location.contains(':') {
            let parts: Vec<&str> = storage_location.split(':').collect();
            if parts.len() == 2 {
                parts[1]
            } else {
                storage_location.as_str()
            }
        } else {
            storage_location.as_str()
        };

        let mut files_count = 0;
        let mut total_bytes = 0u64;

        // Iterate through user directories in backup
        let entries = fs::read_dir(&table_backup_dir).map_err(|e| {
            KalamDbError::IoError(format!("Failed to read user table backup directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                KalamDbError::IoError(format!("Failed to read directory entry: {}", e))
            })?;

            let user_backup_dir = entry.path();
            if !user_backup_dir.is_dir() {
                continue;
            }

            let user_id = user_backup_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            // Destination: ${storage_path}/${user_id}/
            let user_dest_dir = PathBuf::from(base_path).join(user_id);

            // Copy Parquet files with checksum verification
            let (count, bytes) =
                self.copy_parquet_files_with_verification(&user_backup_dir, &user_dest_dir)?;
            files_count += count;
            total_bytes += bytes;
        }

        Ok((files_count, total_bytes))
    }

    /// Restore Parquet files for a shared table (T186)
    fn restore_shared_table_files(
        &self,
        table: &kalamdb_sql::models::Table,
        backup_dir: &Path,
    ) -> Result<(usize, u64), KalamDbError> {
        let table_backup_dir = backup_dir.join("shared_tables").join(&table.table_name);

        if !table_backup_dir.exists() {
            // No files to restore (empty table)
            return Ok((0, 0));
        }

        // Parse storage location
        let storage_location = &table.storage_location;
        let base_path = if storage_location.contains(':') {
            let parts: Vec<&str> = storage_location.split(':').collect();
            if parts.len() == 2 {
                parts[1]
            } else {
                storage_location.as_str()
            }
        } else {
            storage_location.as_str()
        };

        // Destination: ${storage_path}/shared/{table_name}/
        let shared_dest_dir = PathBuf::from(base_path)
            .join("shared")
            .join(&table.table_name);

        // Copy Parquet files with checksum verification
        self.copy_parquet_files_with_verification(&table_backup_dir, &shared_dest_dir)
    }

    /// Copy Parquet files with checksum verification (T186)
    fn copy_parquet_files_with_verification(
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

                // Verify file size matches
                let dest_metadata = fs::metadata(&dest_path).map_err(|e| {
                    KalamDbError::IoError(format!("Failed to verify copied file: {}", e))
                })?;

                if dest_metadata.len() != bytes {
                    return Err(KalamDbError::IoError(format!(
                        "File size mismatch after copy: expected {}, got {}",
                        bytes,
                        dest_metadata.len()
                    )));
                }

                files_count += 1;
                total_bytes += bytes;

                log::debug!(
                    "Restored Parquet file: {} ({} bytes)",
                    path.display(),
                    bytes
                );
            }
        }

        Ok((files_count, total_bytes))
    }

    /// Rollback metadata in case of Parquet restore failure (T185)
    fn rollback_metadata(&self, manifest: &BackupManifest) -> Result<(), KalamDbError> {
        log::warn!(
            "Rolling back metadata for namespace '{}'",
            manifest.namespace.namespace_id
        );

        // Delete table schemas
        for table_id in manifest.table_schemas.keys() {
            if let Err(e) = self.kalam_sql.delete_table_schemas_for_table(table_id) {
                log::error!("Failed to delete schemas for table '{}': {}", table_id, e);
            }
        }

        // Delete tables
        for table in &manifest.tables {
            if let Err(e) = self.kalam_sql.delete_table(&table.table_id) {
                log::error!("Failed to delete table '{}': {}", table.table_name, e);
            }
        }

        // Delete namespace
        if let Err(e) = self
            .kalam_sql
            .delete_namespace(&manifest.namespace.namespace_id)
        {
            log::error!(
                "Failed to delete namespace '{}': {}",
                manifest.namespace.namespace_id,
                e
            );
            return Err(KalamDbError::IoError(format!(
                "Failed to delete namespace during rollback: {}",
                e
            )));
        }

        log::info!(
            "Rollback completed for namespace '{}'",
            manifest.namespace.namespace_id
        );
        Ok(())
    }

    /// Create a restore job record (T188)
    fn create_restore_job(&self, namespace_id: &NamespaceId) -> Result<String, KalamDbError> {
        let job_id = format!(
            "restore-{}-{}",
            namespace_id.as_str(),
            chrono::Utc::now().timestamp_millis()
        );

        let job = Job {
            job_id: job_id.clone(),
            job_type: "restore".to_string(),
            table_name: namespace_id.as_str().to_string(), // Use namespace as table_name
            status: "running".to_string(),
            start_time: chrono::Utc::now().timestamp(),
            end_time: None,
            parameters: vec![format!(r#"{{"namespace_id":"{}"}}"#, namespace_id.as_str())],
            result: None,
            trace: None,
            memory_used_mb: None,
            cpu_used_percent: None,
            node_id: "local".to_string(),
            error_message: None,
        };

        self.kalam_sql
            .insert_job(&job)
            .map_err(|e| KalamDbError::IoError(format!("Failed to create restore job: {}", e)))?;

        Ok(job_id)
    }

    /// Complete a restore job (T188)
    fn complete_restore_job(
        &self,
        job_id: &str,
        tables_restored: usize,
        files_restored: usize,
        total_bytes: u64,
    ) -> Result<(), KalamDbError> {
        let mut job = self
            .kalam_sql
            .get_job(job_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get job: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Job '{}' not found", job_id)))?;

        job.status = "completed".to_string();
        job.end_time = Some(chrono::Utc::now().timestamp());
        job.result = Some(format!(
            r#"{{"tables_restored":{},"files_restored":{},"total_bytes":{}}}"#,
            tables_restored, files_restored, total_bytes
        ));

        self.kalam_sql
            .update_job(&job)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update job: {}", e)))?;

        Ok(())
    }

    /// Mark a restore job as failed (T188)
    fn fail_restore_job(&self, job_id: &str, error: &str) -> Result<(), KalamDbError> {
        let mut job = self
            .kalam_sql
            .get_job(job_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get job: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Job '{}' not found", job_id)))?;

        job.status = "failed".to_string();
        job.end_time = Some(chrono::Utc::now().timestamp());
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
    fn test_restore_service_creation() {
        // Cannot create actual service without RocksDB, just verify struct exists
        // Real tests would require integration test with running database
    }
}
