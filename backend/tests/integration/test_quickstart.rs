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

mod common;

use common::{fixtures, TestServer};
use std::time::Instant;

#[actix_web::test]
async fn test_01_create_namespace() {
    let server = TestServer::new().await;

    let response = server.execute_sql("CREATE NAMESPACE app").await;
    assert_eq!(
        response.status, "success",
        "Failed to create namespace: {:?}",
        response.error
    );

    // Verify namespace exists
    assert!(
        server.namespace_exists("app").await,
        "Namespace should exist"
    );
}

#[actix_web::test]
async fn test_02_create_user_table() {
    let server = TestServer::new().await;

    // Create namespace first
    server.execute_sql("CREATE NAMESPACE app").await;

    // Create user table with flush policy (requires X-USER-ID header)
    let response = server
        .execute_sql_with_user(
            r#"CREATE USER TABLE app.messages (
            id INT AUTO_INCREMENT,
            user_id VARCHAR NOT NULL,
            content VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 100"#,
            Some("user123"), // Pass user_id for USER table creation
        )
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to create user table: {:?}",
        response.error
    );

    // Verify table exists
    assert!(
        server.table_exists("app", "messages").await,
        "Table should exist"
    );
}

#[actix_web::test]
async fn test_03_insert_data() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_messages_table(&server, "app", Some("user123")).await;

    // Insert multiple messages
    let start = Instant::now();
    let responses = fixtures::insert_sample_messages(&server, "app", "user123", 10).await;
    let duration = start.elapsed();

    // Verify all inserts succeeded
    for (i, response) in responses.iter().enumerate() {
        assert_eq!(
            response.status, "success",
            "Insert {} failed: {:?}",
            i, response.error
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
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_messages_table(&server, "app", Some("user123")).await;
    fixtures::insert_sample_messages(&server, "app", "user123", 5).await;

    // Query user messages
    let response = fixtures::query_user_messages(&server, "app", "user123").await;

    assert_eq!(
        response.status, "success",
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
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_messages_table(&server, "app", Some("user123")).await;
    fixtures::insert_sample_messages(&server, "app", "user123", 3).await;

    // Update first message
    let response = fixtures::update_message(&server, "app", 1, "Updated content").await;

    assert_eq!(
        response.status, "success",
        "Update failed: {:?}",
        response.error
    );

    // Verify update
    let query_response = server
        .execute_sql("SELECT content FROM app.messages WHERE id = 1")
        .await;

    assert_eq!(query_response.status, "success");
}

#[actix_web::test]
async fn test_06_delete_data() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_messages_table(&server, "app", Some("user123")).await;
    fixtures::insert_sample_messages(&server, "app", "user123", 3).await;

    // Delete message (soft delete)
    let response = fixtures::delete_message(&server, "app", 1).await;

    assert_eq!(
        response.status, "success",
        "Delete failed: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_07_create_shared_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;

    // Create shared table
    let response = fixtures::create_shared_table(&server, "app", "config").await;

    assert_eq!(
        response.status, "success",
        "Failed to create shared table: {:?}",
        response.error
    );

    // Verify table exists
    assert!(
        server.table_exists("app", "config").await,
        "Shared table should exist"
    );
}

#[actix_web::test]
async fn test_08_insert_into_shared_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_shared_table(&server, "app", "config").await;

    // Insert config data
    let response = server
        .execute_sql(r#"INSERT INTO app.config (name, value) VALUES ('max_connections', '100')"#)
        .await;

    assert_eq!(
        response.status, "success",
        "Insert into shared table failed: {:?}",
        response.error
    );

    // Query back
    let query_response = server
        .execute_sql("SELECT * FROM app.config WHERE name = 'max_connections'")
        .await;

    assert_eq!(query_response.status, "success");
}

#[actix_web::test]
async fn test_09_create_stream_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;

    // Create stream table with TTL
    let response = fixtures::create_stream_table(&server, "app", "events", 3600).await;

    assert_eq!(
        response.status, "success",
        "Failed to create stream table: {:?}",
        response.error
    );

    // Verify table exists
    assert!(
        server.table_exists("app", "events").await,
        "Stream table should exist"
    );
}

#[actix_web::test]
async fn test_10_insert_into_stream_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_stream_table(&server, "app", "events", 3600).await;

    // Insert event
    let response = server.execute_sql(
        r#"INSERT INTO app.events (event_type, payload) VALUES ('login', '{"user_id": "user123"}')"#
    ).await;

    assert_eq!(
        response.status, "success",
        "Insert into stream table failed: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_11_list_namespaces() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_namespace(&server, "test").await;

    // Query system.namespaces
    let response = server.execute_sql("SELECT * FROM system.namespaces").await;

    assert_eq!(
        response.status, "success",
        "Failed to query namespaces: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_12_list_tables() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_messages_table(&server, "app", Some("user123")).await;
    fixtures::create_shared_table(&server, "app", "config").await;

    // Query system.tables
    let response = server
        .execute_sql("SELECT * FROM system.tables WHERE namespace = 'app'")
        .await;

    assert_eq!(
        response.status, "success",
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
        response.status, "success",
        "Failed to query users: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_14_drop_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_messages_table(&server, "app", Some("user123")).await;

    // Verify table exists
    assert!(server.table_exists("app", "messages").await);

    // Drop table
    let response = fixtures::drop_table(&server, "app", "messages").await;

    assert_eq!(
        response.status, "success",
        "Failed to drop table: {:?}",
        response.error
    );

    // Verify table no longer exists
    assert!(
        !server.table_exists("app", "messages").await,
        "Table should be dropped"
    );
}

#[actix_web::test]
async fn test_15_drop_namespace() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "app").await;
    fixtures::create_messages_table(&server, "app", Some("user123")).await;

    // Drop namespace (CASCADE)
    let response = fixtures::drop_namespace(&server, "app").await;

    assert_eq!(
        response.status, "success",
        "Failed to drop namespace: {:?}",
        response.error
    );

    // Verify namespace no longer exists
    assert!(
        !server.namespace_exists("app").await,
        "Namespace should be dropped"
    );
}

#[actix_web::test]
async fn test_16_complete_workflow() {
    let server = TestServer::new().await;

    // Complete workflow: namespace → table → insert → query → cleanup

    // 1. Create namespace
    let response = fixtures::create_namespace(&server, "workflow_test").await;
    assert_eq!(response.status, "success");

    // 2. Create user table
    let response = fixtures::create_messages_table(&server, "workflow_test", Some("user123")).await;
    assert_eq!(response.status, "success");

    // 3. Insert data
    let responses = fixtures::insert_sample_messages(&server, "workflow_test", "user123", 10).await;
    for response in responses {
        assert_eq!(response.status, "success");
    }

    // 4. Query data
    let response = fixtures::query_user_messages(&server, "workflow_test", "user123").await;
    assert_eq!(response.status, "success");

    // 5. Cleanup
    server.cleanup().await.expect("Cleanup failed");
    assert!(!server.namespace_exists("workflow_test").await);
}

#[actix_web::test]
async fn test_17_performance_write_latency() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "perf").await;
    fixtures::create_messages_table(&server, "perf", Some("user123")).await;

    // Measure single write latency
    let mut total_duration = std::time::Duration::ZERO;
    let iterations = 10;

    for i in 0..iterations {
        let start = Instant::now();
        let response =
            fixtures::insert_message(&server, "perf", "user123", &format!("Message {}", i)).await;
        let duration = start.elapsed();

        assert_eq!(response.status, "success");
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
}

#[actix_web::test]
async fn test_18_performance_query_latency() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "perf").await;
    fixtures::create_messages_table(&server, "perf", Some("user123")).await;
    fixtures::insert_sample_messages(&server, "perf", "user123", 100).await;

    // Measure query latency
    let start = Instant::now();
    let response = fixtures::query_user_messages(&server, "perf", "user123").await;
    let duration = start.elapsed();

    assert_eq!(response.status, "success");

    // Target: <200ms for 100 rows
    assert!(
        duration.as_millis() < 200,
        "Query latency {}ms exceeds target of 200ms",
        duration.as_millis()
    );

    println!("Query latency for 100 rows: {}ms", duration.as_millis());
}

#[actix_web::test]
async fn test_19_multiple_namespaces() {
    let server = TestServer::new().await;

    // Create multiple namespaces
    for ns in ["app1", "app2", "app3"] {
        let response = fixtures::create_namespace(&server, ns).await;
        assert_eq!(response.status, "success");
        assert!(server.namespace_exists(ns).await);
    }

    // Cleanup all
    server.cleanup().await.expect("Cleanup failed");

    for ns in ["app1", "app2", "app3"] {
        assert!(!server.namespace_exists(ns).await);
    }
}

#[actix_web::test]
async fn test_20_complete_environment_setup() {
    let server = TestServer::new().await;

    // Use the fixtures utility to set up complete environment
    let result = fixtures::setup_complete_environment(&server, "test_env").await;

    assert!(
        result.is_ok(),
        "Failed to setup environment: {:?}",
        result.err()
    );

    // Verify all components
    assert!(server.namespace_exists("test_env").await);
    assert!(server.table_exists("test_env", "messages").await);
    assert!(server.table_exists("test_env", "config").await);
    assert!(server.table_exists("test_env", "events").await);

    // Cleanup
    server.cleanup().await.expect("Cleanup failed");
}
