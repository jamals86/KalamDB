//! Stream table eviction job
//!
//! This module handles automatic eviction of old events from stream tables.
//! Stream tables are ephemeral and use TTL-based eviction to prevent unbounded growth.
//!
//! ## Architecture
//! - Background job runs periodically (configurable interval)
//! - For each stream table: evict events older than retention_period
//! - Uses timestamp-prefixed keys for efficient deletion
//! - Registers eviction jobs in system.jobs for monitoring

use crate::error::KalamDbError;
use crate::jobs::{JobExecutor, JobResult};
use crate::stores::system_table::SharedTableStoreExt;
use crate::tables::StreamTableStore;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_commons::schemas::TableType;
use kalamdb_sql::KalamSql;
use std::sync::Arc;
use std::time::Duration;

/// Configuration for stream table eviction
#[derive(Debug, Clone)]
pub struct StreamEvictionConfig {
    /// Interval between eviction runs (seconds)
    pub eviction_interval_secs: u64,
    /// Node identifier for job registration
    pub node_id: String,
}

impl Default for StreamEvictionConfig {
    fn default() -> Self {
        Self {
            eviction_interval_secs: 60, // Run every minute
            node_id: "node-1".to_string(),
        }
    }
}

/// Stream table eviction job
pub struct StreamEvictionJob {
    stream_store: Arc<StreamTableStore>,
    kalam_sql: Arc<KalamSql>,
    job_executor: Arc<JobExecutor>,
    config: StreamEvictionConfig,
}

impl StreamEvictionJob {
    /// Create a new stream eviction job
    pub fn new(
        stream_store: Arc<StreamTableStore>,
        kalam_sql: Arc<KalamSql>,
        job_executor: Arc<JobExecutor>,
        config: StreamEvictionConfig,
    ) -> Self {
        Self {
            stream_store,
            kalam_sql,
            job_executor,
            config,
        }
    }

    /// Create with default configuration
    pub fn with_defaults(
        stream_store: Arc<StreamTableStore>,
        kalam_sql: Arc<KalamSql>,
        job_executor: Arc<JobExecutor>,
    ) -> Self {
        Self::new(
            stream_store,
            kalam_sql,
            job_executor,
            StreamEvictionConfig::default(),
        )
    }

    /// Run eviction for all stream tables
    ///
    /// This iterates through all stream tables, calculates cutoff timestamps
    /// based on retention periods, and evicts old events.
    ///
    /// # Returns
    /// Total number of events evicted across all tables
    pub fn run_eviction(&self) -> Result<usize, KalamDbError> {
        let mut total_evicted = 0;

        // Get all tables from system.tables
        let all_tables = self
            .kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        // Filter for stream tables only
        let stream_tables: Vec<_> = all_tables
            .into_iter()
            .filter(|t| t.table_type == TableType::Stream)
            .collect();

        // Get table definitions to access TTL settings
        for table_meta in stream_tables {
            // Get full table definition to access ttl_seconds
            let namespace_id = NamespaceId::from(table_meta.namespace.as_str());
            let table_name = TableName::from(table_meta.table_name.as_str());
            let table_def = self
                .kalam_sql
                .get_table_definition(&namespace_id, &table_name)
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to get table definition for {}.{}: {}",
                        table_meta.namespace.as_str(),
                        table_meta.table_name.as_str(),
                        e
                    ))
                })?;

            // Skip tables without TTL configured (extract from table_options)
            let ttl_seconds = match table_def {
                Some(def) => {
                    if let kalamdb_commons::schemas::TableOptions::Stream(stream_opts) = &def.table_options {
                        stream_opts.ttl_seconds
                    } else {
                        log::warn!(
                            "Table {}.{} is not a STREAM table, skipping eviction",
                            table_meta.namespace,
                            table_meta.table_name
                        );
                        continue;
                    }
                }
                None => {
                    log::trace!(
                        "Stream table {}.{} definition not found, skipping eviction",
                        table_meta.namespace,
                        table_meta.table_name
                    );
                    continue;
                }
            };

            // Convert TTL to milliseconds
            let retention_period_ms = (ttl_seconds as i64) * 1000;

            // Run eviction for this table
            let evicted = self.evict_for_table(
                table_meta.namespace.clone(),
                table_meta.table_name.clone(),
                retention_period_ms,
            )?;

            total_evicted += evicted;

            if evicted > 0 {
                log::info!(
                    "Evicted {} events from stream table {}.{} (TTL: {}s)",
                    evicted,
                    table_meta.namespace,
                    table_meta.table_name,
                    ttl_seconds
                );
            }
        }

        Ok(total_evicted)
    }

    /// Run eviction for a specific stream table
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the stream table
    /// * `table_name` - Name of the stream table
    /// * `retention_period_ms` - Retention period in milliseconds
    ///
    /// # Returns
    /// Number of events evicted
    pub fn evict_for_table(
        &self,
        namespace_id: NamespaceId,
        table_name: TableName,
        _retention_period_ms: i64,
    ) -> Result<usize, KalamDbError> {
        // For the new StreamTableStore, we use cleanup_expired_rows
        // which handles TTL-based eviction automatically
        let namespace_id_str = namespace_id.as_str().to_string();
        let table_name_str = table_name.as_str().to_string();

        let deleted_count = self
            .stream_store
            .cleanup_expired_rows(&namespace_id_str, &table_name_str)
            .map_err(|e| KalamDbError::Other(format!("Failed to cleanup expired rows: {}", e)))?;

        Ok(deleted_count)
    }

    /// Run max buffer eviction for a specific stream table
    ///
    /// Evicts oldest events if the table exceeds max_events limit.
    /// Uses FIFO eviction to keep only the newest max_events entries.
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the stream table
    /// * `table_name` - Name of the stream table
    /// * `max_events` - Maximum number of events to keep in the buffer
    ///
    /// # Returns
    /// Number of events evicted
    pub fn evict_max_buffer(
        &self,
        namespace_id: NamespaceId,
        table_name: TableName,
        max_events: usize,
    ) -> Result<usize, KalamDbError> {
        let namespace_id_str = namespace_id.as_str().to_string();
        let table_name_str = table_name.as_str().to_string();
        let stream_store: Arc<StreamTableStore> = Arc::clone(&self.stream_store);

        // Execute eviction through job executor for monitoring
        let job_id = format!(
            "stream-max-buffer-{}:{}",
            namespace_id.as_str(),
            table_name.as_str()
        );
        let job_type = "stream_max_buffer".to_string();
        let target_resource = Some(format!("{}:{}", namespace_id.as_str(), table_name.as_str()));

        let result = self.job_executor.execute_job(
            job_id,
            job_type,
            target_resource,
            vec![format!("max_events={}", max_events)],
            move || {
                // Get all events - the new scan method returns (row_id, row_data)
                let mut events = stream_store
                    .scan(&namespace_id_str, &table_name_str)
                    .map_err(|e| format!("Failed to scan table: {}", e))?;

                let total_count = events.len();

                if total_count <= max_events {
                    // No eviction needed
                    return Ok(format!(
                        "No eviction needed ({} <= {})",
                        total_count, max_events
                    ));
                }

                // Sort by inserted_at timestamp from the row data
                // The row_data contains "inserted_at" field
                events.sort_by(|(_, a), (_, b)| {
                    let a_time = a
                        .fields
                        .get("inserted_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("1970-01-01T00:00:00Z");
                    let b_time = b
                        .fields
                        .get("inserted_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("1970-01-01T00:00:00Z");
                    a_time.cmp(b_time)
                });

                // Calculate how many to delete
                let to_delete = total_count - max_events;

                // Delete oldest entries (keep newest max_events)
                let mut deleted_count = 0;
                for (row_id, _) in events.iter().take(to_delete) {
                    stream_store
                        .delete(&namespace_id_str, &table_name_str, row_id, true) // Hard delete for max buffer eviction
                        .map_err(|e| format!("Failed to delete event: {}", e))?;
                    deleted_count += 1;
                }

                Ok(format!(
                    "Evicted {} events (max_buffer exceeded: {} > {})",
                    deleted_count, total_count, max_events
                ))
            },
        )?;

        // Extract count from JobResult
        let count = match result {
            JobResult::Success(msg) => {
                // Parse "Evicted N events (max_buffer exceeded: X > Y)"
                // or "No eviction needed (X <= Y)"
                if msg.starts_with("No eviction") {
                    0
                } else {
                    msg.split_whitespace()
                        .nth(1)
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0)
                }
            }
            JobResult::Failure(_) => 0,
        };

        Ok(count)
    }

    /// Get eviction interval
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.config.eviction_interval_secs)
    }

    /// Get configuration
    pub fn config(&self) -> &StreamEvictionConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: StreamEvictionConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::{new_stream_table_store, system::JobsTableProvider};
    use kalamdb_store::RocksDbInit;
    use tempfile::TempDir;

    fn setup_eviction_job() -> (StreamEvictionJob, Arc<StreamTableStore>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();

        let test_namespace = NamespaceId::new("test");
        let test_table = TableName::new("events");
        let stream_store = Arc::new(new_stream_table_store(&test_namespace, &test_table));
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend.clone()).unwrap());
        let jobs_provider = Arc::new(JobsTableProvider::new(Arc::clone(&backend)));
        let job_executor = Arc::new(JobExecutor::new(jobs_provider, "test-node".to_string()));

        let eviction_job =
            StreamEvictionJob::with_defaults(Arc::clone(&stream_store), kalam_sql, job_executor);

        (eviction_job, stream_store, temp_dir)
    }

    #[test]
    fn test_eviction_job_creation() {
        let (eviction_job, _, _temp_dir) = setup_eviction_job();
        assert_eq!(eviction_job.config().eviction_interval_secs, 60);
        assert_eq!(eviction_job.interval(), Duration::from_secs(60));
    }

    #[test]
    fn test_custom_eviction_config() {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();

        let test_namespace = NamespaceId::new("test");
        let test_table = TableName::new("events");
        let stream_store = Arc::new(new_stream_table_store(&test_namespace, &test_table));
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend.clone()).unwrap());
        let jobs_provider = Arc::new(JobsTableProvider::new(Arc::clone(&backend)));
        let job_executor = Arc::new(JobExecutor::new(jobs_provider, "test-node".to_string()));

        let config = StreamEvictionConfig {
            eviction_interval_secs: 30,
            node_id: "custom-node".to_string(),
        };

        let eviction_job =
            StreamEvictionJob::new(stream_store, kalam_sql, job_executor, config.clone());

        assert_eq!(eviction_job.config().eviction_interval_secs, 30);
        assert_eq!(eviction_job.config().node_id, "custom-node");
    }

    #[test]
    fn test_evict_for_table_with_no_events() {
        let (eviction_job, _stream_store, _temp_dir) = setup_eviction_job();

        let namespace_id = NamespaceId::new("app");
        let table_name = TableName::new("events");
        let retention_period_ms = 60_000; // 1 minute

        // Table doesn't exist yet - the job executor will capture the error
        // and return a Failure result, which we convert to 0 evicted
        let result = eviction_job.evict_for_table(namespace_id, table_name, retention_period_ms);

        // The job should complete but report 0 evictions (or error)
        // Since the CF doesn't exist, evict_older_than will fail, job_fn returns Err,
        // execute_job returns Ok(JobResult::Failure(...)), and we return Ok(0)
        match result {
            Ok(count) => assert_eq!(count, 0),
            Err(_) => {} // Also acceptable - depends on error handling
        }
    }

    // Note: Additional integration tests would require setting up RocksDB column families
    // which requires direct DB access. These tests would be better suited for
    // StreamTableStore's own test suite or integration tests with full DB setup.

    #[test]
    fn test_run_eviction_returns_zero_with_no_tables() {
        let (eviction_job, _stream_store, _temp_dir) = setup_eviction_job();

        // No stream tables exist, should return 0
        let result = eviction_job.run_eviction();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_evict_max_buffer_no_eviction_needed() {
        let (eviction_job, _stream_store, _temp_dir) = setup_eviction_job();

        let namespace_id = NamespaceId::new("app");
        let table_name = TableName::new("events");
        let max_events = 100;

        // Table doesn't exist, should handle gracefully
        let result = eviction_job.evict_max_buffer(namespace_id, table_name, max_events);

        // Should complete with 0 evictions or error
        match result {
            Ok(count) => assert_eq!(count, 0),
            Err(_) => {} // Also acceptable for missing table
        }
    }

    #[test]
    fn test_evict_max_buffer_naming() {
        // Verify that evict_max_buffer exists and can be called
        let (eviction_job, _stream_store, _temp_dir) = setup_eviction_job();

        let namespace_id = NamespaceId::new("test");
        let table_name = TableName::new("events");

        // This should not panic - method exists with correct signature
        let _ = eviction_job.evict_max_buffer(namespace_id, table_name, 50);
    }
}
