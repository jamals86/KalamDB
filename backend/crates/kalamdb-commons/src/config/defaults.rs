use num_cpus;

// Default value functions
pub fn default_workers() -> usize {
    0
}

pub fn default_api_version() -> String {
    "v1".to_string()
}

pub fn default_node_id() -> String {
    "node1".to_string()
}

pub fn default_flush_job_shutdown_timeout() -> u32 {
    300 // 5 minutes (T158j)
}

pub fn default_true() -> bool {
    true
}

pub fn default_compression() -> String {
    "lz4".to_string()
}

pub fn default_storage_path() -> String {
    "./data/storage".to_string() // Default dev path; normalized to absolute at runtime
}

pub fn default_shared_tables_template() -> String {
    "{namespace}/{tableName}".to_string()
}

pub fn default_user_tables_template() -> String {
    "{namespace}/{tableName}/{userId}".to_string()
}

pub fn default_max_message_size() -> usize {
    1048576 // 1MB
}

pub fn default_max_query_limit() -> usize {
    1000
}

pub fn default_query_limit() -> usize {
    50
}

pub fn default_log_level() -> String {
    "info".to_string()
}

pub fn default_log_format() -> String {
    "compact".to_string()
}

pub fn default_slow_query_threshold_ms() -> u64 {
    1200 // 1.2 seconds
}

pub fn default_logs_path() -> String {
    "./logs".to_string()
}

pub fn default_request_timeout() -> u64 {
    30
}

pub fn default_keepalive_timeout() -> u64 {
    75
}

pub fn default_max_connections() -> usize {
    25000
}

// DataFusion defaults
pub fn default_datafusion_memory_limit() -> usize {
    1024 * 1024 * 1024 // 1GB
}

pub fn default_datafusion_parallelism() -> usize {
    num_cpus::get()
}

pub fn default_datafusion_max_partitions() -> usize {
    16
}

pub fn default_datafusion_batch_size() -> usize {
    8192
}

// Flush defaults
pub fn default_flush_row_limit() -> usize {
    10000
}

pub fn default_flush_time_interval() -> u64 {
    300 // 5 minutes
}

// Manifest cache defaults (Phase 4 - US6)
pub fn default_manifest_cache_ttl() -> i64 {
    3600 // 1 hour
}

pub fn default_manifest_cache_eviction_interval() -> i64 {
    300 // 5 minutes
}

pub fn default_manifest_cache_max_entries() -> usize {
    50000
}

pub fn default_manifest_cache_memory_window() -> i64 {
    3600 // 1 hour
}

// Retention defaults
pub fn default_deleted_retention_hours() -> i32 {
    168 // 7 days
}

// Stream defaults
pub fn default_stream_ttl() -> u64 {
    10 // 10 seconds
}

pub fn default_stream_max_buffer() -> usize {
    10000
}

pub fn default_stream_eviction_interval() -> u64 {
    60 // 60 seconds = 1 minute
}

pub fn default_user_deletion_grace_period() -> i64 {
    30 // 30 days
}

pub fn default_cleanup_job_schedule() -> String {
    "0 2 * * *".to_string()
}

// Jobs defaults (Phase 9, T163)
pub fn default_jobs_max_concurrent() -> u32 {
    10 // 10 concurrent jobs
}

pub fn default_jobs_max_retries() -> u32 {
    3 // 3 retry attempts
}

pub fn default_jobs_retry_backoff_ms() -> u64 {
    100 // 100ms initial backoff
}

// Execution defaults (Phase 11, T026)
pub fn default_handler_timeout_seconds() -> u64 {
    30 // 30 seconds
}

pub fn default_max_parameters() -> usize {
    50 // Maximum 50 query parameters
}

pub fn default_max_parameter_size_bytes() -> usize {
    524288 // 512KB maximum parameter size
}

// Rate limiter defaults
pub fn default_rate_limit_queries_per_sec() -> u32 {
    100 // 100 queries per second per user
}

pub fn default_rate_limit_messages_per_sec() -> u32 {
    50 // 50 messages per second per WebSocket connection
}

pub fn default_rate_limit_max_subscriptions() -> u32 {
    10 // 10 concurrent subscriptions per user
}

// Authentication defaults (T105 - Phase 7, User Story 5)
pub fn default_auth_allow_remote_access() -> bool {
    false // System users localhost-only by default for security
}

pub fn default_auth_jwt_secret() -> String {
    "CHANGE_ME_IN_PRODUCTION".to_string() // Must be overridden in production
}

pub fn default_auth_jwt_trusted_issuers() -> String {
    "".to_string() // Empty list means no JWT auth by default
}

pub fn default_auth_min_password_length() -> usize {
    8 // Minimum 8 characters for passwords
}

pub fn default_auth_max_password_length() -> usize {
    1024 // Maximum 1024 characters for passwords
}

pub fn default_auth_bcrypt_cost() -> u32 {
    12 // Bcrypt cost factor 12 (good balance of security and performance)
}

pub fn default_auth_enforce_password_complexity() -> bool {
    false
}

// OAuth defaults (Phase 10, User Story 8)
pub fn default_oauth_enabled() -> bool {
    false // OAuth disabled by default
}

pub fn default_oauth_auto_provision() -> bool {
    false // Auto-provisioning disabled by default
}

pub fn default_oauth_default_role() -> String {
    "user".to_string() // Default role for auto-provisioned users
}

pub fn default_oauth_provider_enabled() -> bool {
    false // Individual providers disabled by default
}

// RocksDB defaults (memory-optimized for many column families)
pub fn default_rocksdb_write_buffer_size() -> usize {
    8 * 1024 * 1024 // 8MB (reduced from 64MB to handle many CFs)
}

pub fn default_rocksdb_max_write_buffers() -> i32 {
    2 // Reduced from 3 to save memory
}

pub fn default_rocksdb_block_cache_size() -> usize {
    32 * 1024 * 1024 // 32MB shared cache (reduced from 256MB)
}

pub fn default_rocksdb_max_background_jobs() -> i32 {
    4
}
