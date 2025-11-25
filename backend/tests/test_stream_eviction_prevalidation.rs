//! Integration test for stream eviction pre-validation
//!
//! Verifies that stream eviction jobs are skipped at creation time
//! when there are no expired rows to evict.

use kalamdb::test_helpers::{setup_test_server, TestServer};
use kalamdb_commons::models::schemas::{TableOptions, TableType};
use kalamdb_commons::models::TableName;
use kalamdb_commons::NamespaceId;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_stream_eviction_skips_job_when_no_expired_rows() {
    let server = setup_test_server().await;
    let namespace = NamespaceId::new("test_prevalidate");

    // Create a stream table with 30-second TTL
    let table_name = TableName::new("events");
    let ddl = format!(
        "CREATE STREAM TABLE {}.{} (event_id TEXT, payload TEXT) WITH (ttl_seconds = 30)",
        namespace, table_name
    );
    server.execute_sql(&ddl, None).await.unwrap();

    // Insert a fresh row (just created, not expired)
    let insert = format!(
        "INSERT INTO {}.{} (event_id, payload) VALUES ('evt1', 'fresh data')",
        namespace, table_name
    );
    server.execute_sql(&insert, None).await.unwrap();

    // Manually trigger stream eviction scheduler
    let app_ctx = kalamdb::app_context::AppContext::get();
    let jobs_manager = app_ctx.jobs_manager();
    
    // This should attempt to create a stream eviction job but skip it
    // because pre-validation will return false (no expired rows)
    let result = kalamdb_core::jobs::StreamEvictionScheduler::check_and_schedule(
        &app_ctx,
        jobs_manager,
    )
    .await;

    // Scheduler should succeed (no errors)
    assert!(result.is_ok(), "Scheduler should complete successfully");

    // Query system.jobs to verify no eviction job was created for this table
    let jobs_query = format!(
        "SELECT * FROM system.jobs WHERE job_type = 'StreamEviction' AND namespace_id = '{}'",
        namespace
    );
    let jobs_result = server.execute_sql(&jobs_query, None).await.unwrap();
    
    // No jobs should be created because pre-validation returned false
    assert_eq!(
        jobs_result.len(),
        0,
        "No eviction job should be created when there are no expired rows"
    );
}

#[tokio::test]
async fn test_stream_eviction_creates_job_when_expired_rows_exist() {
    let server = setup_test_server().await;
    let namespace = NamespaceId::new("test_prevalidate_expired");

    // Create a stream table with 1-second TTL
    let table_name = TableName::new("events");
    let ddl = format!(
        "CREATE STREAM TABLE {}.{} (event_id TEXT, payload TEXT) WITH (ttl_seconds = 1)",
        namespace, table_name
    );
    server.execute_sql(&ddl, None).await.unwrap();

    // Insert a row
    let insert = format!(
        "INSERT INTO {}.{} (event_id, payload) VALUES ('evt1', 'old data')",
        namespace, table_name
    );
    server.execute_sql(&insert, None).await.unwrap();

    // Wait for row to expire (TTL is 1 second)
    sleep(Duration::from_millis(1200)).await;

    // Manually trigger stream eviction scheduler
    let app_ctx = kalamdb::app_context::AppContext::get();
    let jobs_manager = app_ctx.jobs_manager();
    
    // This should create a stream eviction job because there are expired rows
    let result = kalamdb_core::jobs::StreamEvictionScheduler::check_and_schedule(
        &app_ctx,
        jobs_manager,
    )
    .await;

    assert!(result.is_ok(), "Scheduler should complete successfully");

    // Query system.jobs to verify eviction job was created
    let jobs_query = format!(
        "SELECT * FROM system.jobs WHERE job_type = 'StreamEviction' AND namespace_id = '{}'",
        namespace
    );
    let jobs_result = server.execute_sql(&jobs_query, None).await.unwrap();
    
    // A job should be created because pre-validation found expired rows
    assert!(
        jobs_result.len() > 0,
        "Eviction job should be created when expired rows exist"
    );
}
