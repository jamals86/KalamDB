//! Audit log store utilities.
//!
//! Provides append-only storage helpers for the `system.audit_log` table.

use crate::storage_trait::{Partition, Result, StorageBackend, StorageError};
use kalamdb_commons::models::AuditLogId;
use kalamdb_commons::system::AuditLogEntry;
use kalamdb_commons::SystemTable;
use std::sync::Arc;

/// Append-only store for audit log entries.
///
/// Wraps a generic `StorageBackend` and serializes entries as JSON.
pub struct AuditLogStore {
    backend: Arc<dyn StorageBackend>,
    partition: Partition,
}

impl AuditLogStore {
    /// Create a new audit log store backed by the given storage backend.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            backend,
            partition: Partition::new(SystemTable::AuditLog.column_family_name()),
        }
    }

    /// Append a new audit log entry.
    pub fn append(&self, entry: &AuditLogEntry) -> Result<()> {
        let key = Self::make_key(entry.timestamp, &entry.audit_id);
        let value = serde_json::to_vec(entry)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        self.backend.put(&self.partition, key.as_bytes(), &value)
    }

    /// Fetch all audit log entries (unsorted iterator order).
    pub fn scan_all(&self) -> Result<Vec<AuditLogEntry>> {
        let iter = self.backend.scan(&self.partition, None, None)?;
        let mut entries = Vec::new();
        for (_key, value) in iter {
            let entry: AuditLogEntry = serde_json::from_slice(&value)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Helper to construct lexicographically sortable key.
    fn make_key(timestamp_ms: i64, audit_id: &AuditLogId) -> String {
        // Zero-pad timestamp to keep ordering and append ID for uniqueness.
        format!("{:020}_{}", timestamp_ms, audit_id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{UserId, UserName};
    use kalamdb_commons::system::AuditLogEntry;
    use kalamdb_store::test_utils::InMemoryBackend;
    use serde_json::json;

    fn sample_entry(action: &str) -> AuditLogEntry {
        AuditLogEntry {
            audit_id: AuditLogId::new(format!("audit-{}", action)),
            timestamp: 1_730_000_000_000,
            actor_user_id: UserId::new("admin"),
            actor_username: UserName::new("admin"),
            action: action.to_string(),
            target: "system.users".to_string(),
            details: Some(json!({ "sample": true }).to_string()),
            ip_address: Some("127.0.0.1".to_string()),
        }
    }

    #[test]
    fn test_append_and_scan() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let store = AuditLogStore::new(backend);

        let entry = sample_entry("user.create");
        store.append(&entry).unwrap();

        let entries = store.scan_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "user.create");
    }
}
