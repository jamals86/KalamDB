//! Configuration model structures for KalamDB.
//!
//! This module provides shared configuration structures used across all KalamDB crates.
//!
//! ## Example Usage
//!
//! ```rust
//! use kalamdb_commons::config::{FlushPolicy, StorageLocation};
//!
//! let policy = FlushPolicy::Rows(1000);
//! let location = StorageLocation::new("s3://bucket/data");
//! ```

/// Flush policy configuration for tables.
///
/// Determines when buffered data should be flushed from RocksDB to Parquet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlushPolicy {
    /// Flush after N rows per user-table combination
    Rows(u64),

    /// Flush after time interval (in seconds)
    Interval(u64),

    /// Flush when either condition is met
    Combined { rows: u64, interval_seconds: u64 },

    /// Manual flush only (no automatic flushing)
    Manual,
}

impl FlushPolicy {
    /// Returns the row threshold, if applicable.
    pub fn row_threshold(&self) -> Option<u64> {
        match self {
            FlushPolicy::Rows(n) => Some(*n),
            FlushPolicy::Combined { rows, .. } => Some(*rows),
            _ => None,
        }
    }

    /// Returns the interval threshold in seconds, if applicable.
    pub fn interval_seconds(&self) -> Option<u64> {
        match self {
            FlushPolicy::Interval(s) => Some(*s),
            FlushPolicy::Combined {
                interval_seconds, ..
            } => Some(*interval_seconds),
            _ => None,
        }
    }

    /// Returns true if automatic flushing is enabled.
    pub fn is_automatic(&self) -> bool {
        !matches!(self, FlushPolicy::Manual)
    }
}

// /// Storage location configuration.
// ///
// /// Specifies where table data should be stored (filesystem, S3, etc.).
// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct StorageLocation {
//     /// Storage path or URL (may contain {user_id} template)
//     path: String,
// }

// impl StorageLocation {
//     /// Creates a new storage location.
//     pub fn new(path: impl Into<String>) -> Self {
//         Self { path: path.into() }
//     }

//     /// Returns the storage path.
//     pub fn path(&self) -> &str {
//         &self.path
//     }

//     /// Returns true if the path contains a {user_id} template.
//     pub fn has_user_template(&self) -> bool {
//         self.path.contains("{user_id}")
//     }

//     /// Resolves the template with a user_id.
//     pub fn resolve(&self, user_id: &str) -> String {
//         self.path.replace("{user_id}", user_id)
//     }
// }

/// Retention policy for deleted rows.
///
/// Controls how long soft-deleted rows are kept before permanent deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    /// Duration in seconds to keep deleted rows
    seconds: u64,
}

impl RetentionPolicy {
    /// Creates a new retention policy.
    pub fn new(seconds: u64) -> Self {
        Self { seconds }
    }

    /// Returns the retention duration in seconds.
    pub fn seconds(&self) -> u64 {
        self.seconds
    }

    /// Returns the retention duration in days.
    pub fn days(&self) -> u64 {
        self.seconds / 86400
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        // Default: 7 days
        Self::new(7 * 86400)
    }
}

/// Stream table configuration.
///
/// Defines behavior for ephemeral stream tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamConfig {
    /// Time-to-live in seconds (rows older than this are evicted)
    pub ttl_seconds: Option<u64>,

    /// Maximum buffer size (evict oldest when exceeded)
    pub max_buffer: Option<usize>,

    /// Whether data is ephemeral (no disk persistence)
    pub ephemeral: bool,
}

impl StreamConfig {
    /// Creates a new stream configuration with defaults.
    pub fn new() -> Self {
        Self {
            ttl_seconds: None,
            max_buffer: None,
            ephemeral: false,
        }
    }

    /// Sets the TTL in seconds.
    pub fn with_ttl(mut self, seconds: u64) -> Self {
        self.ttl_seconds = Some(seconds);
        self
    }

    /// Sets the maximum buffer size.
    pub fn with_max_buffer(mut self, size: usize) -> Self {
        self.max_buffer = Some(size);
        self
    }

    /// Sets ephemeral mode.
    pub fn with_ephemeral(mut self, ephemeral: bool) -> Self {
        self.ephemeral = ephemeral;
        self
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flush_policy() {
        let policy = FlushPolicy::Rows(1000);
        assert_eq!(policy.row_threshold(), Some(1000));
        assert_eq!(policy.interval_seconds(), None);
        assert!(policy.is_automatic());

        let policy = FlushPolicy::Combined {
            rows: 1000,
            interval_seconds: 300,
        };
        assert_eq!(policy.row_threshold(), Some(1000));
        assert_eq!(policy.interval_seconds(), Some(300));
        assert!(policy.is_automatic());

        let policy = FlushPolicy::Manual;
        assert!(!policy.is_automatic());
    }

    #[test]
    fn test_storage_location() {
        let location = StorageLocation::new("/data/users/{user_id}/table");
        assert!(location.has_user_template());
        assert_eq!(location.resolve("user_123"), "/data/users/user_123/table");

        let location = StorageLocation::new("/data/shared/table");
        assert!(!location.has_user_template());
    }

    #[test]
    fn test_retention_policy() {
        let policy = RetentionPolicy::new(7 * 86400);
        assert_eq!(policy.days(), 7);
        assert_eq!(policy.seconds(), 604800);

        let default_policy = RetentionPolicy::default();
        assert_eq!(default_policy.days(), 7);
    }

    #[test]
    fn test_stream_config() {
        let config = StreamConfig::new()
            .with_ttl(3600)
            .with_max_buffer(10000)
            .with_ephemeral(true);

        assert_eq!(config.ttl_seconds, Some(3600));
        assert_eq!(config.max_buffer, Some(10000));
        assert!(config.ephemeral);
    }
}
