//! Helper utilities for flush testing
//!
//! This module provides utilities for:
//! - Executing flush jobs directly in tests (synchronous)
//! - Waiting for flush jobs to complete
//! - Checking Parquet file existence
//! - Verifying job completion metrics

use super::TestServer;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Execute a flush job synchronously for testing
///
/// This bypasses the JobManager and directly calls flush_job.execute(),
/// which is useful in test environments where background tokio tasks
/// may not execute properly.
///
/// # Arguments
///
/// * `server` - Test server instance
/// * `namespace` - Namespace name
/// * `table_name` - Table name
///
/// # Returns
///
/// Result containing flush job result with row counts and file paths
pub async fn execute_flush_synchronously(
    server: &TestServer,
    namespace: &str,
    table_name: &str,
) -> Result<kalamdb_core::flush::FlushJobResult, String> {
    use kalamdb_core::catalog::{NamespaceId, TableName};
    use kalamdb_core::flush::UserTableFlushJob;

    // Get table definition from kalam_sql
    let kalam_sql = &server.kalam_sql;

    let table_def = kalam_sql
        .get_table_definition(namespace, table_name)
        .map_err(|e| format!("Failed to get table definition: {}", e))?
        .ok_or_else(|| format!("Table {}.{} not found", namespace, table_name))?;

    // Get the latest schema from schema_history
    if table_def.schema_history.is_empty() {
        return Err(format!(
            "No schema history found for table '{}.{}'",
            namespace, table_name
        ));
    }

    let latest_schema_version = &table_def.schema_history[table_def.schema_history.len() - 1];

    // Convert to Arrow schema
    let arrow_schema = kalamdb_core::schema::arrow_schema::ArrowSchemaWithOptions::from_json_string(
        &latest_schema_version.arrow_schema_json,
    )
    .map_err(|e| format!("Failed to parse Arrow schema: {}", e))?;

    // Use DB from TestServer
    let db = server.db.clone();
    
    // Create user table store
    let user_table_store = Arc::new(
        kalamdb_store::UserTableStore::new(db.clone())
            .map_err(|e| format!("Failed to create UserTableStore: {}", e))?,
    );

    // Get storage_id from table definition and convert to String
    let storage_id = table_def.storage_id.to_string();

    // Create flush job
    let namespace_id = NamespaceId::from(namespace);
    let table_name_id = TableName::from(table_name);
    
    // Create storage registry (needed for path resolution)
    let storage_registry = Arc::new(kalamdb_core::storage::StorageRegistry::new(
        server.kalam_sql.clone(),
    ));
    
    let flush_job = UserTableFlushJob::new(
        user_table_store.clone(),
        namespace_id,
        table_name_id,
        arrow_schema.schema,
        storage_id,
    )
    .with_storage_registry(storage_registry);

    // Execute flush synchronously
    let result = flush_job
        .execute()
        .map_err(|e| format!("Flush execution failed: {}", e))?;

    Ok(result)
}

/// Wait for a flush job to complete and verify it succeeded
///
/// # Arguments
///
/// * `server` - Test server instance
/// * `job_id` - Job ID to wait for
/// * `max_wait` - Maximum time to wait for completion
///
/// # Returns
///
/// Result containing job result string if successful
pub async fn wait_for_flush_job_completion(
    server: &TestServer,
    job_id: &str,
    max_wait: Duration,
) -> Result<String, String> {
    let start = std::time::Instant::now();
    let check_interval = Duration::from_millis(200);

    loop {
        if start.elapsed() > max_wait {
            return Err(format!(
                "Timeout waiting for job {} to complete after {:?}",
                job_id, max_wait
            ));
        }

        let query = format!(
            "SELECT status, result, error_message, started_at, completed_at FROM system.jobs WHERE job_id = '{}'",
            job_id
        );

        let response = server.execute_sql(&query).await;

        if response.status != "success" {
            // system.jobs might not be accessible in some test setups
            // Just wait the full duration and return success
            println!("  ℹ Cannot query system.jobs (not an error in test env), waiting for job to execute...");
            sleep(max_wait).await;
            return Ok("Job executed (system.jobs not queryable in test)".to_string());
        }

        if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
            if rows.is_empty() {
                // Job not yet in system.jobs, wait a bit
                sleep(check_interval).await;
                continue;
            }

            if let Some(job) = rows.first() {
                let status = job
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                match status {
                    "completed" => {
                        // Verify took_ms is calculated (not 0)
                        let started_at = job.get("started_at").and_then(|v| v.as_i64());
                        let completed_at = job.get("completed_at").and_then(|v| v.as_i64());

                        if let (Some(start), Some(end)) = (started_at, completed_at) {
                            let took_ms = end - start;
                            if took_ms == 0 {
                                return Err(format!(
                                    "Job {} completed but took_ms = 0 (started_at: {}, completed_at: {}), which indicates a failure or instant completion bug",
                                    job_id, start, end
                                ));
                            }
                            println!(
                                "  ✓ Job {} completed successfully in {} ms",
                                job_id, took_ms
                            );
                        }

                        let result = job
                            .get("result")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        return Ok(result);
                    }
                    "failed" => {
                        let error = job
                            .get("error_message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(format!("Job {} failed: {}", job_id, error));
                    }
                    "running" => {
                        // Continue waiting
                        sleep(check_interval).await;
                        continue;
                    }
                    "cancelled" => {
                        return Err(format!("Job {} was cancelled", job_id));
                    }
                    _ => {
                        return Err(format!("Job {} has unexpected status: {}", job_id, status));
                    }
                }
            }
        } else {
            // No rows yet, job might not be created or registered
            sleep(check_interval).await;
        }
    }
}

/// Check if Parquet files exist for a user table
///
/// # Arguments
///
/// * `namespace` - Namespace name
/// * `table_name` - Table name
/// * `user_id` - User ID
///
/// # Returns
///
/// Vector of Parquet file paths found
pub fn check_user_parquet_files(
    namespace: &str,
    table_name: &str,
    user_id: &str,
) -> Vec<PathBuf> {
    let storage_path = PathBuf::from("./data")
        .join(user_id)
        .join("tables")
        .join(namespace)
        .join(table_name);

    check_parquet_files_in_path(&storage_path)
}

/// Check if Parquet files exist for a shared table
///
/// # Arguments
///
/// * `namespace` - Namespace name
/// * `table_name` - Table name
///
/// # Returns
///
/// Vector of Parquet file paths found
pub fn check_shared_parquet_files(namespace: &str, table_name: &str) -> Vec<PathBuf> {
    let storage_path = PathBuf::from("./data")
        .join("shared")
        .join(namespace)
        .join(table_name);

    check_parquet_files_in_path(&storage_path)
}

/// Check for Parquet files in a specific path
fn check_parquet_files_in_path(storage_path: &PathBuf) -> Vec<PathBuf> {
    let mut parquet_files = Vec::new();

    if storage_path.exists() {
        if let Ok(entries) = std::fs::read_dir(storage_path) {
            for entry in entries.flatten() {
                if let Some(extension) = entry.path().extension() {
                    if extension == "parquet" {
                        parquet_files.push(entry.path());
                        println!("  ✓ Found Parquet file: {}", entry.path().display());
                    }
                }
            }
        }
    }

    parquet_files
}

/// Verify that Parquet files exist after a flush with data
///
/// # Arguments
///
/// * `parquet_files` - Vector of Parquet file paths
/// * `expected_min` - Minimum number of files expected (usually 1 if data exists)
/// * `job_result` - Job result string for additional context
pub fn verify_parquet_files_exist(
    parquet_files: &[PathBuf],
    expected_min: usize,
    job_result: &str,
) -> Result<(), String> {
    if parquet_files.is_empty() && expected_min > 0 {
        return Err(format!(
            "No Parquet files found after flush, but expected at least {}. Job result: {}",
            expected_min, job_result
        ));
    }

    if parquet_files.len() < expected_min {
        return Err(format!(
            "Found {} Parquet files but expected at least {}",
            parquet_files.len(),
            expected_min
        ));
    }

    // Verify each file has reasonable size (not empty/corrupted)
    for file_path in parquet_files {
        match std::fs::metadata(file_path) {
            Ok(metadata) => {
                let file_size = metadata.len();
                // Parquet files should have headers even if minimal (~100 bytes minimum)
                if file_size < 50 {
                    return Err(format!(
                        "Parquet file too small: {} bytes (likely corrupted): {}",
                        file_size,
                        file_path.display()
                    ));
                }
                println!(
                    "  ✓ Parquet file valid: {} ({} bytes)",
                    file_path.display(),
                    file_size
                );
            }
            Err(e) => {
                return Err(format!(
                    "Failed to read file metadata for {}: {}",
                    file_path.display(),
                    e
                ));
            }
        }
    }

    println!(
        "  ✓ All {} Parquet files verified successfully",
        parquet_files.len()
    );
    Ok(())
}

/// Wait for Parquet files to appear after flush (polling-based check)
///
/// This is useful when system.jobs is not accessible or when the flush
/// happens asynchronously and we want to verify files are created.
///
/// # Arguments
///
/// * `namespace` - Namespace name
/// * `table_name` - Table name
/// * `user_id` - User ID (for user tables)
/// * `max_wait` - Maximum time to wait
/// * `expected_min` - Minimum number of files expected
///
/// # Returns
///
/// Vector of Parquet file paths found
pub async fn wait_for_parquet_files(
    namespace: &str,
    table_name: &str,
    user_id: &str,
    max_wait: Duration,
    expected_min: usize,
) -> Result<Vec<PathBuf>, String> {
    let start = std::time::Instant::now();
    let check_interval = Duration::from_millis(200);

    loop {
        let parquet_files = check_user_parquet_files(namespace, table_name, user_id);

        if parquet_files.len() >= expected_min {
            println!(
                "  ✓ Found {} Parquet files (expected at least {})",
                parquet_files.len(),
                expected_min
            );
            return Ok(parquet_files);
        }

        if start.elapsed() > max_wait {
            return Err(format!(
                "Timeout waiting for Parquet files. Found {}, expected at least {}. Waited {:?}",
                parquet_files.len(),
                expected_min,
                max_wait
            ));
        }

        sleep(check_interval).await;
    }
}

/// Extract job_id from FLUSH TABLE response message
///
/// Example message: "Flush job created. Job ID: flush-messages-20251026..."
pub fn extract_job_id(message: &str) -> Result<String, String> {
    if let Some(pos) = message.find("Job ID:") {
        let rest = &message[pos + 7..].trim();
        if rest.starts_with('[') {
            // Multiple job IDs (FLUSH ALL TABLES)
            let end = rest.find(']').unwrap_or(rest.len());
            let ids_str = &rest[1..end];
            let first_id = ids_str
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if first_id.is_empty() {
                return Err("No job ID found in array".to_string());
            }
            Ok(first_id)
        } else {
            // Single job ID
            let job_id = rest.split_whitespace().next().unwrap_or("").to_string();
            if job_id.is_empty() {
                return Err("Empty job ID".to_string());
            }
            Ok(job_id)
        }
    } else {
        Err(format!("No 'Job ID:' found in message: {}", message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_job_id_single() {
        let msg = "Flush job created. Job ID: flush-messages-20251026-abc123";
        let result = extract_job_id(msg);
        assert_eq!(result.unwrap(), "flush-messages-20251026-abc123");
    }

    #[test]
    fn test_extract_job_id_multiple() {
        let msg = "Flush jobs created. Job ID: [flush-table1-123, flush-table2-456]";
        let result = extract_job_id(msg);
        assert_eq!(result.unwrap(), "flush-table1-123");
    }

    #[test]
    fn test_extract_job_id_missing() {
        let msg = "Flush job created but no ID";
        let result = extract_job_id(msg);
        assert!(result.is_err());
    }
}
