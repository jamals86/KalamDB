//! Integration tests for stream table DML operations
//!
//! Tests INSERT for stream tables with:
//! - Timestamp-based keys
//! - NO system columns (_updated, _deleted)
//! - Ephemeral mode support
//! - Real-time notifications

mod common;

use common::fixtures;
use common::TestServer;

/// Helper function to create a stream table
async fn create_stream_table(server: &TestServer, namespace: &str, table_name: &str) {
    let create_sql = format!(
        r#"CREATE STREAM TABLE {}.{} (
            id INT,
            event_type VARCHAR,
            data VARCHAR
        )"#,
        namespace, table_name
    );

    let response = server.execute_sql(&create_sql).await;
    assert_eq!(
        response.status, "success",
        "Failed to create stream table: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_stream_table_create_and_basic_insert() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    create_stream_table(&server, "test_ns", "events").await;

    // Insert event
    let response = server
        .execute_sql(
            r#"INSERT INTO test_ns.events (id, event_type, data) 
               VALUES (1, 'user_login', 'user_123')"#,
        )
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to insert event: {:?}",
        response.error
    );

    // Note: SELECT not implemented for stream tables yet
    // This test just verifies INSERT doesn't error
}

#[actix_web::test]
async fn test_stream_table_multiple_inserts() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    create_stream_table(&server, "test_ns", "events").await;

    // Insert multiple events
    let response = server
        .execute_sql(
            r#"INSERT INTO test_ns.events (id, event_type, data) 
               VALUES (1, 'user_login', 'user_123')"#,
        )
        .await;
    assert_eq!(response.status, "success");

    let response = server
        .execute_sql(
            r#"INSERT INTO test_ns.events (id, event_type, data) 
               VALUES (2, 'page_view', 'page_home')"#,
        )
        .await;
    assert_eq!(response.status, "success");

    let response = server
        .execute_sql(
            r#"INSERT INTO test_ns.events (id, event_type, data) 
               VALUES (3, 'user_logout', 'user_123')"#,
        )
        .await;
    assert_eq!(response.status, "success");
}

#[actix_web::test]
async fn test_stream_table_no_system_columns() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    create_stream_table(&server, "test_ns", "events").await;

    // Insert with only user-defined columns (no _updated, _deleted)
    let response = server
        .execute_sql(
            r#"INSERT INTO test_ns.events (id, event_type, data) 
               VALUES (1, 'test_event', 'test_data')"#,
        )
        .await;

    assert_eq!(
        response.status, "success",
        "Stream table INSERT should not require system columns: {:?}",
        response.error
    );
}
