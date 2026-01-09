//! system.settings virtual view
//!
//! **Type**: Virtual View (not backed by persistent storage)
//!
//! Provides server configuration settings as a queryable table with columns:
//! - name: Setting name (e.g., "server.host", "storage.rocksdb_path")
//! - value: Current setting value as string
//! - description: Human-readable description of the setting
//! - category: Setting category (e.g., "server", "storage", "limits")
//!
//! **DataFusion Pattern**: Implements VirtualView trait for consistent view behavior

use super::view_base::VirtualView;
use datafusion::arrow::array::{ArrayRef, StringBuilder};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::config::ServerConfig;
use std::sync::{Arc, OnceLock, RwLock};

/// Static schema for system.settings
static SETTINGS_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Get or initialize the settings schema
fn settings_schema() -> SchemaRef {
    SETTINGS_SCHEMA
        .get_or_init(|| {
            Arc::new(Schema::new(vec![
                Field::new("name", DataType::Utf8, false),
                Field::new("value", DataType::Utf8, false),
                Field::new("description", DataType::Utf8, false),
                Field::new("category", DataType::Utf8, false),
            ]))
        })
        .clone()
}

/// Virtual view that displays server configuration settings
///
/// **DataFusion Design**:
/// - Implements VirtualView trait
/// - Returns TableType::View
/// - Computes batch dynamically from ServerConfig
#[derive(Debug)]
pub struct SettingsView {
    config: Arc<RwLock<Option<ServerConfig>>>,
}

impl SettingsView {
    /// Create a new settings view
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new settings view with config
    pub fn with_config(config: ServerConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(Some(config))),
        }
    }

    /// Set the configuration (called after AppContext init)
    pub fn set_config(&self, config: ServerConfig) {
        *self.config.write().unwrap() = Some(config);
    }

    /// Helper to add a setting row
    fn add_setting(
        names: &mut StringBuilder,
        values: &mut StringBuilder,
        descriptions: &mut StringBuilder,
        categories: &mut StringBuilder,
        name: &str,
        value: &str,
        description: &str,
        category: &str,
    ) {
        names.append_value(name);
        values.append_value(value);
        descriptions.append_value(description);
        categories.append_value(category);
    }
}

impl Default for SettingsView {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualView for SettingsView {
    fn schema(&self) -> SchemaRef {
        settings_schema()
    }

    fn compute_batch(&self) -> Result<RecordBatch, super::super::error::RegistryError> {
        let mut names = StringBuilder::new();
        let mut values = StringBuilder::new();
        let mut descriptions = StringBuilder::new();
        let mut categories = StringBuilder::new();

        let config_guard = self.config.read().unwrap();

        if let Some(config) = config_guard.as_ref() {
            // ===== Server Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "server.host",
                &config.server.host,
                "Server bind address",
                "server",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "server.port",
                &config.server.port.to_string(),
                "Server port number",
                "server",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "server.workers",
                &config.server.workers.to_string(),
                "Number of worker threads (0 = auto-detect CPU cores)",
                "server",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "server.enable_http2",
                &config.server.enable_http2.to_string(),
                "Enable HTTP/2 protocol support",
                "server",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "server.api_version",
                &config.server.api_version,
                "API version prefix (e.g., v1)",
                "server",
            );

            // ===== Cluster Settings =====
            if let Some(cluster) = &config.cluster {
                Self::add_setting(
                    &mut names,
                    &mut values,
                    &mut descriptions,
                    &mut categories,
                    "cluster.node_id",
                    &cluster.node_id.to_string(),
                    "Authoritative node identifier for cluster",
                    "cluster",
                );
                Self::add_setting(
                    &mut names,
                    &mut values,
                    &mut descriptions,
                    &mut categories,
                    "cluster.cluster_id",
                    &cluster.cluster_id,
                    "Unique cluster identifier",
                    "cluster",
                );
            }

            // ===== Storage Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "storage.rocksdb_path",
                &config.storage.rocksdb_path,
                "RocksDB data directory path",
                "storage",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "storage.default_storage_path",
                &config.storage.default_storage_path,
                "Default path for Parquet file storage",
                "storage",
            );

            // ===== RocksDB Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "storage.rocksdb.write_buffer_size",
                &config.storage.rocksdb.write_buffer_size.to_string(),
                "Write buffer size per column family in bytes",
                "storage",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "storage.rocksdb.max_write_buffers",
                &config.storage.rocksdb.max_write_buffers.to_string(),
                "Maximum number of write buffers",
                "storage",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "storage.rocksdb.block_cache_size",
                &config.storage.rocksdb.block_cache_size.to_string(),
                "Block cache size for reads in bytes",
                "storage",
            );

            // ===== DataFusion Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "datafusion.memory_limit",
                &config.datafusion.memory_limit.to_string(),
                "Memory limit for query execution in bytes",
                "datafusion",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "datafusion.query_parallelism",
                &config.datafusion.query_parallelism.to_string(),
                "Number of parallel threads for query execution",
                "datafusion",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "datafusion.batch_size",
                &config.datafusion.batch_size.to_string(),
                "Batch size for record processing",
                "datafusion",
            );

            // ===== Flush Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "flush.default_row_limit",
                &config.flush.default_row_limit.to_string(),
                "Default row limit for flush policies",
                "flush",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "flush.default_time_interval",
                &config.flush.default_time_interval.to_string(),
                "Default time interval for flush in seconds",
                "flush",
            );

            // ===== Limits Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "limits.max_message_size",
                &config.limits.max_message_size.to_string(),
                "Maximum message size for REST API requests in bytes",
                "limits",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "limits.max_query_limit",
                &config.limits.max_query_limit.to_string(),
                "Maximum rows that can be returned in a single query",
                "limits",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "limits.default_query_limit",
                &config.limits.default_query_limit.to_string(),
                "Default LIMIT for queries without explicit LIMIT clause",
                "limits",
            );

            // ===== Logging Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "logging.level",
                &config.logging.level,
                "Log level: error, warn, info, debug, trace",
                "logging",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "logging.logs_path",
                &config.logging.logs_path,
                "Directory for log files",
                "logging",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "logging.log_to_console",
                &config.logging.log_to_console.to_string(),
                "Also log to console/stdout",
                "logging",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "logging.slow_query_threshold_ms",
                &config.logging.slow_query_threshold_ms.to_string(),
                "Slow query logging threshold in milliseconds",
                "logging",
            );

            // ===== Performance Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "performance.request_timeout",
                &config.performance.request_timeout.to_string(),
                "Request timeout in seconds",
                "performance",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "performance.max_connections",
                &config.performance.max_connections.to_string(),
                "Maximum concurrent connections",
                "performance",
            );

            // ===== Stream Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "stream.default_ttl_seconds",
                &config.stream.default_ttl_seconds.to_string(),
                "Default TTL for stream table rows in seconds",
                "stream",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "stream.default_max_buffer",
                &config.stream.default_max_buffer.to_string(),
                "Default maximum buffer size for stream tables",
                "stream",
            );

            // ===== Retention Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "retention.default_deleted_retention_hours",
                &config.retention.default_deleted_retention_hours.to_string(),
                "Default retention hours for soft-deleted rows",
                "retention",
            );

            // ===== Rate Limit Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "rate_limit.max_queries_per_sec",
                &config.rate_limit.max_queries_per_sec.to_string(),
                "Maximum SQL queries per second per user",
                "rate_limit",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "rate_limit.max_subscriptions_per_user",
                &config.rate_limit.max_subscriptions_per_user.to_string(),
                "Maximum concurrent live query subscriptions per user",
                "rate_limit",
            );

            // ===== Auth Settings (redacted sensitive values) =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "auth.bcrypt_cost",
                &config.auth.bcrypt_cost.to_string(),
                "Bcrypt cost factor for password hashing",
                "auth",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "auth.min_password_length",
                &config.auth.min_password_length.to_string(),
                "Minimum password length",
                "auth",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "auth.allow_remote_access",
                &config.auth.allow_remote_access.to_string(),
                "Allow remote access for system users",
                "auth",
            );
            // Note: jwt_secret is intentionally not exposed for security

            // ===== Jobs Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "jobs.max_concurrent",
                &config.jobs.max_concurrent.to_string(),
                "Maximum number of concurrent jobs",
                "jobs",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "jobs.max_retries",
                &config.jobs.max_retries.to_string(),
                "Maximum number of retry attempts per job",
                "jobs",
            );

            // ===== Execution Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "execution.handler_timeout_seconds",
                &config.execution.handler_timeout_seconds.to_string(),
                "Handler execution timeout in seconds",
                "execution",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "execution.max_parameters",
                &config.execution.max_parameters.to_string(),
                "Maximum number of parameters per statement",
                "execution",
            );

            // ===== Security Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "security.max_request_body_size",
                &config.security.max_request_body_size.to_string(),
                "Maximum request body size in bytes",
                "security",
            );
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "security.max_ws_message_size",
                &config.security.max_ws_message_size.to_string(),
                "Maximum WebSocket message size in bytes",
                "security",
            );

            // ===== Shutdown Settings =====
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "shutdown.flush.timeout",
                &config.shutdown.flush.timeout.to_string(),
                "Timeout in seconds to wait for flush jobs during shutdown",
                "shutdown",
            );
        } else {
            // No config available yet
            Self::add_setting(
                &mut names,
                &mut values,
                &mut descriptions,
                &mut categories,
                "status",
                "not_initialized",
                "Configuration not yet loaded",
                "system",
            );
        }

        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(names.finish()) as ArrayRef,
                Arc::new(values.finish()) as ArrayRef,
                Arc::new(descriptions.finish()) as ArrayRef,
                Arc::new(categories.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| {
            super::super::error::RegistryError::Other(format!(
                "Failed to build settings batch: {}",
                e
            ))
        })
    }

    fn view_name(&self) -> &str {
        "system.settings"
    }
}

// Re-export as SettingsTableProvider for consistency
pub type SettingsTableProvider = super::view_base::ViewTableProvider<SettingsView>;

/// Helper function to create a settings table provider
pub fn create_settings_provider() -> SettingsTableProvider {
    SettingsTableProvider::new(Arc::new(SettingsView::new()))
}

/// Helper function to create a settings table provider with config
pub fn create_settings_provider_with_config(config: ServerConfig) -> SettingsTableProvider {
    SettingsTableProvider::new(Arc::new(SettingsView::with_config(config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_view_schema() {
        let view = SettingsView::new();
        let schema = view.schema();

        assert_eq!(schema.fields().len(), 4);
        assert_eq!(schema.field(0).name(), "name");
        assert_eq!(schema.field(1).name(), "value");
        assert_eq!(schema.field(2).name(), "description");
        assert_eq!(schema.field(3).name(), "category");
    }

    #[test]
    fn test_settings_view_without_config() {
        let view = SettingsView::new();
        let batch = view.compute_batch().unwrap();

        // Should have one row with "not_initialized" status
        assert_eq!(batch.num_rows(), 1);
    }

    #[test]
    fn test_settings_view_with_config() {
        let config = ServerConfig::default();
        let view = SettingsView::with_config(config);
        let batch = view.compute_batch().unwrap();

        // Should have multiple settings rows
        assert!(batch.num_rows() > 10);
    }
}
