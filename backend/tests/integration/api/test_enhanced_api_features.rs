//! Integration tests for Enhanced API features (User Story 9).
//!
//! These tests exercise the REST batch execution semantics introduced in US9,
//! the renamed response timing field, and explicit error messaging for failed
//! batch statements. Additional WebSocket and system table coverage will be
//! added alongside their implementations.

#[path = "../common/mod.rs"]
mod common;

use actix_web::test;
use common::{
    create_test_jwt, fixtures, start_http_server_for_websocket_tests, start_test_server,
    websocket::WebSocketClient,
};
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

fn unique_namespace(prefix: &str) -> String {
    format!("{}_{}", prefix, Uuid::new_v4().simple())
}

async fn execute_sql_request(server: &common::HttpTestServer, sql: &str) -> Value {
    let request = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "test_user"))
        .set_json(&json!({ "sql": sql }));

    let response = server.execute_request(request).await;
    assert!(
        response.status().is_success(),
        "Expected successful HTTP response, got {}",
        response.status()
    );

    test::read_body_json(response).await
}

#[actix_web::test]
async fn test_api_response_uses_took_ms() {
    let server = start_test_server().await;
    let body = execute_sql_request(&server, "SELECT 42 as answer").await;

    assert_eq!(body["status"], "success", "Query should succeed");
    assert!(
        body.get("took_ms").is_some(),
        "Response should include took_ms field: {body}",
    );
    assert!(
        body.get("execution_time_ms").is_none(),
        "Legacy execution_time_ms field should be absent"
    );
}

#[actix_web::test]
async fn test_batch_sql_sequential_execution() {
    let server = start_test_server().await;
    let namespace = unique_namespace("batch_seq");
    let sql = format!(
        "CREATE NAMESPACE {ns}; \
         CREATE USER TABLE {ns}.messages (id INT, content TEXT); \
         INSERT INTO {ns}.messages (id, content) VALUES (1, 'hello'); \
         SELECT id, content FROM {ns}.messages ORDER BY id;",
        ns = namespace
    );

    let body = execute_sql_request(&server, &sql).await;
    assert_eq!(body["status"], "success");

    let results = body["results"].as_array().expect("results array expected");
    assert_eq!(
        results.len(),
        4,
        "Expected one result per statement: {results:?}"
    );

    // Final SELECT should return the inserted row
    let select_rows = results[3]["rows"].as_array().expect("SELECT rows expected");
    assert_eq!(select_rows.len(), 1, "One row should be returned");
    assert_eq!(
        select_rows[0]["id"], 1,
        "Row id should match inserted value"
    );
    assert_eq!(
        select_rows[0]["content"], "hello",
        "Row content should match inserted value"
    );
}

#[actix_web::test]
async fn test_batch_sql_partial_failure_commits_previous() {
    let server = start_test_server().await;
    let namespace = unique_namespace("batch_partial");

    let failing_sql = format!(
        "CREATE NAMESPACE {ns}; \
         CREATE USER TABLE {ns}.items (id INT, content TEXT); \
         INSERT INTO {ns}.items (id, content) VALUES (1, 'first'); \
         INSERT INTO {ns}.items (id, missing_column) VALUES (2, 'second');",
        ns = namespace
    );

    let request = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "test_user"))
        .set_json(&json!({ "sql": failing_sql }));

    let response = server.execute_request(request).await;
    assert_eq!(response.status(), 400, "Should return 400 on batch failure");

    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["status"], "error");
    let error = body["error"]["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        error.contains("Statement 4"),
        "Error should report failing statement index: {error}"
    );

    // Verify the successful insert was committed
    let check_sql = format!(
        "SELECT id, content FROM {ns}.items ORDER BY id;",
        ns = namespace
    );
    let check_body = execute_sql_request(&server, &check_sql).await;
    assert_eq!(check_body["status"], "success");

    let rows = check_body["results"][0]["rows"]
        .as_array()
        .expect("rows array expected");
    assert_eq!(rows.len(), 1, "First insert should remain committed");
    assert_eq!(rows[0]["id"], 1);
    assert_eq!(rows[0]["content"], "first");
}

#[actix_web::test]
async fn test_batch_sql_explicit_transaction() {
    let server = start_test_server().await;
    let namespace = unique_namespace("batch_txn");

    // Create namespace and table first so transaction focuses on DML.
    let setup_sql = format!(
        "CREATE NAMESPACE {ns}; \
         CREATE USER TABLE {ns}.orders (id INT, amount INT);",
        ns = namespace
    );
    execute_sql_request(&server, &setup_sql).await;

    let txn_sql = format!(
        "BEGIN; \
         INSERT INTO {ns}.orders (id, amount) VALUES (1, 10); \
         INSERT INTO {ns}.orders (id, amount) VALUES (2, 20); \
         COMMIT; \
         SELECT SUM(amount) AS total FROM {ns}.orders;",
        ns = namespace
    );

    let body = execute_sql_request(&server, &txn_sql).await;
    assert_eq!(body["status"], "success");

    let results = body["results"].as_array().expect("results array expected");
    assert_eq!(results.len(), 5, "Transaction batch should yield 5 results");

    let total = results[4]["rows"][0]["total"]
        .as_i64()
        .expect("SUM(amount) expected");
    assert_eq!(total, 30, "Both inserts inside transaction should apply");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn test_websocket_initial_data_fetch() {
    let (server, base_url) = start_http_server_for_websocket_tests().await;
    let namespace = unique_namespace("ws_init");
    let user_id = "ws_user";

    fixtures::create_namespace(&server, &namespace).await;
    let create_table = format!(
        "CREATE USER TABLE {}.events (id INT, content TEXT)",
        namespace
    );
    assert_eq!(server.execute_sql(&create_table).await.status, "success");

    for i in 0..10 {
        let insert_sql = format!(
            "INSERT INTO {}.events (id, content) VALUES ({}, 'message {}')",
            namespace, i, i
        );
        assert_eq!(
            server
                .execute_sql_as_user(&insert_sql, user_id)
                .await
                .status,
            "success"
        );
    }

    let ws_url = format!("{}/v1/ws", base_url.replace("http", "ws"));
    let token = create_test_jwt(user_id, "kalamdb-dev-secret-key-change-in-production", 3600);

    let mut client = WebSocketClient::connect_with_auth(&ws_url, Some(&token))
        .await
        .expect("WebSocket connection should succeed");

    let query_sql = format!("SELECT id, content FROM {}.events ORDER BY id", namespace);

    client
        .subscribe_with_options("events", &query_sql, json!({ "last_rows": 5 }))
        .await
        .expect("Subscription should succeed");

    let initial = client
        .wait_for_initial_data("events", Duration::from_secs(2))
        .await
        .expect("Initial data should arrive");

    assert_eq!(initial.count, 5, "Should return requested number of rows");
    assert_eq!(initial.rows.len(), 5, "Row payload should match count");
    assert_eq!(initial.rows[0]["id"].as_i64(), Some(5));
    assert_eq!(initial.rows[4]["id"].as_i64(), Some(9));

    client.disconnect().await.expect("WebSocket disconnect");
    server.cleanup().await.expect("cleanup");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn test_drop_table_with_active_subscriptions() {
    let (server, base_url) = start_http_server_for_websocket_tests().await;
    let namespace = unique_namespace("ws_drop_guard");
    let user_id = "drop_guard_user";

    fixtures::create_namespace(&server, &namespace).await;
    let create_table = format!(
        "CREATE USER TABLE {}.logs (id INT, content TEXT)",
        namespace
    );
    assert_eq!(server.execute_sql(&create_table).await.status, "success");

    let ws_url = format!("{}/v1/ws", base_url.replace("http", "ws"));
    let token = create_test_jwt(user_id, "kalamdb-dev-secret-key-change-in-production", 3600);

    let mut client = WebSocketClient::connect_with_auth(&ws_url, Some(&token))
        .await
        .expect("WebSocket connection");

    let query_sql = format!("SELECT * FROM {}.logs", namespace);
    client
        .subscribe("logs", &query_sql)
        .await
        .expect("Subscription should succeed");

    sleep(Duration::from_millis(100)).await;

    let drop_sql = format!("DROP USER TABLE {}.logs", namespace);
    let drop_response = server.execute_sql(&drop_sql).await;
    assert_eq!(drop_response.status, "error");
    let error = drop_response.error.expect("drop should return error");
    assert!(
        error.message.contains("active live queries"),
        "Error message should mention active subscriptions: {}",
        error.message
    );

    client.disconnect().await.expect("disconnect");
    sleep(Duration::from_millis(100)).await;

    let second_attempt = server.execute_sql(&drop_sql).await;
    assert_eq!(second_attempt.status, "success");
    server.cleanup().await.expect("cleanup");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn test_kill_live_query_command() {
    let (server, base_url) = start_http_server_for_websocket_tests().await;
    let namespace = unique_namespace("ws_kill");
    let user_id = "kill_user";

    fixtures::create_namespace(&server, &namespace).await;
    let create_table = format!(
        "CREATE USER TABLE {}.metrics (id INT, value INT)",
        namespace
    );
    assert_eq!(server.execute_sql(&create_table).await.status, "success");

    let ws_url = format!("{}/v1/ws", base_url.replace("http", "ws"));
    let token = create_test_jwt(user_id, "kalamdb-dev-secret-key-change-in-production", 3600);

    let mut client = WebSocketClient::connect_with_auth(&ws_url, Some(&token))
        .await
        .expect("connect");

    let query_sql = format!("SELECT * FROM {}.metrics", namespace);
    client
        .subscribe("metrics", &query_sql)
        .await
        .expect("subscribe");

    sleep(Duration::from_millis(100)).await;

    let live_id_sql = format!(
        "SELECT live_id FROM system.live_queries WHERE user_id = '{}'",
        user_id
    );
    let live_query_rows = server.execute_sql(&live_id_sql).await;
    assert_eq!(live_query_rows.status, "success");
    let rows = live_query_rows.results[0]
        .rows
        .clone()
        .expect("rows expected");
    assert_eq!(rows.len(), 1, "Exactly one live query expected");
    let live_id = rows[0]["live_id"].as_str().expect("live_id as str");

    let kill_sql = format!("KILL LIVE QUERY '{}'", live_id);
    let kill_response = server.execute_sql(&kill_sql).await;
    assert_eq!(kill_response.status, "success");

    sleep(Duration::from_millis(100)).await;

    let remaining = server.execute_sql(&live_id_sql).await;
    let remaining_rows = remaining.results[0].rows.clone().unwrap_or_default();
    assert!(remaining_rows.is_empty(), "Live query should be removed");

    client.disconnect().await.expect("disconnect after kill");
    server.cleanup().await.expect("cleanup");
}
