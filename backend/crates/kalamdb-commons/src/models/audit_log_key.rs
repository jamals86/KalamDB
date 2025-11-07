//! Type-safe key for audit log entries.
//!
//! Combines a millisecond timestamp with the `AuditLogId` to produce a
//! lexicographically sortable key suitable for RocksDB column families.

use crate::models::ids::AuditLogId;
use std::fmt;

/// Composite key for audit log entries.
///
/// Keys are formatted as `{timestamp:020}_{audit_id}` to maintain natural
/// ordering by time while preserving uniqueness via the audit identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AuditLogKey {
    timestamp: i64,
    audit_id: AuditLogId,
    key: String,
}

impl AuditLogKey {
    /// Creates a new audit log key from the given timestamp and identifier.
    pub fn new(timestamp: i64, audit_id: AuditLogId) -> Self {
        let key = format!("{:020}_{}", timestamp, audit_id.as_str());
        Self {
            timestamp,
            audit_id,
            key,
        }
    }

    /// Returns the millisecond timestamp component.
    pub fn timestamp(&self) -> i64 {
        self.timestamp
    }

    /// Returns the audit identifier component.
    pub fn audit_id(&self) -> &AuditLogId {
        &self.audit_id
    }

    /// Returns the formatted key as a string slice.
    pub fn as_str(&self) -> &str {
        &self.key
    }
}

impl AsRef<[u8]> for AuditLogKey {
    fn as_ref(&self) -> &[u8] {
        self.key.as_bytes()
    }
}

impl fmt::Display for AuditLogKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key)
    }
}
