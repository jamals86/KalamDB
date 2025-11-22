//! Integration tests for namespace validation during table creation (US5).
//!
//! Verifies that CREATE TABLE commands fail when the target namespace does not
//! exist and provide actionable guidance so operators can recover quickly.

#[path = "../common/mod.rs"]
mod common;

use std::time::Duration;

use common::{fixtures, TestServer};
use kalamdb_api::models::ResponseStatus;

#[actix_web::test]
async fn test_create_table_nonexistent_namespace_error() {
    let server = TestServer::new().await;

    let response = server
        .execute_sql(
            r#"CREATE TABLE missing_ns.audit_log (
                id INT PRIMARY KEY,
                action TEXT
            )"#,
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "Expected namespace validation failure"
    );
    let error = response.error.expect("Expected an error payload");
    assert!(
        error.message.contains("missing_ns"),
        "Error message should mention namespace: {:?}",
        error
    );
    assert!(
        error
            .message
            .contains("Create it first with CREATE NAMESPACE missing_ns"),
        "Error message should include recovery guidance: {:?}",
        error
    );
}

#[actix_web::test]
#[ignore = "CREATE TABLE (shared) requires pre-created column families at DB init"]
async fn test_create_table_after_namespace_creation() {
    let server = TestServer::new().await;
    let create_sql = r#"CREATE TABLE audit.trail (
        id INT PRIMARY KEY,
        actor TEXT
    )"#;

    // First attempt should fail because namespace is missing.
    let initial = server.execute_sql(create_sql).await;
    assert_eq!(
        initial.status,
        ResponseStatus::Error,
        "Expected failure when namespace missing"
    );

    // Create the namespace and retry.
    let ns_response = fixtures::create_namespace(&server, "audit").await;
    assert_eq!(
        ns_response.status,
        ResponseStatus::Success,
        "Namespace creation should succeed"
    );

    let retry = server.execute_sql(create_sql).await;
    assert_eq!(
        retry.status,
        ResponseStatus::Success,
        "Retry should succeed once namespace exists: {:?}",
        retry.error
    );
    assert!(
        server.table_exists("audit", "trail").await,
        "Table should exist after successful creation"
    );
}

#[actix_web::test]
async fn test_user_table_namespace_validation() {
    let server = TestServer::new().await;
    let sql = r#"CREATE TABLE workspace.notes (
        id INT PRIMARY KEY,
        content TEXT
    ) WITH (
        TYPE = 'USER'
    )"#;

    let response = server.execute_sql_as_user(sql, "user123").await;
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "User table creation should fail without namespace"
    );

    fixtures::create_namespace(&server, "workspace").await;
    let retry = server.execute_sql_as_user(sql, "user123").await;
    assert_eq!(
        retry.status,
        ResponseStatus::Success,
        "User table creation should succeed once namespace exists: {:?}",
        retry.error
    );
}

#[actix_web::test]
#[ignore = "Shared table creation requires pre-created column families at DB init"]
async fn test_shared_table_namespace_validation() {
    let server = TestServer::new().await;

    let response = server
        .execute_sql(
            r#"CREATE TABLE ops.config (
                setting TEXT,
                value TEXT
            ) WITH (
                TYPE = 'SHARED'
            )"#,
        )
        .await;
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "Shared table creation should fail without namespace"
    );

    fixtures::create_namespace(&server, "ops").await;
    let retry = server
        .execute_sql(
            r#"CREATE TABLE ops.config (
                setting TEXT,
                value TEXT
            ) WITH (
                TYPE = 'SHARED'
            )"#,
        )
        .await;
    assert_eq!(
        retry.status,
        ResponseStatus::Success,
        "Shared table creation should succeed once namespace exists: {:?}",
        retry.error
    );
}

#[actix_web::test]
async fn test_stream_table_namespace_validation() {
    let server = TestServer::new().await;

    let response = server
        .execute_sql(
            r#"CREATE TABLE telemetry.events (
                event_id TEXT,
                payload TEXT
            ) WITH (
                TYPE = 'STREAM',
                TTL_SECONDS = 60
            )"#,
        )
        .await;
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "Stream table creation should fail without namespace"
    );

    fixtures::create_namespace(&server, "telemetry").await;
    let retry = server
        .execute_sql(
            r#"CREATE TABLE telemetry.events (
                event_id TEXT,
                payload TEXT
            ) WITH (
                TYPE = 'STREAM',
                TTL_SECONDS = 60
            )"#,
        )
        .await;
    assert_eq!(
        retry.status,
        ResponseStatus::Success,
        "Stream table creation should succeed once namespace exists: {:?}",
        retry.error
    );
}

#[actix_web::test]
#[ignore = "CREATE TABLE (shared) requires pre-created column families at DB init"]
async fn test_namespace_validation_race_condition() {
    let server = TestServer::new().await;
    let table_sql = r#"CREATE TABLE race_ns.logs (
        id INT PRIMARY KEY,
        message TEXT
    )"#;

    let create_table_server = server.clone();
    let namespace_server = server.clone();

    let create_table =
        tokio::spawn(async move { create_table_server.execute_sql(table_sql).await });

    let create_namespace = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        fixtures::create_namespace(&namespace_server, "race_ns").await
    });

    let table_result = create_table.await.expect("table task panicked");
    let namespace_result = create_namespace.await.expect("namespace task panicked");

    assert_eq!(
        namespace_result.status,
        ResponseStatus::Success,
        "Namespace creation should succeed in concurrent scenario: {:?}",
        namespace_result.error
    );
    assert_eq!(
        table_result.status,
        ResponseStatus::Error,
        "Table creation should fail if namespace does not exist at submission time"
    );

    // Retry after namespace exists to confirm success.
    let retry = server.execute_sql(table_sql).await;
    assert_eq!(
        retry.status,
        ResponseStatus::Success,
        "Retry should succeed once namespace creation completes: {:?}",
        retry.error
    );
}

#[actix_web::test]
async fn test_error_message_includes_guidance() {
    let server = TestServer::new().await;

    let response = server
        .execute_sql(
            r#"CREATE TABLE docs.notes (
                id INT
            )"#,
        )
        .await;

    let error = response.error.expect("Expected an error");
    assert!(
        error
            .message
            .contains("Create it first with CREATE NAMESPACE docs"),
        "Error message should nudge operator toward CREATE NAMESPACE: {:?}",
        error
    );
}
