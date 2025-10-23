//! Flush policy management
//!
//! This module manages flush policies for tables, determining when to flush
//! RocksDB buffer to Parquet files.

use serde::{Deserialize, Serialize};

/// Flush policy for table data
///
/// Determines when to flush buffered data from RocksDB to Parquet files.
///
/// # Examples
///
/// ```
/// use kalamdb_core::flush::FlushPolicy;
///
/// let row_policy = FlushPolicy::RowLimit { row_limit: 10000 };
/// let time_policy = FlushPolicy::TimeInterval { interval_seconds: 300 };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FlushPolicy {
    /// Flush after N rows inserted
    RowLimit {
        /// Number of rows before flushing (must be > 0 and < 1,000,000)
        row_limit: u32,
    },
    /// Flush every N seconds
    TimeInterval {
        /// Interval in seconds (must be > 0 and < 86400 = 24 hours)
        interval_seconds: u32,
    },
    /// Flush when EITHER row limit OR time interval is reached
    Combined {
        /// Number of rows before flushing (must be > 0 and < 1,000,000)
        row_limit: u32,
        /// Interval in seconds (must be > 0 and < 86400 = 24 hours)
        interval_seconds: u32,
    },
}

impl FlushPolicy {
    /// Validate the flush policy parameters
    pub fn validate(&self) -> Result<(), String> {
        match self {
            FlushPolicy::RowLimit { row_limit } => {
                if *row_limit == 0 {
                    return Err("Row limit must be greater than 0".to_string());
                }
                if *row_limit >= 1_000_000 {
                    return Err("Row limit must be less than 1,000,000".to_string());
                }
                Ok(())
            }
            FlushPolicy::TimeInterval { interval_seconds } => {
                if *interval_seconds == 0 {
                    return Err("Interval must be greater than 0".to_string());
                }
                if *interval_seconds >= 86400 {
                    return Err("Interval must be less than 86400 seconds (24 hours)".to_string());
                }
                Ok(())
            }
            FlushPolicy::Combined {
                row_limit,
                interval_seconds,
            } => {
                // Validate row limit
                if *row_limit == 0 {
                    return Err("Row limit must be greater than 0".to_string());
                }
                if *row_limit >= 1_000_000 {
                    return Err("Row limit must be less than 1,000,000".to_string());
                }
                // Validate interval
                if *interval_seconds == 0 {
                    return Err("Interval must be greater than 0".to_string());
                }
                if *interval_seconds >= 86400 {
                    return Err("Interval must be less than 86400 seconds (24 hours)".to_string());
                }
                Ok(())
            }
        }
    }

    /// Create a row limit flush policy with validation
    pub fn row_limit(limit: u32) -> Result<Self, String> {
        let policy = FlushPolicy::RowLimit { row_limit: limit };
        policy.validate()?;
        Ok(policy)
    }

    /// Create a time interval flush policy with validation
    pub fn time_interval(seconds: u32) -> Result<Self, String> {
        let policy = FlushPolicy::TimeInterval {
            interval_seconds: seconds,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Create a combined flush policy with validation
    pub fn combined(limit: u32, seconds: u32) -> Result<Self, String> {
        let policy = FlushPolicy::Combined {
            row_limit: limit,
            interval_seconds: seconds,
        };
        policy.validate()?;
        Ok(policy)
    }
}

impl Default for FlushPolicy {
    fn default() -> Self {
        FlushPolicy::RowLimit { row_limit: 10000 }
    }
}

impl From<kalamdb_sql::ddl::FlushPolicy> for FlushPolicy {
    fn from(policy: kalamdb_sql::ddl::FlushPolicy) -> Self {
        match policy {
            kalamdb_sql::ddl::FlushPolicy::Rows(rows) => FlushPolicy::RowLimit {
                row_limit: rows.min(1_000_000) as u32,
            },
            kalamdb_sql::ddl::FlushPolicy::Time(seconds) => FlushPolicy::TimeInterval {
                interval_seconds: seconds.min(86400) as u32,
            },
            kalamdb_sql::ddl::FlushPolicy::Combined { rows, seconds } => FlushPolicy::Combined {
                row_limit: rows.min(1_000_000) as u32,
                interval_seconds: seconds.min(86400) as u32,
            },
        }
    }
}

impl From<kalamdb_sql::ddl::UserTableFlushPolicy> for FlushPolicy {
    fn from(policy: kalamdb_sql::ddl::UserTableFlushPolicy) -> Self {
        match policy {
            kalamdb_sql::ddl::UserTableFlushPolicy::RowLimit { row_limit } => {
                FlushPolicy::RowLimit { row_limit }
            }
            kalamdb_sql::ddl::UserTableFlushPolicy::TimeInterval { interval_seconds } => {
                FlushPolicy::TimeInterval { interval_seconds }
            }
            kalamdb_sql::ddl::UserTableFlushPolicy::Combined {
                row_limit,
                interval_seconds,
            } => FlushPolicy::Combined {
                row_limit,
                interval_seconds,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_limit_validation() {
        assert!(FlushPolicy::row_limit(10000).is_ok());
        assert!(FlushPolicy::row_limit(1).is_ok());
        assert!(FlushPolicy::row_limit(999_999).is_ok());

        assert!(FlushPolicy::row_limit(0).is_err());
        assert!(FlushPolicy::row_limit(1_000_000).is_err());
    }

    #[test]
    fn test_time_interval_validation() {
        assert!(FlushPolicy::time_interval(300).is_ok());
        assert!(FlushPolicy::time_interval(1).is_ok());
        assert!(FlushPolicy::time_interval(86399).is_ok());

        assert!(FlushPolicy::time_interval(0).is_err());
        assert!(FlushPolicy::time_interval(86400).is_err());
    }

    #[test]
    fn test_default_policy() {
        let policy = FlushPolicy::default();
        assert_eq!(policy, FlushPolicy::RowLimit { row_limit: 10000 });
    }

    #[test]
    fn test_serialization() {
        let policy = FlushPolicy::RowLimit { row_limit: 5000 };
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("row_limit"));
        assert!(json.contains("5000"));

        let deserialized: FlushPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, deserialized);
    }
}
