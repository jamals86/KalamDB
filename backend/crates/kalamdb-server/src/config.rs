// Configuration module
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Main server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub server: ServerSettings,
    pub storage: StorageSettings,
    pub limits: LimitsSettings,
    pub logging: LoggingSettings,
    pub performance: PerformanceSettings,
    #[serde(default)]
    pub datafusion: DataFusionSettings,
    #[serde(default)]
    pub flush: FlushSettings,
    #[serde(default)]
    pub retention: RetentionSettings,
    #[serde(default)]
    pub stream: StreamSettings,
    #[serde(default)]
    pub rate_limit: RateLimitSettings,
}

/// Server settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_workers")]
    pub workers: usize,
    /// API version prefix for endpoints (default: "v1")
    #[serde(default = "default_api_version")]
    pub api_version: String,
    /// Timeout in seconds to wait for flush jobs to complete during graceful shutdown (T158j)
    #[serde(default = "default_flush_job_shutdown_timeout")]
    pub flush_job_shutdown_timeout_seconds: u32,
}

/// Storage settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettings {
    pub rocksdb_path: String,
    /// Default path for 'local' storage when base_directory='' (T164a)
    #[serde(default = "default_storage_path")]
    pub default_storage_path: String,
    #[serde(default = "default_true")]
    pub enable_wal: bool,
    #[serde(default = "default_compression")]
    pub compression: String,
    #[serde(default)]
    pub rocksdb: RocksDbSettings,
}

/// RocksDB-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksDbSettings {
    /// Write buffer size per column family in bytes (default: 64MB)
    #[serde(default = "default_rocksdb_write_buffer_size")]
    pub write_buffer_size: usize,

    /// Maximum number of write buffers (default: 3)
    #[serde(default = "default_rocksdb_max_write_buffers")]
    pub max_write_buffers: i32,

    /// Block cache size for reads in bytes (default: 256MB)
    #[serde(default = "default_rocksdb_block_cache_size")]
    pub block_cache_size: usize,

    /// Maximum number of background jobs (default: 4)
    #[serde(default = "default_rocksdb_max_background_jobs")]
    pub max_background_jobs: i32,
}

impl Default for RocksDbSettings {
    fn default() -> Self {
        Self {
            write_buffer_size: default_rocksdb_write_buffer_size(),
            max_write_buffers: default_rocksdb_max_write_buffers(),
            block_cache_size: default_rocksdb_block_cache_size(),
            max_background_jobs: default_rocksdb_max_background_jobs(),
        }
    }
}

/// Limits settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsSettings {
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,
    #[serde(default = "default_max_query_limit")]
    pub max_query_limit: usize,
    #[serde(default = "default_query_limit")]
    pub default_query_limit: usize,
}

/// Logging settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSettings {
    #[serde(default = "default_log_level")]
    pub level: String,
    pub file_path: String,
    #[serde(default = "default_true")]
    pub log_to_console: bool,
    #[serde(default = "default_log_format")]
    pub format: String,
}

/// Performance settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSettings {
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
    #[serde(default = "default_keepalive_timeout")]
    pub keepalive_timeout: u64,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

/// DataFusion settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFusionSettings {
    /// Memory limit for query execution in bytes (default: 1GB)
    #[serde(default = "default_datafusion_memory_limit")]
    pub memory_limit: usize,

    /// Number of parallel threads for query execution (default: number of CPU cores)
    #[serde(default = "default_datafusion_parallelism")]
    pub query_parallelism: usize,

    /// Maximum number of partitions per query (default: 16)
    #[serde(default = "default_datafusion_max_partitions")]
    pub max_partitions: usize,

    /// Batch size for record processing (default: 8192)
    #[serde(default = "default_datafusion_batch_size")]
    pub batch_size: usize,
}

/// Flush policy defaults
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlushSettings {
    /// Default row limit for flush (default: 10000 rows)
    #[serde(default = "default_flush_row_limit")]
    pub default_row_limit: usize,

    /// Default time interval for flush in seconds (default: 300s = 5 minutes)
    #[serde(default = "default_flush_time_interval")]
    pub default_time_interval: u64,
}

/// Retention policy defaults
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionSettings {
    /// Default retention hours for soft-deleted rows (default: 168 hours = 7 days)
    #[serde(default = "default_deleted_retention_hours")]
    pub default_deleted_retention_hours: i32,
}

/// Stream table defaults
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSettings {
    /// Default TTL for stream table rows in seconds (default: 10 seconds)
    #[serde(default = "default_stream_ttl")]
    pub default_ttl_seconds: u64,

    /// Default maximum buffer size for stream tables (default: 10000 rows)
    #[serde(default = "default_stream_max_buffer")]
    pub default_max_buffer: usize,
}

/// Rate limiter settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitSettings {
    /// Maximum queries per second per user (default: 100)
    #[serde(default = "default_rate_limit_queries_per_sec")]
    pub max_queries_per_sec: u32,

    /// Maximum WebSocket messages per second per connection (default: 50)
    #[serde(default = "default_rate_limit_messages_per_sec")]
    pub max_messages_per_sec: u32,

    /// Maximum concurrent subscriptions per user (default: 10)
    #[serde(default = "default_rate_limit_max_subscriptions")]
    pub max_subscriptions_per_user: u32,
}

impl Default for DataFusionSettings {
    fn default() -> Self {
        Self {
            memory_limit: default_datafusion_memory_limit(),
            query_parallelism: default_datafusion_parallelism(),
            max_partitions: default_datafusion_max_partitions(),
            batch_size: default_datafusion_batch_size(),
        }
    }
}

impl Default for FlushSettings {
    fn default() -> Self {
        Self {
            default_row_limit: default_flush_row_limit(),
            default_time_interval: default_flush_time_interval(),
        }
    }
}

impl Default for RetentionSettings {
    fn default() -> Self {
        Self {
            default_deleted_retention_hours: default_deleted_retention_hours(),
        }
    }
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            default_ttl_seconds: default_stream_ttl(),
            default_max_buffer: default_stream_max_buffer(),
        }
    }
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            max_queries_per_sec: default_rate_limit_queries_per_sec(),
            max_messages_per_sec: default_rate_limit_messages_per_sec(),
            max_subscriptions_per_user: default_rate_limit_max_subscriptions(),
        }
    }
}

// Default value functions
fn default_workers() -> usize {
    0
}

fn default_api_version() -> String {
    "v1".to_string()
}

fn default_flush_job_shutdown_timeout() -> u32 {
    300 // 5 minutes (T158j)
}

fn default_true() -> bool {
    true
}

fn default_compression() -> String {
    "lz4".to_string()
}

fn default_storage_path() -> String {
    "./data/storage".to_string() // T164a: Default path for 'local' storage
}

fn default_max_message_size() -> usize {
    1048576 // 1MB
}

fn default_max_query_limit() -> usize {
    1000
}

fn default_query_limit() -> usize {
    50
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "compact".to_string()
}

fn default_request_timeout() -> u64 {
    30
}

fn default_keepalive_timeout() -> u64 {
    75
}

fn default_max_connections() -> usize {
    25000
}

// DataFusion defaults
fn default_datafusion_memory_limit() -> usize {
    1024 * 1024 * 1024 // 1GB
}

fn default_datafusion_parallelism() -> usize {
    num_cpus::get()
}

fn default_datafusion_max_partitions() -> usize {
    16
}

fn default_datafusion_batch_size() -> usize {
    8192
}

// Flush defaults
fn default_flush_row_limit() -> usize {
    10000
}

fn default_flush_time_interval() -> u64 {
    300 // 5 minutes
}

// Retention defaults
fn default_deleted_retention_hours() -> i32 {
    168 // 7 days
}

// Stream defaults
fn default_stream_ttl() -> u64 {
    10 // 10 seconds
}

fn default_stream_max_buffer() -> usize {
    10000
}

// Rate limiter defaults
fn default_rate_limit_queries_per_sec() -> u32 {
    100 // 100 queries per second per user
}

fn default_rate_limit_messages_per_sec() -> u32 {
    50 // 50 messages per second per WebSocket connection
}

fn default_rate_limit_max_subscriptions() -> u32 {
    10 // 10 concurrent subscriptions per user
}

// RocksDB defaults
fn default_rocksdb_write_buffer_size() -> usize {
    64 * 1024 * 1024 // 64MB
}

fn default_rocksdb_max_write_buffers() -> i32 {
    3
}

fn default_rocksdb_block_cache_size() -> usize {
    256 * 1024 * 1024 // 256MB
}

fn default_rocksdb_max_background_jobs() -> i32 {
    4
}

impl ServerConfig {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;

        let mut config: ServerConfig = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;

        // Override with environment variables if present
        config.apply_env_overrides()?;

        config.validate()?;

        Ok(config)
    }

    /// Apply environment variable overrides for sensitive configuration
    ///
    /// Supported environment variables:
    /// - KALAMDB_ROCKSDB_PATH: Override storage.rocksdb_path
    /// - KALAMDB_LOG_FILE_PATH: Override logging.file_path
    /// - KALAMDB_HOST: Override server.host
    /// - KALAMDB_PORT: Override server.port
    fn apply_env_overrides(&mut self) -> anyhow::Result<()> {
        use std::env;

        // Database path (sensitive - may contain credentials in cloud storage URLs)
        if let Ok(path) = env::var("KALAMDB_ROCKSDB_PATH") {
            self.storage.rocksdb_path = path;
        }

        // Log file path
        if let Ok(path) = env::var("KALAMDB_LOG_FILE_PATH") {
            self.logging.file_path = path;
        }

        // Server host
        if let Ok(host) = env::var("KALAMDB_HOST") {
            self.server.host = host;
        }

        // Server port
        if let Ok(port_str) = env::var("KALAMDB_PORT") {
            self.server.port = port_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid KALAMDB_PORT value: {}", port_str))?;
        }

        Ok(())
    }

    /// Validate configuration settings
    pub fn validate(&self) -> anyhow::Result<()> {
        // Validate port range
        if self.server.port == 0 {
            return Err(anyhow::anyhow!("Server port cannot be 0"));
        }

        // Validate log level
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid log level '{}'. Must be one of: {}",
                self.logging.level,
                valid_levels.join(", ")
            ));
        }

        // Validate compression type
        let valid_compressions = ["none", "snappy", "zlib", "lz4", "zstd"];
        if !valid_compressions.contains(&self.storage.compression.as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid compression type '{}'. Must be one of: {}",
                self.storage.compression,
                valid_compressions.join(", ")
            ));
        }

        // Validate log format
        let valid_formats = ["compact", "pretty", "json"];
        if !valid_formats.contains(&self.logging.format.as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid log format '{}'. Must be one of: {}",
                self.logging.format,
                valid_formats.join(", ")
            ));
        }

        // Validate message size limit
        if self.limits.max_message_size == 0 {
            return Err(anyhow::anyhow!("max_message_size cannot be 0"));
        }

        // Validate query limits
        if self.limits.max_query_limit == 0 {
            return Err(anyhow::anyhow!("max_query_limit cannot be 0"));
        }

        if self.limits.default_query_limit > self.limits.max_query_limit {
            return Err(anyhow::anyhow!(
                "default_query_limit ({}) cannot exceed max_query_limit ({})",
                self.limits.default_query_limit,
                self.limits.max_query_limit
            ));
        }

        Ok(())
    }

    /// Get default configuration (useful for testing)
    pub fn default() -> Self {
        ServerConfig {
            server: ServerSettings {
                host: "127.0.0.1".to_string(),
                port: 8080,
                workers: 0,
                api_version: default_api_version(),
                flush_job_shutdown_timeout_seconds: default_flush_job_shutdown_timeout(),
            },
            storage: StorageSettings {
                rocksdb_path: "./data/rocksdb".to_string(),
                default_storage_path: default_storage_path(),
                enable_wal: true,
                compression: "lz4".to_string(),
                rocksdb: RocksDbSettings::default(),
            },
            limits: LimitsSettings {
                max_message_size: 1048576,
                max_query_limit: 1000,
                default_query_limit: 50,
            },
            logging: LoggingSettings {
                level: "info".to_string(),
                file_path: "./logs/app.log".to_string(),
                log_to_console: true,
                format: "compact".to_string(),
            },
            performance: PerformanceSettings {
                request_timeout: 30,
                keepalive_timeout: 75,
                max_connections: 25000,
            },
            datafusion: DataFusionSettings::default(),
            flush: FlushSettings::default(),
            retention: RetentionSettings::default(),
            stream: StreamSettings::default(),
            rate_limit: RateLimitSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = ServerConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_port() {
        let mut config = ServerConfig::default();
        config.server.port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_log_level() {
        let mut config = ServerConfig::default();
        config.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_compression() {
        let mut config = ServerConfig::default();
        config.storage.compression = "invalid".to_string();
        assert!(config.validate().is_err());
    }
}
