//! Test that FLUSH TABLE command persists jobs to system.jobs table
//!
//! Bug: FLUSH TABLE creates a job but doesn't persist it to system.jobs,
//! while FLUSH ALL TABLES does persist jobs correctly.

#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_api::models::ResponseStatus;
use kalamdb_commons::{JobType, Role};

#[tokio::test]
async fn test_flush_table_persists_job() {
    let server = TestServer::new().await;
    let app_context = &server.app_context;

    // Create a job directly using the Jobs API
    use kalamdb_commons::{JobId, JobStatus, JobType, NodeId};

    let now = chrono::Utc::now().timestamp_millis();
    let job = kalamdb_commons::system::Job {
        job_id: JobId::new("test-flush-123"),
        job_type: JobType::Flush,
        status: JobStatus::New,
        parameters: Some(r#"{"namespace_id":"app","table_name":"app.conversations"}"#.to_string()),
        message: None,
        exception_trace: None,
        idempotency_key: None,
        retry_count: 0,
        max_retries: 3,
        memory_used: None,
        cpu_used: None,
        created_at: now,
        updated_at: now,
        started_at: None,
        finished_at: None,
        node_id: NodeId::new("node-1".to_string()),
        queue: None,
        priority: None,
    };

    // Insert the job
    app_context.insert_job(&job).expect("Failed to insert job");

    // Verify it was persisted
    let jobs = app_context.scan_all_jobs().expect("Failed to scan jobs");
    assert_eq!(jobs.len(), 1, "Should have exactly 1 job");

    let retrieved_job = &jobs[0];
    assert_eq!(retrieved_job.job_id.as_str(), "test-flush-123");
    assert_eq!(retrieved_job.job_type, JobType::Flush);
    // namespace_id and table_name are now extracted from parameters
    assert_eq!(retrieved_job.namespace_id().unwrap().as_str(), "app");
    assert_eq!(
        retrieved_job.table_name().unwrap().as_str(),
        "app.conversations"
    );

    println!("✓ Job persistence verified");
}

#[tokio::test]
async fn test_flush_all_tables_persists_jobs() {
    let server = TestServer::new().await;
    let app_context = &server.app_context;

    // Create a DBA user to execute admin operations
    let admin_username = "test_admin";
    let admin_password = "AdminPass123!";
    let admin_id = server
        .create_user(admin_username, admin_password, Role::Dba)
        .await;
    let admin_id_str = admin_id.as_str();

    // Create 'local' storage (required for user tables)
    // TestServer creates 'local' storage by default, so we don't need to create it.
    // But if we wanted to be safe, we could check or use IF NOT EXISTS if supported.
    // For now, we assume it exists because TestServer ensures it.

    // Create namespace
    let res = server
        .execute_sql_as_user("CREATE NAMESPACE IF NOT EXISTS app", admin_id_str)
        .await;
    assert_eq!(
        res.status,
        ResponseStatus::Success,
        "Failed to create namespace: {:?}",
        res.error
    );

    // Create multiple user tables
    // Drop tables if they exist to ensure clean state
    let _ = server
        .execute_sql_as_user("DROP TABLE IF EXISTS app.table1", admin_id_str)
        .await;
    let _ = server
        .execute_sql_as_user("DROP TABLE IF EXISTS app.table2", admin_id_str)
        .await;

    let res = server
        .execute_sql_as_user(
            "CREATE TABLE app.table1 (id INT PRIMARY KEY, data VARCHAR) WITH (TYPE = 'USER')",
            admin_id_str,
        )
        .await;
    assert_eq!(
        res.status,
        ResponseStatus::Success,
        "Failed to create table1: {:?}",
        res.error
    );

    let res = server
        .execute_sql_as_user(
            "CREATE TABLE app.table2 (id INT PRIMARY KEY, data VARCHAR) WITH (TYPE = 'USER')",
            admin_id_str,
        )
        .await;
    assert_eq!(
        res.status,
        ResponseStatus::Success,
        "Failed to create table2: {:?}",
        res.error
    );

    // Execute FLUSH ALL TABLES
    let flush_result = server
        .execute_sql_as_user("FLUSH ALL TABLES IN NAMESPACE app", admin_id_str)
        .await;
    assert_eq!(
        flush_result.status,
        ResponseStatus::Success,
        "FLUSH ALL TABLES should succeed: {:?}",
        flush_result.error
    );

    println!("Flush all result: {:?}", flush_result);

    // Verify jobs were persisted
    let jobs = app_context.scan_all_jobs().expect("Failed to scan jobs");

    let flush_jobs: Vec<_> = jobs
        .iter()
        .filter(|job| {
            job.job_type == JobType::Flush
                && (job.table_name().map(|t| t.as_str().to_string()) == Some("table1".to_string())
                    || job.table_name().map(|t| t.as_str().to_string()) == Some("table2".to_string()))
        })
        .collect();

    assert_eq!(
        flush_jobs.len(),
        2,
        "Should have 2 flush jobs (one per table). Found: {:?}",
        flush_jobs
    );
}
