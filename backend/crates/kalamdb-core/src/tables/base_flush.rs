//! Base flush trait and common utilities for table flush operations
//!
//! This module provides a common interface and implementation for flushing table data
//! from RocksDB to Parquet files. It eliminates code duplication across different table
//! types (shared, user) by providing:
//!
//! - `TableFlush` trait: Common interface for all flush operations
//! - `FlushJobResult`: Standardized result type with metrics
//! - `FlushExecutor`: Template method pattern for job tracking and error handling
//!
//! ## Architecture
//!
//! ```text
//! TableFlush trait (defines interface)
//!      ↓
//! FlushExecutor (template method - common workflow)
//!      ↓
//! Implementations (SharedTableFlushJob, UserTableFlushJob)
//! ```
//!
//! ## Benefits
//!
//! - **DRY**: Eliminates 400+ lines of duplicated code
//! - **Consistent**: All flush jobs use same tracking/metrics/notification pattern
//! - **Extensible**: Easy to add new table types
//! - **Testable**: Base functionality tested once, implementations test unique logic

use crate::error::KalamDbError;
use crate::live_query::manager::LiveQueryManager;
use chrono::Utc;
use kalamdb_commons::system::Job;
use kalamdb_commons::{JobId, JobStatus, JobType, NodeId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Metadata for user table flush operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub struct UserTableFlushMetadata {
    /// Number of unique users whose data was flushed
    pub users_count: usize,

    /// Error messages for users that failed to flush (partial failures)
    pub errors: Vec<String>,
}


/// Metadata for shared table flush operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub struct SharedTableFlushMetadata {
    /// Additional context (reserved for future use)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}


/// Flush metadata for different table types
///
/// Type-safe alternative to JsonValue for flush operation metadata.
/// Each table type has its own strongly-typed metadata structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "table_type", rename_all = "snake_case")]
pub enum FlushMetadata {
    /// User table flush metadata (partitioned by user_id)
    UserTable(UserTableFlushMetadata),

    /// Shared table flush metadata (single Parquet file)
    SharedTable(SharedTableFlushMetadata),
}

impl FlushMetadata {
    /// Create metadata for user table flush
    pub fn user_table(users_count: usize, errors: Vec<String>) -> Self {
        FlushMetadata::UserTable(UserTableFlushMetadata {
            users_count,
            errors,
        })
    }

    /// Create metadata for shared table flush
    pub fn shared_table() -> Self {
        FlushMetadata::SharedTable(SharedTableFlushMetadata::default())
    }

    /// Get users_count if this is UserTable metadata
    pub fn users_count(&self) -> Option<usize> {
        match self {
            FlushMetadata::UserTable(meta) => Some(meta.users_count),
            _ => None,
        }
    }

    /// Get errors if this is UserTable metadata
    pub fn errors(&self) -> Option<&[String]> {
        match self {
            FlushMetadata::UserTable(meta) => Some(&meta.errors),
            _ => None,
        }
    }
}

/// Result of a flush job execution
///
/// Contains job metadata, metrics, and output files
#[derive(Debug, Clone)]
pub struct FlushJobResult {
    /// Job record for system.jobs table
    pub job_record: Job,

    /// Total rows flushed from RocksDB to Parquet
    pub rows_flushed: usize,

    /// Parquet files written (relative paths)
    pub parquet_files: Vec<String>,

    /// Type-safe metadata specific to table type
    pub metadata: FlushMetadata,
}

/// Base trait for table flush operations
///
/// Implement this trait for each table type (shared, user, stream).
/// Use `FlushExecutor::execute_with_tracking()` to run flush jobs with
/// automatic job tracking, metrics, and error handling.
///
/// ## Snapshot Consistency
///
/// All flush operations use RocksDB snapshots via `scan_iter()`:
/// - ✅ ACID guarantees: Flush sees consistent point-in-time view
/// - ✅ No locking: Writes continue during flush
/// - ✅ Isolation: Concurrent flushes don't interfere
///
/// ## Example
///
/// ```rust,ignore
/// impl TableFlush for MyTableFlushJob {
///     fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
///         // Unique flush logic (scan, group, write Parquet)
///         let rows = self.store.scan_iter()?; // ← Snapshot-backed!
///         // ... process rows ...
///         Ok(FlushJobResult { /* ... */ })
///     }
///
///     fn table_identifier(&self) -> String {
///         format!("{}.{}", self.namespace_id, self.table_name)
///     }
/// }
///
/// // Usage
/// let result = FlushExecutor::execute_with_tracking(&my_flush_job)?;
/// ```
pub trait TableFlush: Send + Sync {
    /// Execute the flush job
    ///
    /// Implement table-specific logic:
    /// - Scan rows from RocksDB (use `scan_iter()` for snapshot consistency)
    /// - Convert rows to Arrow RecordBatch (implementations handle this directly)
    /// - Write Parquet file(s) (use `ParquetWriter`)
    ///
    /// # Returns
    ///
    /// `FlushJobResult` with metrics and job record
    ///
    /// # Errors
    ///
    /// Returns `KalamDbError` if flush fails (I/O error, invalid data, etc.)
    fn execute(&self) -> Result<FlushJobResult, KalamDbError>;

    /// Get table identifier for logging
    ///
    /// Should return `namespace.table_name` format
    fn table_identifier(&self) -> String;

    /// Optional: Get LiveQueryManager for notifications
    ///
    /// Override if flush job supports live query notifications.
    /// Default returns None (no notifications).
    ///
    /// NOTE: Implementations should handle notifications themselves using
    /// `notify_table_change()` which is async. The base trait cannot do this
    /// because TableFlush must be Send + Sync (not async).
    fn live_query_manager(&self) -> Option<&Arc<LiveQueryManager>> {
        None
    }

    /// Optional: Get node ID for job tracking
    ///
    /// Override to customize node identification.
    /// Default uses process ID.
    fn node_id(&self) -> NodeId {
        //TODO: Use the nodeId from global config or context
        NodeId::from(format!("node-{}", std::process::id()))
    }

    /// Generate unique job ID
    ///
    /// Format: `flush-{table}-{timestamp}`
    fn generate_job_id(&self) -> String {
        format!(
            "flush-{}-{}",
            self.table_identifier().replace('.', "-"),
            Utc::now().timestamp_millis()
        )
    }

    /// Create base job record with common fields
    ///
    /// Initializes job with:
    /// - Status: JobStatus::Running
    /// - Type: JobType::Flush
    /// - Node ID
    /// - Start time
    ///
    /// NOTE: Requires namespace_id to be provided by implementation
    fn create_job_record(&self, job_id: &str, namespace_id: kalamdb_commons::NamespaceId) -> Job {
        let now_ms = chrono::Utc::now().timestamp_millis();
        Job {
            job_id: JobId::new(job_id.to_string()),
            job_type: JobType::Flush,
            namespace_id,
            table_name: None,
            status: JobStatus::Running,
            parameters: None,
            message: None,
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: now_ms,
            updated_at: now_ms,
            started_at: Some(now_ms),
            finished_at: None,
            node_id: self.node_id(),
            queue: None,
            priority: None,
        }
    }

    /// Send LiveQuery notification after successful flush (optional)
    ///
    /// Override this method in implementations if notifications are needed.
    /// Default is no-op.
    ///
    /// NOTE: Use `notify_table_change()` which is async. Spawn a tokio task:
    ///
    /// ```rust,ignore
    /// if let Some(manager) = self.live_query_manager() {
    ///     let manager = Arc::clone(manager);
    ///     let table = self.table_identifier();
    ///     let rows = rows_flushed;
    ///     let files = parquet_files.to_vec();
    ///     tokio::spawn(async move {
    ///         let notification = ChangeNotification::flush(table.clone(), rows, files);
    ///         let _ = manager.notify_table_change(&table, notification).await;
    ///     });
    /// }
    /// ```
    fn notify_flush(&self, _rows_flushed: usize, _parquet_files: &[String]) {
        // Default: no notifications
    }
}

/// Template method executor for flush jobs
///
/// Provides common workflow for all flush operations:
/// 1. Generate job ID
/// 2. Create job record (status: "running")
/// 3. Execute flush (call `TableFlush::execute()`)
/// 4. Track metrics (duration, row count)
/// 5. Send LiveQuery notifications
/// 6. Update job record (status: "completed" or "failed")
///
/// ## Benefits
///
/// - **Consistent**: All flushes use same tracking/error handling
/// - **Observable**: Metrics and logs for all operations
/// - **Reliable**: Proper error handling and job state management
///
/// ## Usage
///
/// ```rust,ignore
/// let flush_job = SharedTableFlushJob::new(/* ... */);
/// let result = FlushExecutor::execute_with_tracking(&flush_job)?;
/// println!("Flushed {} rows", result.rows_flushed);
/// ```
pub struct FlushExecutor;

impl FlushExecutor {
    /// Execute flush job with automatic tracking and metrics
    ///
    /// Template method pattern:
    /// - Wraps `TableFlush::execute()` with common logic
    /// - Handles errors and updates job status
    /// - Sends notifications on success
    ///
    /// # Arguments
    ///
    /// - `flush_job`: Implementation of `TableFlush` trait
    /// - `namespace_id`: Namespace for job record
    ///
    /// # Returns
    ///
    /// `FlushJobResult` with completed job record and metrics
    ///
    /// # Errors
    ///
    /// Returns `KalamDbError` if flush fails. Job record is marked as failed.
    pub fn execute_with_tracking<F: TableFlush>(
        flush_job: &F,
        namespace_id: kalamdb_commons::NamespaceId,
    ) -> Result<FlushJobResult, KalamDbError> {
        let job_id = flush_job.generate_job_id();
        let job_record = flush_job.create_job_record(&job_id, namespace_id);

        log::info!(
            "🚀 Flush job started: job_id={}, table={}",
            job_id,
            flush_job.table_identifier()
        );

        let start_time = std::time::Instant::now();

        match flush_job.execute() {
            Ok(mut result) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;

                log::info!(
                    "✅ Flush completed: job_id={}, table={}, rows={}, files={}, duration_ms={}",
                    job_id,
                    flush_job.table_identifier(),
                    result.rows_flushed,
                    result.parquet_files.len(),
                    duration_ms
                );

                // Serialize metadata to JSON for job result
                let metadata_json = serde_json::to_value(&result.metadata).map_err(|e| {
                    KalamDbError::Other(format!("Failed to serialize metadata: {}", e))
                })?;

                // Update job record with success
                let now_ms = chrono::Utc::now().timestamp_millis();
                let mut completed_job = job_record;
                completed_job.status = JobStatus::Completed;
                completed_job.message = Some(
                    serde_json::json!({
                        "rows_flushed": result.rows_flushed,
                        "duration_ms": duration_ms,
                        "parquet_files": result.parquet_files,
                        "metadata": metadata_json,
                    })
                    .to_string(),
                );
                completed_job.updated_at = now_ms;
                completed_job.finished_at = Some(now_ms);

                // Send notification
                flush_job.notify_flush(result.rows_flushed, &result.parquet_files);

                // Return result with updated job record
                result.job_record = completed_job;
                Ok(result)
            }
            Err(e) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;

                log::error!(
                    "❌ Flush failed: job_id={}, table={}, duration_ms={}, error={}",
                    job_id,
                    flush_job.table_identifier(),
                    duration_ms,
                    e
                );

                // Update job record with failure
                let now_ms = chrono::Utc::now().timestamp_millis();
                let mut failed_job = job_record;
                failed_job.status = JobStatus::Failed;
                failed_job.message = Some(format!("Flush failed: {}", e));
                failed_job.updated_at = now_ms;
                failed_job.finished_at = Some(now_ms);
                
                // Return error
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::NamespaceId;

    struct MockFlushJob {
        should_fail: bool,
        rows_count: usize,
        namespace_id: NamespaceId,
    }

    impl TableFlush for MockFlushJob {
        fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
            if self.should_fail {
                return Err(KalamDbError::Other("Mock failure".to_string()));
            }

            Ok(FlushJobResult {
                job_record: self.create_job_record("test-job-123", self.namespace_id.clone()),
                rows_flushed: self.rows_count,
                parquet_files: vec!["batch-123.parquet".to_string()],
                metadata: FlushMetadata::shared_table(),
            })
        }

        fn table_identifier(&self) -> String {
            "test_namespace.test_table".to_string()
        }
    }

    #[test]
    fn test_generate_job_id() {
        let job = MockFlushJob {
            should_fail: false,
            rows_count: 0,
            namespace_id: NamespaceId::new("test".to_string()),
        };

        let job_id = job.generate_job_id();
        assert!(job_id.starts_with("flush-test_namespace-test_table-"));
    }

    #[test]
    fn test_create_job_record() {
        let job = MockFlushJob {
            should_fail: false,
            rows_count: 0,
            namespace_id: NamespaceId::new("test".to_string()),
        };

        let record = job.create_job_record("test-123", job.namespace_id.clone());
        assert_eq!(record.job_id.as_str(), "test-123");
        assert_eq!(record.job_type, JobType::Flush);
        assert_eq!(record.status, kalamdb_commons::JobStatus::Running);
    }

    #[test]
    fn test_flush_executor_success() {
        let namespace_id = NamespaceId::new("test".to_string());
        let job = MockFlushJob {
            should_fail: false,
            rows_count: 100,
            namespace_id: namespace_id.clone(),
        };

        let result = FlushExecutor::execute_with_tracking(&job, namespace_id);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.rows_flushed, 100);
        assert_eq!(result.parquet_files.len(), 1);
        assert_eq!(
            result.job_record.status,
            kalamdb_commons::JobStatus::Completed
        );
    assert!(result.job_record.message.is_some());
    }

    #[test]
    fn test_flush_executor_failure() {
        let namespace_id = NamespaceId::new("test".to_string());
        let job = MockFlushJob {
            should_fail: true,
            rows_count: 0,
            namespace_id: namespace_id.clone(),
        };

        let result = FlushExecutor::execute_with_tracking(&job, namespace_id);
        assert!(result.is_err());

        match result {
            Err(KalamDbError::Other(msg)) => {
                assert_eq!(msg, "Mock failure");
            }
            _ => panic!("Expected Other error"),
        }
    }

    #[test]
    fn test_table_identifier() {
        let namespace_id = NamespaceId::new("test".to_string());
        let job = MockFlushJob {
            should_fail: false,
            rows_count: 0,
            namespace_id,
        };

        assert_eq!(job.table_identifier(), "test_namespace.test_table");
    }

    #[test]
    fn test_node_id_default() {
        let namespace_id = NamespaceId::new("test".to_string());
        let job = MockFlushJob {
            should_fail: false,
            rows_count: 0,
            namespace_id,
        };

        let node_id = job.node_id();
        assert!(node_id.as_str().starts_with("node-"));
    }
}
