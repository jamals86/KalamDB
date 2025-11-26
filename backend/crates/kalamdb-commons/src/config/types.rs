use super::defaults::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub manifest_cache: ManifestCacheSettings,
    #[serde(default)]
    pub retention: RetentionSettings,
    #[serde(default)]
    pub stream: StreamSettings,
    #[serde(default)]
    pub websocket: WebSocketSettings,
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

impl Default for LimitsSettings {
    fn default() -> Self {
        Self {
            max_message_size: default_max_message_size(),
            max_query_limit: default_max_query_limit(),
            default_query_limit: default_query_limit(),
        }
    }
}

/// Logging settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSettings {
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Directory for all log files (default: "./logs")
    /// Used for app.log, slow.log, and other log files
    #[serde(default = "default_logs_path")]
    pub logs_path: String,
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
    /// Slow query logging threshold in milliseconds (default: 1000ms = 1 second)
    /// Queries taking longer than this threshold will be logged to slow.log
    #[serde(default = "default_slow_query_threshold_ms")]
    pub slow_query_threshold_ms: u64,
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
    /// Backlog size for pending connections (default: 2048)
    /// Increase for high-traffic servers
    #[serde(default = "default_backlog")]
    pub backlog: u32,
    /// Max blocking threads per worker for CPU-intensive operations (default: 512 / workers)
    /// Used for RocksDB and other synchronous operations
    #[serde(default = "default_worker_max_blocking_threads")]
    pub worker_max_blocking_threads: usize,
    /// Client request timeout in seconds (default: 5)
    /// Time allowed for client to send complete request headers
    #[serde(default = "default_client_request_timeout")]
    pub client_request_timeout: u64,
    /// Client disconnect timeout in seconds (default: 2)
    /// Time allowed for graceful connection shutdown
    #[serde(default = "default_client_disconnect_timeout")]
    pub client_disconnect_timeout: u64,
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

/// Manifest cache settings (Phase 4 - US6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCacheSettings {
    /// Time-to-live for cached manifests in seconds (default: 3600s = 1 hour)
    #[serde(default = "default_manifest_cache_ttl")]
    pub ttl_seconds: i64,

    /// Eviction job interval in seconds (default: 300s = 5 minutes)
    #[serde(default = "default_manifest_cache_eviction_interval")]
    pub eviction_interval_seconds: i64,

    /// Maximum number of cached manifest entries (default: 50000)
    #[serde(default = "default_manifest_cache_max_entries")]
    pub max_entries: usize,

    /// Memory window for last_accessed tracking in seconds (default: 3600s = 1 hour)
    #[serde(default = "default_manifest_cache_memory_window")]
    pub last_accessed_memory_window: i64,
}

impl Default for ManifestCacheSettings {
    fn default() -> Self {
        Self {
            ttl_seconds: default_manifest_cache_ttl(),
            eviction_interval_seconds: default_manifest_cache_eviction_interval(),
            max_entries: default_manifest_cache_max_entries(),
            last_accessed_memory_window: default_manifest_cache_memory_window(),
        }
    }
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

/// WebSocket settings for connection management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketSettings {
    /// Client heartbeat timeout in seconds (default: 10)
    /// How long to wait for client pong before disconnecting
    #[serde(default = "default_websocket_client_timeout")]
    pub client_timeout_secs: Option<u64>,

    /// Authentication timeout in seconds (default: 3)
    /// How long to wait for auth message before disconnecting
    #[serde(default = "default_websocket_auth_timeout")]
    pub auth_timeout_secs: Option<u64>,

    /// Heartbeat check interval in seconds (default: 5)
    /// How often the shared heartbeat manager checks all connections
    #[serde(default = "default_websocket_heartbeat_interval")]
    pub heartbeat_interval_secs: Option<u64>,
}

impl Default for WebSocketSettings {
    fn default() -> Self {
        Self {
            client_timeout_secs: default_websocket_client_timeout(),
            auth_timeout_secs: default_websocket_auth_timeout(),
            heartbeat_interval_secs: default_websocket_heartbeat_interval(),
        }
    }
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

    /// Maximum concurrent connections per IP address (default: 100)
    /// Prevents a single IP from exhausting all server connections
    #[serde(default = "default_max_connections_per_ip")]
    pub max_connections_per_ip: u32,

    /// Maximum requests per second per IP before authentication (default: 200)
    /// Applied before auth to protect against unauthenticated floods
    #[serde(default = "default_max_requests_per_ip_per_sec")]
    pub max_requests_per_ip_per_sec: u32,

    /// Maximum request body size in bytes (default: 10MB)
    /// Prevents memory exhaustion from huge request payloads
    #[serde(default = "default_request_body_limit_bytes")]
    pub request_body_limit_bytes: usize,

    /// Duration in seconds to ban abusive IPs (default: 300 = 5 minutes)
    #[serde(default = "default_ban_duration_seconds")]
    pub ban_duration_seconds: u64,

    /// Enable connection protection middleware (default: true)
    #[serde(default = "default_enable_connection_protection")]
    pub enable_connection_protection: bool,
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
            max_connections_per_ip: default_max_connections_per_ip(),
            max_requests_per_ip_per_sec: default_max_requests_per_ip_per_sec(),
            request_body_limit_bytes: default_request_body_limit_bytes(),
            ban_duration_seconds: default_ban_duration_seconds(),
            enable_connection_protection: default_enable_connection_protection(),
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

/// Get default configuration (useful for testing)
///
/// Note: Prefer `Default::default()` constructor pattern.
impl Default for ServerConfig {
    fn default() -> Self {
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
                logs_path: default_logs_path(),
                log_to_console: true,
                format: "compact".to_string(),
                targets: HashMap::new(),
                slow_query_threshold_ms: default_slow_query_threshold_ms(),
            },
            performance: PerformanceSettings {
                request_timeout: 30,
                keepalive_timeout: 75,
                max_connections: 25000,
                backlog: default_backlog(),
                worker_max_blocking_threads: default_worker_max_blocking_threads(),
                client_request_timeout: default_client_request_timeout(),
                client_disconnect_timeout: default_client_disconnect_timeout(),
            },
            datafusion: DataFusionSettings::default(),
            flush: FlushSettings::default(),
            manifest_cache: ManifestCacheSettings::default(),
            retention: RetentionSettings::default(),
            stream: StreamSettings::default(),
            websocket: WebSocketSettings::default(),
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
