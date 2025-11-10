// Configuration module
use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::HashMap;
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
    #[serde(default, alias = "authentication")]
    pub auth: AuthSettings,
    #[serde(default)]
    pub oauth: OAuthSettings,
    #[serde(default)]
    pub user_management: UserManagementSettings,
    #[serde(default)]
    pub shutdown: ShutdownSettings,
    #[serde(default)]
    pub jobs: JobsSettings,
    #[serde(default)]
    pub execution: ExecutionSettings,
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
    /// Unique node identifier for this server instance (default: "node1")
    #[serde(default = "default_node_id")]
    pub node_id: String,
}

/// Storage settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettings {
    pub rocksdb_path: String,
    /// Default path for 'local' storage when base_directory='' (T164a)
    #[serde(default = "default_storage_path")]
    pub default_storage_path: String,
    /// Template for shared table paths (placeholders: {namespace}, {tableName})
    #[serde(default = "default_shared_tables_template")]
    pub shared_tables_template: String,
    /// Template for user table paths (placeholders: {namespace}, {tableName}, {userId})
    #[serde(default = "default_user_tables_template")]
    pub user_tables_template: String,
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
    /// Optional per-target log level overrides (e.g., datafusion="info", arrow="warn")
    /// Configure via a TOML table:
    /// [logging.targets]
    /// datafusion = "info"
    /// arrow = "warn"
    /// parquet = "warn"
    #[serde(default)]
    pub targets: HashMap<String, String>,
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

    /// Stream eviction interval in seconds (default: 60 seconds = 1 minute)
    /// How often to check and evict expired events from stream tables
    #[serde(default = "default_stream_eviction_interval")]
    pub eviction_interval_seconds: u64,
}

/// User management cleanup settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserManagementSettings {
    /// Grace period (in days) before soft-deleted users are purged
    #[serde(default = "default_user_deletion_grace_period")]
    pub deletion_grace_period_days: i64,

    /// Cron expression for scheduling the cleanup job
    #[serde(default = "default_cleanup_job_schedule")]
    pub cleanup_job_schedule: String,
}

/// Job management settings (Phase 9, T163)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsSettings {
    /// Maximum number of concurrent jobs (default: 10)
    #[serde(default = "default_jobs_max_concurrent")]
    pub max_concurrent: u32,

    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_jobs_max_retries")]
    pub max_retries: u32,

    /// Initial retry backoff delay in milliseconds (default: 100ms)
    #[serde(default = "default_jobs_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
}

/// SQL execution settings (Phase 11, T026)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSettings {
    /// Handler execution timeout in seconds (default: 30)
    #[serde(default = "default_handler_timeout_seconds")]
    pub handler_timeout_seconds: u64,

    /// Maximum number of query parameters (default: 50)
    #[serde(default = "default_max_parameters")]
    pub max_parameters: usize,

    /// Maximum size of a single parameter in bytes (default: 524288 = 512KB)
    #[serde(default = "default_max_parameter_size_bytes")]
    pub max_parameter_size_bytes: usize,
}


/// Shutdown settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct ShutdownSettings {
    /// Flush job timeout settings
    pub flush: ShutdownFlushSettings,
}

/// Flush job shutdown timeout settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownFlushSettings {
    /// Timeout in seconds to wait for flush jobs to complete during graceful shutdown
    #[serde(default = "default_flush_job_shutdown_timeout")]
    pub timeout: u32,
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

/// Authentication settings (T105 - Phase 7, User Story 5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSettings {
    /// Allow remote (non-localhost) connections for system users (default: false)
    /// When false, system users with auth_type='internal' can only authenticate from localhost
    /// When true, system users can authenticate from any IP (requires password in metadata)
    #[serde(default = "default_auth_allow_remote_access")]
    pub allow_remote_access: bool,

    /// JWT secret for token validation (required for JWT auth)
    #[serde(default = "default_auth_jwt_secret")]
    pub jwt_secret: String,

    /// Trusted JWT issuers (comma-separated list of domains)
    #[serde(default = "default_auth_jwt_trusted_issuers")]
    pub jwt_trusted_issuers: String,

    /// Minimum password length (default: 8)
    #[serde(default = "default_auth_min_password_length")]
    pub min_password_length: usize,

    /// Maximum password length (default: 1024)
    #[serde(default = "default_auth_max_password_length")]
    pub max_password_length: usize,

    /// Bcrypt cost factor (default: 12, range: 4-31)
    #[serde(default = "default_auth_bcrypt_cost")]
    pub bcrypt_cost: u32,

    /// Enforce password complexity policy (uppercase, lowercase, digit, special)
    #[serde(default = "default_auth_enforce_password_complexity")]
    pub enforce_password_complexity: bool,
}

/// OAuth settings (Phase 10, User Story 8)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthSettings {
    /// Enable OAuth authentication (default: false)
    #[serde(default = "default_oauth_enabled")]
    pub enabled: bool,

    /// Auto-provision users on first OAuth login (default: false)
    #[serde(default = "default_oauth_auto_provision")]
    pub auto_provision: bool,

    /// Default role for auto-provisioned OAuth users (default: "user")
    #[serde(default = "default_oauth_default_role")]
    pub default_role: String,

    /// OAuth providers configuration
    #[serde(default)]
    pub providers: OAuthProvidersSettings,
}

/// OAuth providers configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct OAuthProvidersSettings {
    #[serde(default)]
    pub google: OAuthProviderConfig,
    #[serde(default)]
    pub github: OAuthProviderConfig,
    #[serde(default)]
    pub azure: OAuthProviderConfig,
}

/// Individual OAuth provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    /// Enable this provider (default: false)
    #[serde(default = "default_oauth_provider_enabled")]
    pub enabled: bool,

    /// OAuth provider's issuer URL
    #[serde(default)]
    pub issuer: String,

    /// JSON Web Key Set endpoint for public keys
    #[serde(default)]
    pub jwks_uri: String,

    /// OAuth application client ID (optional)
    #[serde(default)]
    pub client_id: Option<String>,

    /// OAuth application client secret (optional, for GitHub)
    #[serde(default)]
    pub client_secret: Option<String>,

    /// Azure tenant ID (optional, for Azure)
    #[serde(default)]
    pub tenant: Option<String>,
}

impl Default for OAuthSettings {
    fn default() -> Self {
        Self {
            enabled: default_oauth_enabled(),
            auto_provision: default_oauth_auto_provision(),
            default_role: default_oauth_default_role(),
            providers: OAuthProvidersSettings::default(),
        }
    }
}


impl Default for OAuthProviderConfig {
    fn default() -> Self {
        Self {
            enabled: default_oauth_provider_enabled(),
            issuer: String::new(),
            jwks_uri: String::new(),
            client_id: None,
            client_secret: None,
            tenant: None,
        }
    }
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            allow_remote_access: default_auth_allow_remote_access(),
            jwt_secret: default_auth_jwt_secret(),
            jwt_trusted_issuers: default_auth_jwt_trusted_issuers(),
            min_password_length: default_auth_min_password_length(),
            max_password_length: default_auth_max_password_length(),
            bcrypt_cost: default_auth_bcrypt_cost(),
            enforce_password_complexity: default_auth_enforce_password_complexity(),
        }
    }
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
            eviction_interval_seconds: default_stream_eviction_interval(),
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

impl Default for UserManagementSettings {
    fn default() -> Self {
        Self {
            deletion_grace_period_days: default_user_deletion_grace_period(),
            cleanup_job_schedule: default_cleanup_job_schedule(),
        }
    }
}

impl Default for JobsSettings {
    fn default() -> Self {
        Self {
            max_concurrent: default_jobs_max_concurrent(),
            max_retries: default_jobs_max_retries(),
            retry_backoff_ms: default_jobs_retry_backoff_ms(),
        }
    }
}

impl Default for ExecutionSettings {
    fn default() -> Self {
        Self {
            handler_timeout_seconds: default_handler_timeout_seconds(),
            max_parameters: default_max_parameters(),
            max_parameter_size_bytes: default_max_parameter_size_bytes(),
        }
    }
}


impl Default for ShutdownFlushSettings {
    fn default() -> Self {
        Self {
            timeout: default_flush_job_shutdown_timeout(),
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

fn default_node_id() -> String {
    "node1".to_string()
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
    "./data/storage".to_string() // Default dev path; normalized to absolute at runtime
}

fn default_shared_tables_template() -> String {
    "{namespace}/{tableName}".to_string()
}

fn default_user_tables_template() -> String {
    "{namespace}/{tableName}/{userId}".to_string()
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

fn default_stream_eviction_interval() -> u64 {
    60 // 60 seconds = 1 minute
}

fn default_user_deletion_grace_period() -> i64 {
    30 // 30 days
}

fn default_cleanup_job_schedule() -> String {
    "0 2 * * *".to_string()
}

// Jobs defaults (Phase 9, T163)
fn default_jobs_max_concurrent() -> u32 {
    10 // 10 concurrent jobs
}

fn default_jobs_max_retries() -> u32 {
    3 // 3 retry attempts
}

fn default_jobs_retry_backoff_ms() -> u64 {
    100 // 100ms initial backoff
}

// Execution defaults (Phase 11, T026)
fn default_handler_timeout_seconds() -> u64 {
    30 // 30 seconds
}

fn default_max_parameters() -> usize {
    50 // Maximum 50 query parameters
}

fn default_max_parameter_size_bytes() -> usize {
    524288 // 512KB maximum parameter size
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

// Authentication defaults (T105 - Phase 7, User Story 5)
fn default_auth_allow_remote_access() -> bool {
    false // System users localhost-only by default for security
}

fn default_auth_jwt_secret() -> String {
    "CHANGE_ME_IN_PRODUCTION".to_string() // Must be overridden in production
}

fn default_auth_jwt_trusted_issuers() -> String {
    "".to_string() // Empty list means no JWT auth by default
}

fn default_auth_min_password_length() -> usize {
    8 // Minimum 8 characters for passwords
}

fn default_auth_max_password_length() -> usize {
    1024 // Maximum 1024 characters for passwords
}

fn default_auth_bcrypt_cost() -> u32 {
    12 // Bcrypt cost factor 12 (good balance of security and performance)
}

fn default_auth_enforce_password_complexity() -> bool {
    false
}

// OAuth defaults (Phase 10, User Story 8)
fn default_oauth_enabled() -> bool {
    false // OAuth disabled by default
}

fn default_oauth_auto_provision() -> bool {
    false // Auto-provisioning disabled by default
}

fn default_oauth_default_role() -> String {
    "user".to_string() // Default role for auto-provisioned users
}

fn default_oauth_provider_enabled() -> bool {
    false // Individual providers disabled by default
}

// RocksDB defaults (memory-optimized for many column families)
fn default_rocksdb_write_buffer_size() -> usize {
    8 * 1024 * 1024 // 8MB (reduced from 64MB to handle many CFs)
}

fn default_rocksdb_max_write_buffers() -> i32 {
    2 // Reduced from 3 to save memory
}

fn default_rocksdb_block_cache_size() -> usize {
    32 * 1024 * 1024 // 32MB shared cache (reduced from 256MB)
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
    /// Supported environment variables (T030):
    /// - KALAMDB_SERVER_HOST: Override server.host
    /// - KALAMDB_SERVER_PORT: Override server.port
    /// - KALAMDB_LOG_LEVEL: Override logging.level
    /// - KALAMDB_LOG_FILE: Override logging.file_path
    /// - KALAMDB_LOG_TO_CONSOLE: Override logging.log_to_console
    /// - KALAMDB_DATA_DIR: Override storage.rocksdb_path
    /// - KALAMDB_ROCKSDB_PATH: Override storage.rocksdb_path (legacy, prefer KALAMDB_DATA_DIR)
    /// - KALAMDB_LOG_FILE_PATH: Override logging.file_path (legacy, prefer KALAMDB_LOG_FILE)
    /// - KALAMDB_HOST: Override server.host (legacy, prefer KALAMDB_SERVER_HOST)
    /// - KALAMDB_PORT: Override server.port (legacy, prefer KALAMDB_SERVER_PORT)
    ///
    /// Environment variables take precedence over config.toml values (T031)
    fn apply_env_overrides(&mut self) -> anyhow::Result<()> {
        use std::env;

        // Server host (new naming convention)
        if let Ok(host) = env::var("KALAMDB_SERVER_HOST") {
            self.server.host = host;
        } else if let Ok(host) = env::var("KALAMDB_HOST") {
            // Legacy fallback
            self.server.host = host;
        }

        // Server port (new naming convention)
        if let Ok(port_str) = env::var("KALAMDB_SERVER_PORT") {
            self.server.port = port_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid KALAMDB_SERVER_PORT value: {}", port_str))?;
        } else if let Ok(port_str) = env::var("KALAMDB_PORT") {
            // Legacy fallback
            self.server.port = port_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid KALAMDB_PORT value: {}", port_str))?;
        }

        // Log level
        if let Ok(level) = env::var("KALAMDB_LOG_LEVEL") {
            self.logging.level = level;
        }

        // Log file path (new naming convention)
        if let Ok(path) = env::var("KALAMDB_LOG_FILE") {
            self.logging.file_path = path;
        } else if let Ok(path) = env::var("KALAMDB_LOG_FILE_PATH") {
            // Legacy fallback
            self.logging.file_path = path;
        }

        // Log to console
        if let Ok(val) = env::var("KALAMDB_LOG_TO_CONSOLE") {
            self.logging.log_to_console =
                val.to_lowercase() == "true" || val == "1" || val.to_lowercase() == "yes";
        }

        // Data directory (new naming convention)
        if let Ok(path) = env::var("KALAMDB_DATA_DIR") {
            self.storage.rocksdb_path = path;
        } else if let Ok(path) = env::var("KALAMDB_ROCKSDB_PATH") {
            // Legacy fallback
            self.storage.rocksdb_path = path;
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

        // Validate per-target log levels if provided
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        for (target, level) in &self.logging.targets {
            if !valid_levels.contains(&level.as_str()) {
                return Err(anyhow::anyhow!(
                    "Invalid log level '{}' for target '{}'. Must be one of: {}",
                    level,
                    target,
                    valid_levels.join(", ")
                ));
            }
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

        if self.user_management.deletion_grace_period_days < 0 {
            return Err(anyhow::anyhow!(
                "deletion_grace_period_days cannot be negative"
            ));
        }

        if self.user_management.cleanup_job_schedule.trim().is_empty() {
            return Err(anyhow::anyhow!("cleanup_job_schedule cannot be empty"));
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
                node_id: default_node_id(),
            },
            storage: StorageSettings {
                rocksdb_path: "./data/rocksdb".to_string(),
                default_storage_path: default_storage_path(),
                shared_tables_template: default_shared_tables_template(),
                user_tables_template: default_user_tables_template(),
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
                targets: HashMap::new(),
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
            auth: AuthSettings::default(),
            oauth: OAuthSettings::default(),
            user_management: UserManagementSettings::default(),
            shutdown: ShutdownSettings::default(),
            jobs: JobsSettings::default(),
            execution: ExecutionSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

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

    #[test]
    fn test_env_override_server_host() {
        env::set_var("KALAMDB_SERVER_HOST", "0.0.0.0");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        env::remove_var("KALAMDB_SERVER_HOST");
    }

    #[test]
    fn test_env_override_server_port() {
        env::set_var("KALAMDB_SERVER_PORT", "9090");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.server.port, 9090);
        env::remove_var("KALAMDB_SERVER_PORT");
    }

    #[test]
    fn test_env_override_log_level() {
        env::set_var("KALAMDB_LOG_LEVEL", "debug");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.logging.level, "debug");
        env::remove_var("KALAMDB_LOG_LEVEL");
    }

    #[test]
    fn test_env_override_log_to_console() {
        env::set_var("KALAMDB_LOG_TO_CONSOLE", "false");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.logging.log_to_console, false);
        env::remove_var("KALAMDB_LOG_TO_CONSOLE");

        // Test truthy values
        env::set_var("KALAMDB_LOG_TO_CONSOLE", "true");
        config.apply_env_overrides().unwrap();
        assert_eq!(config.logging.log_to_console, true);
        env::remove_var("KALAMDB_LOG_TO_CONSOLE");

        env::set_var("KALAMDB_LOG_TO_CONSOLE", "1");
        config.apply_env_overrides().unwrap();
        assert_eq!(config.logging.log_to_console, true);
        env::remove_var("KALAMDB_LOG_TO_CONSOLE");
    }

    #[test]
    fn test_env_override_data_dir() {
        env::set_var("KALAMDB_DATA_DIR", "/custom/data");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.storage.rocksdb_path, "/custom/data");
        env::remove_var("KALAMDB_DATA_DIR");
    }

    #[test]
    fn test_legacy_env_vars() {
        // // Test legacy KALAMDB_HOST
        // env::set_var("KALAMDB_HOST", "192.168.1.1");
        // let mut config = ServerConfig::default();
        // config.apply_env_overrides().unwrap();
        // assert_eq!(config.server.host, "192.168.1.1");
        // env::remove_var("KALAMDB_HOST");

        // // Test legacy KALAMDB_PORT
        // env::set_var("KALAMDB_PORT", "3000");
        // config = ServerConfig::default();
        // config.apply_env_overrides().unwrap();
        // assert_eq!(config.server.port, 3000);
        // env::remove_var("KALAMDB_PORT");

        // // Test legacy KALAMDB_ROCKSDB_PATH
        // env::set_var("KALAMDB_ROCKSDB_PATH", "/old/path");
        // config = ServerConfig::default();
        // config.apply_env_overrides().unwrap();
        // assert_eq!(config.storage.rocksdb_path, "/old/path");
        // env::remove_var("KALAMDB_ROCKSDB_PATH");
    }

    #[test]
    fn test_new_env_vars_override_legacy() {
        // Set both new and legacy vars - new should win
        env::set_var("KALAMDB_SERVER_HOST", "new.host");
        env::set_var("KALAMDB_HOST", "old.host");

        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.server.host, "new.host");

        env::remove_var("KALAMDB_SERVER_HOST");
        env::remove_var("KALAMDB_HOST");

        // Same for port
        env::set_var("KALAMDB_SERVER_PORT", "8888");
        env::set_var("KALAMDB_PORT", "9999");

        config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.server.port, 8888);

        env::remove_var("KALAMDB_SERVER_PORT");
        env::remove_var("KALAMDB_PORT");
    }
}
