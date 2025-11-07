#![cfg(any())] // Disabled pending migration to UnifiedJobManager APIs
 
//! Integration tests for scheduled user cleanup job.
//!
//! Verifies that:
//! - Soft-deleted users past the grace period are permanently removed
//! - User table rows are purged for deleted users
//! - Job execution records are written to system.jobs

#[path = "integration/common/mod.rs"]
mod common;

use chrono::Utc;
use common::{fixtures, TestServer};
use kalamdb_commons::{JobStatus, JobType, NamespaceId, TableName};
use kalamdb_core::jobs::{JobExecutor, JobResult, UserCleanupConfig, UserCleanupJob};
use kalamdb_core::tables::system::system_table_store::UserTableStoreExt;
use kalamdb_core::tables::system::JobsTableProvider;
use kalamdb_core::tables::{new_user_table_store, UserTableStore};
use kalamdb_store::{RocksDBBackend, StorageBackend};
use std::sync::Arc;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn user_store(server: &TestServer, namespace: &str, table_name: &str) -> Arc<UserTableStore> {
    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(server.db.clone()));
    Arc::new(new_user_table_store(
        backend,
        &NamespaceId::new(namespace),
        &TableName::new(table_name),
    ))
}

fn cleanup_job(server: &TestServer, store: Arc<UserTableStore>) -> UserCleanupJob {
    UserCleanupJob::new(
        server.kalam_sql.clone(),
        store,
        UserCleanupConfig {
            grace_period_days: 30,
        },
    )
}

async fn create_user(server: &TestServer, username: &str) {
    let sql = format!(
        "CREATE USER '{}' WITH PASSWORD 'StrongPass123!' ROLE 'user'",
        username
    );
    let response = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(
        response.status, "success",
        "Failed to create user {}: {:?}",
        username, response.error
    );
}

async fn soft_delete_user(server: &TestServer, username: &str) {
    let sql = format!("DROP USER '{}'", username);
    let response = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(
        response.status, "success",
        "Failed to soft delete user {}: {:?}",
        username, response.error
    );
}

fn mark_user_deleted_at(server: &TestServer, username: &str, days_ago: i64) {
    let mut user = server
        .kalam_sql
        .get_user(username)
        .expect("get_user failed")
        .expect("user not found");
    user.deleted_at = Some(Utc::now().timestamp_millis() - days_ago * DAY_MS);
    server
        .kalam_sql
        .insert_user(&user)
        .expect("Failed to overwrite user record");
}

#[actix_web::test]
async fn test_cleanup_deletes_expired_users() {
    let server = TestServer::new().await;
    let store = user_store(&server, "cleanup_ns", "tasks");
    let job = cleanup_job(&server, store);

    create_user(&server, "cleanup_expired").await;
    soft_delete_user(&server, "cleanup_expired").await;
    mark_user_deleted_at(&server, "cleanup_expired", 31);

    let deleted = job.enforce().expect("cleanup enforce failed");
    assert_eq!(deleted, 1);

    let user_after = server
        .kalam_sql
        .get_user("cleanup_expired")
        .expect("get_user failed");
    assert!(
        user_after.is_none(),
        "User should be permanently deleted after cleanup"
    );
}

#[actix_web::test]
async fn test_cleanup_cascade_deletes_tables() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "cleanup_ns").await;
    let store = user_store(&server, "cleanup_ns", "tasks");
    let job = cleanup_job(&server, Arc::clone(&store));

    create_user(&server, "cleanup_tables").await;

    let create_table_sql = r#"CREATE USER TABLE cleanup_ns.tasks (
        id TEXT,
        content TEXT
    )"#;
    let response = server
        .execute_sql_as_user(create_table_sql, "cleanup_tables")
        .await;
    assert_eq!(
        response.status, "success",
        "Failed to create user table: {:?}",
        response.error
    );

    let insert_sql = r#"INSERT INTO cleanup_ns.tasks (id, content)
        VALUES ('task1', 'Pending review')"#;
    server
        .execute_sql_as_user(insert_sql, "cleanup_tables")
        .await;

    soft_delete_user(&server, "cleanup_tables").await;
    mark_user_deleted_at(&server, "cleanup_tables", 45);

    let deleted = job.enforce().expect("cleanup enforce failed");
    assert_eq!(deleted, 1);

    let remaining_rows = store
        .scan_user("cleanup_ns", "tasks", "cleanup_tables")
        .expect("scan_user failed");
    assert!(
        remaining_rows.is_empty(),
        "User rows should be removed from user table"
    );

    let user_after = server
        .kalam_sql
        .get_user("cleanup_tables")
        .expect("get_user failed");
    assert!(user_after.is_none(), "User should be deleted");
}

#[actix_web::test]
async fn test_cleanup_job_logging() {
    let server = TestServer::new().await;
    let store = user_store(&server, "cleanup_ns", "tasks");
    let job = cleanup_job(&server, Arc::clone(&store));

    create_user(&server, "cleanup_logs").await;
    soft_delete_user(&server, "cleanup_logs").await;
    mark_user_deleted_at(&server, "cleanup_logs", 40);

    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(server.db.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(backend));
    let job_executor = JobExecutor::new(jobs_provider, kalamdb_commons::NodeId::new("test-node".to_string()));

    let job_id = "user-cleanup-test-job".to_string();
    let result = job_executor
        .execute_job(
            job_id.clone(),
            "user_cleanup".to_string(),
            None,
            vec![],
            || {
                job.enforce()
                    .map(|count| format!("Deleted {} expired users", count))
                    .map_err(|err| err.to_string())
            },
        )
        .expect("execute_job failed");

    match result {
        JobResult::Success(message) => {
            assert!(
                message.contains("Deleted 1"),
                "Expected success message to include deleted count"
            );
        }
        JobResult::Failure(err) => panic!("Job failed unexpectedly: {}", err),
    }

    let job_entry = server
        .kalam_sql
        .get_job(&job_id)
        .expect("get_job failed")
        .expect("job not found in system.jobs");

    assert_eq!(job_entry.job_type, JobType::Cleanup);
    assert_eq!(job_entry.status, JobStatus::Completed);
    assert_eq!(
        job_entry.result,
        Some("Deleted 1 expired users".to_string())
    );
}
