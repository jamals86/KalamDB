//! Jobs table store implementation
//!
//! This module provides a SystemTableStore<JobId, Job> wrapper for the system.jobs table.

use crate::stores::SystemTableStore;
use kalamdb_commons::system::Job;
use kalamdb_commons::JobId;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Type alias for the jobs table store
pub type JobsStore = SystemTableStore<JobId, Job>;

/// Helper function to create a new jobs table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the jobs table
pub fn new_jobs_store(backend: Arc<dyn StorageBackend>) -> JobsStore {
    SystemTableStore::new(backend, "system_jobs") //TODO: user the enum partition name
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{JobStatus, JobType, NamespaceId, Role, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_store::CrossUserTableStore;
    use kalamdb_store::EntityStoreV2 as EntityStore;

    fn create_test_store() -> JobsStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_jobs_store(backend)
    }

    fn create_test_job(job_id: &str) -> Job {
        Job {
            job_id: JobId::new(job_id),
            job_type: JobType::Flush,
            namespace_id: NamespaceId::new("default"),
            table_name: Some(TableName::new("events")),
            status: JobStatus::Running,
            parameters: None,
            result: None,
            trace: None,
            memory_used: None,
            cpu_used: None,
            created_at: 1000,
            started_at: Some(1000),
            completed_at: None,
            node_id: "server-01".to_string(),
            error_message: None,
        }
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert_eq!(store.partition(), "system_jobs");
    }

    #[test]
    fn test_put_and_get_job() {
        let store = create_test_store();
        let job_id = JobId::new("job1");
        let job = create_test_job("job1");

        // Put job
        EntityStore::put(&store, &job_id, &job).unwrap();

        // Get job
        let retrieved = EntityStore::get(&store, &job_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.job_id, job_id);
        assert_eq!(retrieved.status, JobStatus::Running);
    }

    #[test]
    fn test_delete_job() {
        let store = create_test_store();
        let job_id = JobId::new("job1");
        let job = create_test_job("job1");

        // Put then delete
        EntityStore::put(&store, &job_id, &job).unwrap();
        EntityStore::delete(&store, &job_id).unwrap();

        // Verify deleted
        let retrieved = EntityStore::get(&store, &job_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_jobs() {
        let store = create_test_store();

        // Insert multiple jobs
        for i in 1..=3 {
            let job_id = JobId::new(format!("job{}", i));
            let job = create_test_job(&format!("job{}", i));
            EntityStore::put(&store, &job_id, &job).unwrap();
        }

        // Scan all
        let jobs = EntityStore::scan_all(&store).unwrap();
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_admin_only_access() {
        let store = create_test_store();

        // System tables return None for table_access (admin-only)
        assert!(store.table_access().is_none());

        // Only Service, Dba, System roles can read
        assert!(!store.can_read(&Role::User));
        assert!(store.can_read(&Role::Service));
        assert!(store.can_read(&Role::Dba));
        assert!(store.can_read(&Role::System));
    }
}
