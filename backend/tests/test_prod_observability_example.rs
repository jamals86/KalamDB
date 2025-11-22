//! Production Readiness: Observability Tests (WORKING EXAMPLE)
//!
//! This file demonstrates the CORRECT pattern for production readiness tests.
//! Uses TestServer HTTP API instead of direct internal APIs.
//!
//! See PRODUCTION_READINESS_STATUS.md for full rewrite instructions.

#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_api::models::{QueryResult, ResponseStatus};

/// Verify system.tables contains accurate metadata
#[actix_web::test]
async fn system_tables_metadata() {
    let server = TestServer::new().await;

    // Create namespace and table
    let resp = server.execute_sql("CREATE NAMESPACE app").await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let create_table = r#"
        CREATE USER TABLE app.messages (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            timestamp BIGINT
        )
        WITH flush_policy = 'never'
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Query system.tables
    let resp = server
        .execute_sql("SELECT namespace_id, name, table_type FROM system.tables WHERE name = 'messages'")
        .await;

    assert_eq!(resp.status, ResponseStatus::Success);
    if let Some(QueryResult {
        rows: Some(rows), ..
    }) = resp.result
    {
        assert_eq!(rows.len(), 1, "Should find exactly one table");

        let row = &rows[0];
        assert_eq!(
            row.get("name").unwrap().as_str().unwrap(),
            "messages",
            "Table name should match"
        );
        assert_eq!(
            row.get("namespace_id").unwrap().as_str().unwrap(),
            "app",
            "Namespace should match"
        );
        assert_eq!(
            row.get("table_type").unwrap().as_str().unwrap(),
            "user",
            "Table type should be 'user'"
        );
    } else {
        panic!("Expected query result with rows");
    }
}

/// Verify system.namespaces shows table counts
#[actix_web::test]
async fn system_namespaces_table_count() {
    let server = TestServer::new().await;

    // Create namespace
    let resp = server.execute_sql("CREATE NAMESPACE analytics").await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Initially, table_count should be 0
    let resp = server
        .execute_sql("SELECT name, table_count FROM system.namespaces WHERE name = 'analytics'")
        .await;

    assert_eq!(resp.status, ResponseStatus::Success);
    if let Some(QueryResult {
        rows: Some(rows), ..
    }) = resp.result
    {
        assert_eq!(rows.len(), 1);
        let table_count = rows[0].get("table_count").unwrap().as_i64().unwrap();
        assert_eq!(table_count, 0, "New namespace should have 0 tables");
    } else {
        panic!("Expected query result");
    }

    // Create a table
    let create_table = r#"
        CREATE USER TABLE analytics.events (
            event_id TEXT PRIMARY KEY,
            event_type TEXT NOT NULL
        )
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Now table_count should be 1
    let resp = server
        .execute_sql("SELECT name, table_count FROM system.namespaces WHERE name = 'analytics'")
        .await;

    assert_eq!(resp.status, ResponseStatus::Success);
    if let Some(QueryResult {
        rows: Some(rows), ..
    }) = resp.result
    {
        assert_eq!(rows.len(), 1);
        let table_count = rows[0].get("table_count").unwrap().as_i64().unwrap();
        assert_eq!(table_count, 1, "Namespace should show 1 table after creation");
    } else {
        panic!("Expected query result");
    }
}

/// Verify system.jobs tracks flush jobs
#[actix_web::test]
async fn system_jobs_tracking() {
    let server = TestServer::new().await;

    // Create namespace and user table with flush policy
    let resp = server.execute_sql("CREATE NAMESPACE app").await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let create_table = r#"
        CREATE USER TABLE app.logs (
            log_id TEXT PRIMARY KEY,
            message TEXT
        )
        WITH flush_policy = 'on_demand'
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Insert data
    let resp = server
        .execute_sql_as_user(
            "INSERT INTO app.logs (log_id, message) VALUES ('log1', 'Test message')",
            "user1",
        )
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Trigger flush
    let resp = server
        .execute_sql("FLUSH USER TABLE app.logs AS user1")
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Check system.jobs for flush job
    let resp = server
        .execute_sql("SELECT job_type, status, namespace_id, table_name FROM system.jobs WHERE job_type = 'flush'")
        .await;

    assert_eq!(resp.status, ResponseStatus::Success);
    if let Some(QueryResult {
        rows: Some(rows), ..
    }) = resp.result
    {
        assert!(
            !rows.is_empty(),
            "Should have at least one flush job recorded"
        );

        // Find our flush job
        let flush_job = rows.iter().find(|r| {
            r.get("table_name").unwrap().as_str().unwrap() == "logs"
                && r.get("namespace_id").unwrap().as_str().unwrap() == "app"
        });

        assert!(
            flush_job.is_some(),
            "Should find flush job for app.logs table"
        );

        let job = flush_job.unwrap();
        let status = job.get("status").unwrap().as_str().unwrap();
        assert!(
            status == "completed" || status == "running",
            "Flush job should be completed or running"
        );
    } else {
        panic!("Expected query result");
    }
}

/// Verify error messages are clear for invalid queries
#[actix_web::test]
async fn validation_clear_error_messages() {
    let server = TestServer::new().await;

    // Syntax error should have helpful message
    let resp = server.execute_sql("SELCT * FROM system.tables").await;
    assert_eq!(resp.status, ResponseStatus::Error);
    assert!(resp.error.is_some());
    let error = resp.error.unwrap();
    assert!(
        error.to_lowercase().contains("syntax") || error.to_lowercase().contains("parser"),
        "Syntax error should mention 'syntax' or 'parser': {}",
        error
    );

    // Table not found should be clear
    let resp = server
        .execute_sql("SELECT * FROM nonexistent.table")
        .await;
    assert_eq!(resp.status, ResponseStatus::Error);
    assert!(resp.error.is_some());
    let error = resp.error.unwrap();
    assert!(
        error.to_lowercase().contains("not found")
            || error.to_lowercase().contains("does not exist")
            || error.to_lowercase().contains("unknown"),
        "Missing table error should be clear: {}",
        error
    );
}

/// Verify concurrent SELECT queries work correctly
#[actix_web::test]
async fn concurrency_concurrent_readers() {
    let server = TestServer::new().await;

    // Setup
    let resp = server.execute_sql("CREATE NAMESPACE app").await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let create_table = r#"
        CREATE USER TABLE app.data (
            id TEXT PRIMARY KEY,
            value TEXT
        )
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Insert test data
    for i in 0..10 {
        let sql = format!(
            "INSERT INTO app.data (id, value) VALUES ('id{}', 'value{}')",
            i, i
        );
        let resp = server.execute_sql_as_user(&sql, "user1").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }

    // Spawn concurrent readers
    let mut handles = vec![];
    for _ in 0..5 {
        let server_clone = server.clone();
        let handle = tokio::spawn(async move {
            let resp = server_clone
                .execute_sql("SELECT COUNT(*) as count FROM app.data AS user1")
                .await;
            assert_eq!(resp.status, ResponseStatus::Success);
            if let Some(QueryResult {
                rows: Some(rows), ..
            }) = resp.result
            {
                assert_eq!(rows.len(), 1);
                let count = rows[0].get("count").unwrap().as_i64().unwrap();
                assert_eq!(count, 10, "Should read 10 rows");
            }
        });
        handles.push(handle);
    }

    // Wait for all readers
    for handle in handles {
        handle.await.expect("Reader task panicked");
    }
}
