//! Type-safe table options for different table types

use serde::{Deserialize, Serialize};

/// Table options for USER tables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserTableOptions {
    /// User ID partitioning strategy
    #[serde(default)]
    pub partition_by_user: bool,

    /// Maximum rows per user partition (0 = unlimited)
    #[serde(default)]
    pub max_rows_per_user: u64,

    /// Enable row-level security
    #[serde(default = "default_true")]
    pub enable_rls: bool,

    /// Compression algorithm (none, snappy, lz4, zstd)
    #[serde(default = "default_compression")]
    pub compression: String,
}

/// Table options for SHARED tables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedTableOptions {
    /// Access level (public, restricted)
    #[serde(default = "default_access_level")]
    pub access_level: String,

    /// Enable caching for shared data
    #[serde(default = "default_true")]
    pub enable_cache: bool,

    /// Cache TTL in seconds (0 = no expiration)
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,

    /// Compression algorithm (none, snappy, lz4, zstd)
    #[serde(default = "default_compression")]
    pub compression: String,

    /// Enable replication across nodes
    #[serde(default)]
    pub enable_replication: bool,
}

/// Table options for STREAM tables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamTableOptions {
    /// Time-to-live for stream events in seconds (required for STREAM tables)
    pub ttl_seconds: u64,

    /// Eviction strategy (time_based, size_based, hybrid)
    #[serde(default = "default_eviction_strategy")]
    pub eviction_strategy: String,

    /// Maximum stream size in bytes (0 = unlimited)
    #[serde(default)]
    pub max_stream_size_bytes: u64,

    /// Enable automatic compaction
    #[serde(default = "default_true")]
    pub enable_compaction: bool,

    /// Watermark delay in seconds for late events
    #[serde(default = "default_watermark_delay")]
    pub watermark_delay_seconds: u64,

    /// Compression algorithm (none, snappy, lz4, zstd)
    #[serde(default = "default_compression")]
    pub compression: String,
}

/// Table options for SYSTEM tables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemTableOptions {
    /// System table is read-only
    #[serde(default = "default_true")]
    pub read_only: bool,

    /// Enable system table caching
    #[serde(default = "default_true")]
    pub enable_cache: bool,

    /// Cache TTL in seconds
    #[serde(default = "default_system_cache_ttl")]
    pub cache_ttl_seconds: u64,

    /// Allow system table queries only from localhost
    #[serde(default)]
    pub localhost_only: bool,
}

/// Union type for all table options
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "table_type", rename_all = "UPPERCASE")]
pub enum TableOptions {
    User(UserTableOptions),
    Shared(SharedTableOptions),
    Stream(StreamTableOptions),
    System(SystemTableOptions),
}

impl TableOptions {
    /// Create default options for a USER table
    pub fn user() -> Self {
        TableOptions::User(UserTableOptions::default())
    }

    /// Create default options for a SHARED table
    pub fn shared() -> Self {
        TableOptions::Shared(SharedTableOptions::default())
    }

    /// Create default options for a STREAM table with required TTL
    pub fn stream(ttl_seconds: u64) -> Self {
        TableOptions::Stream(StreamTableOptions {
            ttl_seconds,
            ..Default::default()
        })
    }

    /// Create default options for a SYSTEM table
    pub fn system() -> Self {
        TableOptions::System(SystemTableOptions::default())
    }

    /// Get the compression setting (common across all types)
    pub fn compression(&self) -> &str {
        match self {
            TableOptions::User(opts) => &opts.compression,
            TableOptions::Shared(opts) => &opts.compression,
            TableOptions::Stream(opts) => &opts.compression,
            TableOptions::System(_) => "none", // System tables don't use compression
        }
    }

    /// Check if caching is enabled (where applicable)
    pub fn is_cache_enabled(&self) -> bool {
        match self {
            TableOptions::User(_) => false, // User tables don't cache
            TableOptions::Shared(opts) => opts.enable_cache,
            TableOptions::Stream(_) => false, // Stream tables don't cache
            TableOptions::System(opts) => opts.enable_cache,
        }
    }

    /// Get cache TTL in seconds (returns None if caching not supported)
    pub fn cache_ttl_seconds(&self) -> Option<u64> {
        match self {
            TableOptions::User(_) => None,
            TableOptions::Shared(opts) => Some(opts.cache_ttl_seconds),
            TableOptions::Stream(_) => None,
            TableOptions::System(opts) => Some(opts.cache_ttl_seconds),
        }
    }
}

// Default value functions for serde
fn default_true() -> bool {
    true
}

fn default_compression() -> String {
    "snappy".to_string()
}

fn default_access_level() -> String {
    "public".to_string()
}

fn default_cache_ttl() -> u64 {
    3600 // 1 hour
}

fn default_system_cache_ttl() -> u64 {
    300 // 5 minutes
}

fn default_eviction_strategy() -> String {
    "time_based".to_string()
}

fn default_watermark_delay() -> u64 {
    60 // 1 minute
}

impl Default for UserTableOptions {
    fn default() -> Self {
        Self {
            partition_by_user: false,
            max_rows_per_user: 0,
            enable_rls: true,
            compression: default_compression(),
        }
    }
}

impl Default for SharedTableOptions {
    fn default() -> Self {
        Self {
            access_level: default_access_level(),
            enable_cache: true,
            cache_ttl_seconds: default_cache_ttl(),
            compression: default_compression(),
            enable_replication: false,
        }
    }
}

impl Default for StreamTableOptions {
    fn default() -> Self {
        Self {
            ttl_seconds: 86400, // 24 hours default
            eviction_strategy: default_eviction_strategy(),
            max_stream_size_bytes: 0,
            enable_compaction: true,
            watermark_delay_seconds: default_watermark_delay(),
            compression: default_compression(),
        }
    }
}

impl Default for SystemTableOptions {
    fn default() -> Self {
        Self {
            read_only: true,
            enable_cache: true,
            cache_ttl_seconds: default_system_cache_ttl(),
            localhost_only: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_table_options_default() {
        let opts = UserTableOptions::default();
        assert!(!opts.partition_by_user);
        assert_eq!(opts.max_rows_per_user, 0);
        assert!(opts.enable_rls);
        assert_eq!(opts.compression, "snappy");
    }

    #[test]
    fn test_shared_table_options_default() {
        let opts = SharedTableOptions::default();
        assert_eq!(opts.access_level, "public");
        assert!(opts.enable_cache);
        assert_eq!(opts.cache_ttl_seconds, 3600);
        assert_eq!(opts.compression, "snappy");
        assert!(!opts.enable_replication);
    }

    #[test]
    fn test_stream_table_options_default() {
        let opts = StreamTableOptions::default();
        assert_eq!(opts.ttl_seconds, 86400);
        assert_eq!(opts.eviction_strategy, "time_based");
        assert_eq!(opts.max_stream_size_bytes, 0);
        assert!(opts.enable_compaction);
        assert_eq!(opts.watermark_delay_seconds, 60);
        assert_eq!(opts.compression, "snappy");
    }

    #[test]
    fn test_system_table_options_default() {
        let opts = SystemTableOptions::default();
        assert!(opts.read_only);
        assert!(opts.enable_cache);
        assert_eq!(opts.cache_ttl_seconds, 300);
        assert!(!opts.localhost_only);
    }

    #[test]
    fn test_table_options_constructors() {
        let user = TableOptions::user();
        assert!(matches!(user, TableOptions::User(_)));

        let shared = TableOptions::shared();
        assert!(matches!(shared, TableOptions::Shared(_)));

        let stream = TableOptions::stream(3600);
        if let TableOptions::Stream(opts) = stream {
            assert_eq!(opts.ttl_seconds, 3600);
        } else {
            panic!("Expected Stream options");
        }

        let system = TableOptions::system();
        assert!(matches!(system, TableOptions::System(_)));
    }

    #[test]
    fn test_compression_getter() {
        assert_eq!(TableOptions::user().compression(), "snappy");
        assert_eq!(TableOptions::shared().compression(), "snappy");
        assert_eq!(TableOptions::stream(3600).compression(), "snappy");
        assert_eq!(TableOptions::system().compression(), "none");
    }

    #[test]
    fn test_cache_enabled() {
        assert!(!TableOptions::user().is_cache_enabled());
        assert!(TableOptions::shared().is_cache_enabled());
        assert!(!TableOptions::stream(3600).is_cache_enabled());
        assert!(TableOptions::system().is_cache_enabled());
    }

    #[test]
    fn test_cache_ttl() {
        assert_eq!(TableOptions::user().cache_ttl_seconds(), None);
        assert_eq!(TableOptions::shared().cache_ttl_seconds(), Some(3600));
        assert_eq!(TableOptions::stream(3600).cache_ttl_seconds(), None);
        assert_eq!(TableOptions::system().cache_ttl_seconds(), Some(300));
    }

    #[test]
    fn test_serialization() {
        let opts = TableOptions::stream(7200);
        let json = serde_json::to_string(&opts).unwrap();
        let decoded: TableOptions = serde_json::from_str(&json).unwrap();
        assert_eq!(opts, decoded);
    }

    #[test]
    fn test_custom_options() {
        let custom_stream = TableOptions::Stream(StreamTableOptions {
            ttl_seconds: 1800,
            eviction_strategy: "size_based".to_string(),
            max_stream_size_bytes: 1_000_000_000,
            enable_compaction: false,
            watermark_delay_seconds: 120,
            compression: "lz4".to_string(),
        });

        assert_eq!(custom_stream.compression(), "lz4");
        if let TableOptions::Stream(opts) = custom_stream {
            assert_eq!(opts.ttl_seconds, 1800);
            assert_eq!(opts.eviction_strategy, "size_based");
            assert_eq!(opts.max_stream_size_bytes, 1_000_000_000);
        }
    }
}
