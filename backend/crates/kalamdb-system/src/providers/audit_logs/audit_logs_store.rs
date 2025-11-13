//! Audit logs table store implementation
//!
//! This module provides a SystemTableStore<AuditLogId, AuditLogEntry> wrapper for the system.audit_log table.

use kalamdb_commons::models::AuditLogId;
use kalamdb_commons::system::AuditLogEntry;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

use crate::system_table_store::SystemTableStore;

/// Type alias for the audit logs table store
pub type AuditLogsStore = SystemTableStore<AuditLogId, AuditLogEntry>;

/// Helper function to create a new audit logs table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the audit_log table
pub fn new_audit_logs_store(backend: Arc<dyn StorageBackend>) -> AuditLogsStore {
    SystemTableStore::new(backend, "system_audit_log")
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{UserId, UserName};
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_store::CrossUserTableStore;
    use kalamdb_store::EntityStore as EntityStore;
    use serde_json::json;

    fn create_test_store() -> AuditLogsStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_audit_logs_store(backend)
    }

    fn create_test_audit_entry(audit_id: &str, action: &str, timestamp_ms: i64) -> AuditLogEntry {
        AuditLogEntry {
            audit_id: AuditLogId::new(audit_id),
            timestamp: timestamp_ms,
            actor_user_id: UserId::new("admin"),
            actor_username: UserName::new("admin"),
            action: action.to_string(),
            target: "system.users".to_string(),
            details: Some(json!({"test": true}).to_string()),
            ip_address: Some("127.0.0.1".to_string()),
        }
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert_eq!(store.partition(), "system_audit_log");
    }

    #[test]
    fn test_put_and_get_audit_entry() {
        let store = create_test_store();
        let audit_id = AuditLogId::new("audit_001");
        let entry = create_test_audit_entry("audit_001", "user.create", 1730000000000);

        // Put entry
        EntityStore::put(&store, &audit_id, &entry).unwrap();

        // Get entry
        let retrieved = EntityStore::get(&store, &audit_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.audit_id, audit_id);
        assert_eq!(retrieved.action, "user.create");
        assert_eq!(retrieved.timestamp, 1730000000000);
    }

    #[test]
    fn test_append_audit_entry() {
        let store = create_test_store();

        // Append multiple entries with different timestamps
        for i in 1..=3 {
            let audit_id = AuditLogId::new(&format!("audit_{:03}", i));
            let entry = create_test_audit_entry(
                &format!("audit_{:03}", i),
                "user.update",
                1730000000000 + (i as i64 * 1000),
            );
            EntityStore::put(&store, &audit_id, &entry).unwrap();
        }

        // Verify all entries exist
        let entries = EntityStore::scan_all(&store).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_scan_all_audit_entries() {
        let store = create_test_store();

        // Insert multiple entries
        for i in 1..=5 {
            let audit_id = AuditLogId::new(&format!("audit_{:03}", i));
            let entry = create_test_audit_entry(
                &format!("audit_{:03}", i),
                "user.delete",
                1730000000000 + (i as i64 * 1000),
            );
            EntityStore::put(&store, &audit_id, &entry).unwrap();
        }

        // Scan all
        let entries = EntityStore::scan_all(&store).unwrap();
        assert_eq!(entries.len(), 5);

        // Verify actions
        for entry in entries {
            assert_eq!(entry.1.action, "user.delete");
            assert_eq!(entry.1.actor_user_id.as_str(), "admin");
        }
    }

    #[test]
    fn test_timestamp_ordering() {
        let store = create_test_store();

        // Insert entries with explicit timestamps (out of order)
        let timestamps = vec![1730000003000, 1730000001000, 1730000002000];
        for (i, &ts) in timestamps.iter().enumerate() {
            let audit_id = AuditLogId::new(&format!("audit_{:03}", i + 1));
            let entry = create_test_audit_entry(&format!("audit_{:03}", i + 1), "test.action", ts);
            EntityStore::put(&store, &audit_id, &entry).unwrap();
        }

        // Scan all entries
        let mut entries = EntityStore::scan_all(&store).unwrap();
        assert_eq!(entries.len(), 3);

        // Sort by timestamp to verify ordering capability
        entries.sort_by_key(|e| e.1.timestamp);
        assert_eq!(entries[0].1.timestamp, 1730000001000);
        assert_eq!(entries[1].1.timestamp, 1730000002000);
        assert_eq!(entries[2].1.timestamp, 1730000003000);
    }

    #[test]
    fn test_concurrent_writes() {
        use std::thread;

        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let store = Arc::new(new_audit_logs_store(backend));

        // Spawn 10 threads, each writing 10 entries
        let handles: Vec<_> = (0..10)
            .map(|thread_id| {
                let store_clone = Arc::clone(&store);
                thread::spawn(move || {
                    for i in 0..10 {
                        let audit_id = AuditLogId::new(&format!("audit_t{}_i{}", thread_id, i));
                        let entry = create_test_audit_entry(
                            &format!("audit_t{}_i{}", thread_id, i),
                            "concurrent.write",
                            1730000000000 + (thread_id as i64 * 1000) + i,
                        );
                        EntityStore::put(&*store_clone, &audit_id, &entry).unwrap();
                    }
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all 100 entries were written
        let entries = EntityStore::scan_all(&*store).unwrap();
        assert_eq!(entries.len(), 100);
    }

    #[test]
    fn test_admin_only_access() {
        use kalamdb_commons::Role;

        let store = create_test_store();

        // System tables return None for table_access (admin-only)
        assert!(store.table_access().is_none());

        // Only Service, Dba, System roles can read audit logs
        assert!(!store.can_read(&Role::User));
        assert!(store.can_read(&Role::Service));
        assert!(store.can_read(&Role::Dba));
        assert!(store.can_read(&Role::System));
    }
}
