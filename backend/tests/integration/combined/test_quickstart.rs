//! End-to-end integration test for KalamDB quickstart guide.
//!
//! This test suite validates the complete quickstart workflow:
//! 1. Create namespace
//! 2. Create user/shared/stream tables
//! 3. Insert and query data
//! 4. Test WebSocket subscriptions (mock)
//! 5. Query system tables
//! 6. Validate performance metrics
//!
//! Based on: specs/002-simple-kalamdb/quickstart.md

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, QueryResultTestExt, TestServer};
use kalamdb_api::models::ResponseStatus;
use std::time::Instant;

#[actix_web::test]
async fn test_01_create_namespace() {
    let server = TestServer::new().await;

    let response = server
        .execute_sql("CREATE NAMESPACE IF NOT EXISTS test_qs_01")
        .await;
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create namespace: {:?}",
        response.error
    );

    // Verify namespace exists
    assert!(
        server.namespace_exists("test_qs_01").await,
        "Namespace should exist"
    );
}

#[actix_web::test]
async fn test_02_create_user_table() {
    let server = TestServer::new().await;

    // Create namespace first
    server
        .execute_sql("CREATE NAMESPACE IF NOT EXISTS test_qs_02")
        .await;

    // Create user table idempotently via fixtures (handles IF NOT EXISTS)
    fixtures::create_messages_table(&server, "test_qs_02", Some("user123")).await;

    // Verify table exists
    assert!(
        server.table_exists("test_qs_02", "messages").await,
        "Table should exist"
    );
}

#[actix_web::test]
async fn test_03_insert_data() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_03").await;
    fixtures::create_messages_table(&server, "test_qs_03", Some("user123")).await;

    // Insert multiple messages
    let start = Instant::now();
    let responses = fixtures::insert_sample_messages(&server, "test_qs_03", "user123", 10).await;
    let duration = start.elapsed();

    // Verify all inserts succeeded
    for (i, response) in responses.iter().enumerate() {
        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Insert {} failed: {:?}",
            i,
            response.error
        );
    }

    // Performance check: 10 inserts should complete quickly
    assert!(
        duration.as_millis() < 1000,
        "10 inserts took {}ms, expected <1000ms",
        duration.as_millis()
    );
}

#[actix_web::test]
async fn test_04_query_data() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_04").await;
    fixtures::create_messages_table(&server, "test_qs_04", Some("user123")).await;
    fixtures::insert_sample_messages(&server, "test_qs_04", "user123", 5).await;

    // Query user messages
    let response = fixtures::query_user_messages(&server, "test_qs_04", "user123").await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Query failed: {:?}",
        response.error
    );

    // Verify we got results
    assert!(!response.results.is_empty(), "Should have query results");

    // Check if the first result has rows
    if let Some(rows) = &response.results[0].rows {
        assert!(!rows.is_empty(), "Should have data rows");
    }
}

#[actix_web::test]
async fn test_05_update_data() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_05").await;
    fixtures::create_messages_table(&server, "test_qs_05", Some("user123")).await;
    // Insert sample messages as the target user
    fixtures::insert_sample_messages(&server, "test_qs_05", "user123", 3).await;

    // Select one id for update
    let select_resp = server
        .execute_sql_as_user(
            "SELECT id FROM test_qs_05.messages WHERE user_id = 'user123' ORDER BY created_at LIMIT 1",
            "user123",
        )
        .await;
    assert_eq!(
        select_resp.status,
        ResponseStatus::Success,
        "Query for id failed: {:?}",
        select_resp.error
    );
    let id = select_resp
        .results
        .get(0)
        .and_then(|r| r.row_as_map(0))
        .and_then(|row| row.get("id").cloned())
        .and_then(|v| v.as_i64())
        .expect("Expected an id row");

    // Update by selected id
    let response = server
        .execute_sql_as_user(
            &format!(
                "UPDATE test_qs_05.messages SET content = 'Updated content' WHERE id = {}",
                id
            ),
            "user123",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Update failed: {:?}",
        response.error
    );

    // Verify update
    let verify_resp = server
        .execute_sql_as_user(
            &format!(
                "SELECT COUNT(*) AS cnt FROM test_qs_05.messages WHERE id = {} AND content = 'Updated content'",
                id
            ),
            "user123",
        )
        .await;
    assert_eq!(
        verify_resp.status,
        ResponseStatus::Success,
        "Verification SELECT failed: {:?}",
        verify_resp.error
    );
    let cnt = verify_resp
        .results
        .get(0)
        .and_then(|r| r.row_as_map(0))
        .and_then(|row| row.get("cnt").cloned())
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(cnt >= 1, "Updated row not found");
}

#[actix_web::test]
async fn test_06_delete_data() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_06").await;
    fixtures::create_messages_table(&server, "test_qs_06", Some("user123")).await;
    fixtures::insert_sample_messages(&server, "test_qs_06", "user123", 5).await;

    // Select one id for deletion
    let select_resp = server
        .execute_sql_as_user(
            "SELECT id FROM test_qs_06.messages WHERE user_id = 'user123' ORDER BY created_at LIMIT 1",
            "user123",
        )
        .await;
    assert_eq!(
        select_resp.status,
        ResponseStatus::Success,
        "Query for id failed: {:?}",
        select_resp.error
    );
    let id = select_resp
        .results
        .get(0)
        .and_then(|r| r.row_as_map(0))
        .and_then(|row| row.get("id").cloned())
        .and_then(|v| v.as_i64())
        .expect("Expected an id row");

    let response = server
        .execute_sql_as_user(
            &format!("DELETE FROM test_qs_06.messages WHERE id = {}", id),
            "user123",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Delete failed: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_07_create_shared_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_07").await;

    // Create shared table
    let response = fixtures::create_shared_table(&server, "test_qs_07", "config").await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create shared table: {:?}",
        response.error
    );

    // Verify table exists
    assert!(
        server.table_exists("test_qs_07", "config").await,
        "Shared table should exist"
    );
}

#[actix_web::test]
async fn test_08_insert_into_shared_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_08").await;
    fixtures::create_shared_table(&server, "test_qs_08", "config").await;

    // Insert config data
    let response = server
        .execute_sql(r#"INSERT INTO test_qs_08.config (name, value) VALUES ('max_connections', '100')"#)
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Insert into shared table failed: {:?}",
        response.error
    );

    // Query back
    let query_response = server
        .execute_sql("SELECT * FROM test_qs_08.config WHERE name = 'max_connections'")
        .await;

    assert_eq!(query_response.status, ResponseStatus::Success);
}

#[actix_web::test]
async fn test_09_create_stream_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_09").await;

    // Create stream table with TTL
    let response = fixtures::create_stream_table(&server, "test_qs_09", "events", 3600).await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create stream table: {:?}",
        response.error
    );

    // Verify table exists
    assert!(
        server.table_exists("test_qs_09", "events").await,
        "Stream table should exist"
    );
}

#[actix_web::test]
async fn test_10_insert_into_stream_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_10").await;
    fixtures::create_stream_table(&server, "test_qs_10", "events", 3600).await;

    // Insert event
    let response = server.execute_sql(
        r#"INSERT INTO test_qs_10.events (event_type, payload) VALUES ('login', '{"user_id": "user123"}')"#
    ).await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Insert into stream table failed: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_11_list_namespaces() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_11a").await;
    fixtures::create_namespace(&server, "test_qs_11b").await;

    // Query system.namespaces
    let response = server.execute_sql("SELECT * FROM system.namespaces").await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query namespaces: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_12_list_tables() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_12").await;
    fixtures::create_messages_table(&server, "test_qs_12", Some("user123")).await;
    fixtures::create_shared_table(&server, "test_qs_12", "config").await;

    // Query system.tables
    let response = server
        .execute_sql("SELECT * FROM system.tables WHERE namespace_id = 'test_qs_12'")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query tables: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_13_query_system_users() {
    let server = TestServer::new().await;

    // Query system.users (should be empty initially)
    let response = server.execute_sql("SELECT * FROM system.users").await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query users: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_14_drop_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_14").await;
    fixtures::create_messages_table(&server, "test_qs_14", Some("user123")).await;

    // Verify table exists
    assert!(server.table_exists("test_qs_14", "messages").await);

    // Drop table
    let response = fixtures::drop_table(&server, "test_qs_14", "messages").await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to drop table: {:?}",
        response.error
    );

    // Verify response includes cleanup job ID
    let result_message = response
        .results
        .get(0)
        .and_then(|r| r.message.as_ref())
        .expect("DROP TABLE should return result message");

    assert!(
        result_message.contains("Cleanup job:"),
        "DROP TABLE should create cleanup job, got: {}",
        result_message
    );

    // Verify table no longer exists (soft delete completed)
    assert!(
        !server.table_exists("test_qs_14", "messages").await,
        "Table should be dropped"
    );
}

#[actix_web::test]
async fn test_15_drop_namespace() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_15").await;
    fixtures::create_messages_table(&server, "test_qs_15", Some("user123")).await;

    // Drop namespace (CASCADE)
    let response = fixtures::drop_namespace(&server, "test_qs_15").await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to drop namespace: {:?}",
        response.error
    );

    // Verify namespace no longer exists
    assert!(
        !server.namespace_exists("test_qs_15").await,
        "Namespace should be dropped"
    );
}

#[actix_web::test]
async fn test_16_complete_workflow() {
    let server = TestServer::new().await;

    // Complete workflow: namespace → table → insert → query → cleanup

    // 1. Create namespace
    let response = fixtures::create_namespace(&server, "test_qs_16").await;
    assert_eq!(response.status, ResponseStatus::Success);

    // 2. Create user table
    let response = fixtures::create_messages_table(&server, "test_qs_16", Some("user123")).await;
    assert_eq!(response.status, ResponseStatus::Success);

    // 3. Insert data
    let responses = fixtures::insert_sample_messages(&server, "test_qs_16", "user123", 10).await;
    for response in responses {
        assert_eq!(response.status, ResponseStatus::Success);
    }

    // 4. Query data
    let response = fixtures::query_user_messages(&server, "test_qs_16", "user123").await;
    assert_eq!(response.status, ResponseStatus::Success);

    let _ = fixtures::drop_namespace(&server, "test_qs_16").await;
}

#[actix_web::test]
async fn test_17_performance_write_latency() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_17").await;
    fixtures::create_messages_table(&server, "test_qs_17", Some("user123")).await;

    // Measure single write latency
    let mut total_duration = std::time::Duration::ZERO;
    let iterations = 10;

    for i in 0..iterations {
        let start = Instant::now();
        let response =
            fixtures::insert_message(&server, "test_qs_17", "user123", &format!("Message {}", i)).await;
        let duration = start.elapsed();

        assert_eq!(response.status, ResponseStatus::Success);
        total_duration += duration;
    }

    let avg_latency = total_duration / iterations;

    // Target: <100ms average (REST API overhead included)
    // Note: Direct RocksDB writes should be <1ms, but REST API adds overhead
    assert!(
        avg_latency.as_millis() < 100,
        "Average write latency {}ms exceeds target of 100ms",
        avg_latency.as_millis()
    );

    println!("Average write latency: {}ms", avg_latency.as_millis());

    let _ = fixtures::drop_namespace(&server, "test_qs_17").await;
}

#[actix_web::test]
async fn test_18_performance_query_latency() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "test_qs_18").await;
    fixtures::create_messages_table(&server, "test_qs_18", Some("user123")).await;
    fixtures::insert_sample_messages(&server, "test_qs_18", "user123", 100).await;

    // Measure query latency
    let start = Instant::now();
    let response = fixtures::query_user_messages(&server, "test_qs_18", "user123").await;
    let duration = start.elapsed();

    assert_eq!(response.status, ResponseStatus::Success);

    // Target: <200ms for 100 rows
    assert!(
        duration.as_millis() < 200,
        "Query latency {}ms exceeds target of 200ms",
        duration.as_millis()
    );

    println!("Query latency for 100 rows: {}ms", duration.as_millis());

    let _ = fixtures::drop_namespace(&server, "test_qs_18").await;
}

#[actix_web::test]
async fn test_19_multiple_namespaces() {
    let server = TestServer::new().await;

    // Create multiple namespaces
    for ns in ["test_qs_19a", "test_qs_19b", "test_qs_19c"] {
        let response = fixtures::create_namespace(&server, ns).await;
        assert_eq!(response.status, ResponseStatus::Success);
        assert!(server.namespace_exists(ns).await);
    }

    for ns in ["test_qs_19a", "test_qs_19b", "test_qs_19c"] {
        let _ = fixtures::drop_namespace(&server, ns).await;
        assert!(!server.namespace_exists(ns).await);
    }
}

#[actix_web::test]
async fn test_20_complete_environment_setup() {
    let server = TestServer::new().await;
    let namespace = format!("qs_env_20_{}", std::process::id());

    // Use the fixtures utility to set up complete environment
    let result = fixtures::setup_complete_environment(&server, &namespace).await;

    assert!(
        result.is_ok(),
        "Failed to setup environment: {:?}",
        result.err()
    );

    // Verify all components
    assert!(server.namespace_exists(&namespace).await);
    assert!(server.table_exists(&namespace, "messages").await);
    assert!(server.table_exists(&namespace, "config").await);
    assert!(server.table_exists(&namespace, "events").await);

    // Cleanup
    let _ = fixtures::drop_namespace(&server, &namespace).await;
}
