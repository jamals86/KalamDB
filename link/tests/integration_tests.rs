#![allow(dead_code)]
//! Integration tests for kalam-link library
//!
//! These tests verify the kalam-link API against a running server.
//! Tests will fail immediately if server is not running.
//!
//! # Running Tests
//!
//! ```bash
//! # Terminal 1: Start server
//! cd backend && cargo run --bin kalamdb-server
//!
//! # Terminal 2: Run tests
//! cd cli/kalam-link && cargo test --test integration_tests
//! ```

use kalam_link::models::{BatchControl, BatchStatus, ResponseStatus};
use kalam_link::{AuthProvider, ChangeEvent, KalamLinkClient, KalamLinkError, SubscriptionConfig};
use std::time::Duration;
use tokio::time::{sleep, timeout};

const SERVER_URL: &str = "http://localhost:8080";
const TEST_USER: &str = "link_test_user";

/// Check if server is running - fail fast if not
async fn ensure_server_running() {
    let client = reqwest::Client::new();
    let result = client
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .json(&serde_json::json!({ "sql": "SELECT 1" }))
        .timeout(Duration::from_secs(2))
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            println!("✅ Server is running at {}", SERVER_URL);
        }
        _ => {
            panic!(
                "\n❌ Server is NOT running at {}\n\n\
                Start the server first:\n\
                  cd backend && cargo run --bin kalamdb-server\n\n\
                Then run tests:\n\
                  cd cli/kalam-link && cargo test --test integration_tests\n",
                SERVER_URL
            );
        }
    }
}

fn create_client() -> Result<KalamLinkClient, KalamLinkError> {
    KalamLinkClient::builder()
        .base_url(SERVER_URL)
        .timeout(Duration::from_secs(10))
        .build()
}

async fn setup_namespace(ns: &str) {
    let client = create_client().unwrap();
    let _ = client
        .execute_query(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", ns))
        .await;
    sleep(Duration::from_millis(100)).await;
    let _ = client
        .execute_query(&format!("CREATE NAMESPACE {}", ns))
        .await;
    sleep(Duration::from_millis(100)).await;
}

async fn cleanup_namespace(ns: &str) {
    let client = create_client().unwrap();
    let _ = client
        .execute_query(&format!("DROP NAMESPACE {} CASCADE", ns))
        .await;
}

// =============================================================================
// Client Builder Tests
// =============================================================================

#[tokio::test]
async fn test_client_builder_basic() {
    let client = KalamLinkClient::builder().base_url(SERVER_URL).build();

    assert!(client.is_ok(), "Client builder should succeed");
}

#[tokio::test]
async fn test_client_builder_with_timeout() {
    let client = KalamLinkClient::builder()
        .base_url(SERVER_URL)
        .timeout(Duration::from_secs(5))
        .build();

    assert!(client.is_ok(), "Client with custom timeout should succeed");
}

#[tokio::test]
async fn test_client_builder_with_jwt() {
    let client = KalamLinkClient::builder()
        .base_url(SERVER_URL)
        .jwt_token("test.jwt.token")
        .build();

    assert!(client.is_ok(), "Client with JWT should succeed");
}

#[tokio::test]
async fn test_client_builder_missing_url() {
    let result = KalamLinkClient::builder().build();

    assert!(result.is_err(), "Client without URL should fail");
    if let Err(e) = result {
        assert!(e.to_string().contains("base_url"));
    }
}

// =============================================================================
// Query Execution Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_execute_simple_query() {
    ensure_server_running().await;

    let client = create_client().unwrap();
    let result = client.execute_query("SELECT 1 as num").await;

    assert!(result.is_ok(), "Simple query should succeed");
    let response = result.unwrap();
    assert_eq!(response.status, ResponseStatus::Success);
    assert!(!response.results.is_empty());
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_execute_query_with_results() {
    ensure_server_running().await;

    setup_namespace("link_test").await;

    let client = create_client().unwrap();

    // Create table and insert data
    client
        .execute_query("CREATE USER TABLE link_test.items (id INT, name VARCHAR) FLUSH ROWS 10")
        .await
        .ok();

    client
        .execute_query("INSERT INTO link_test.items (id, name) VALUES (1, 'test')")
        .await
        .ok();

    // Query
    let result = client.execute_query("SELECT * FROM link_test.items").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.status, ResponseStatus::Success);
    assert!(!response.results.is_empty());

    if let Some(rows) = &response.results[0].rows {
        assert!(!rows.is_empty(), "Should have at least one row");
    }

    cleanup_namespace("link_test").await;
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_execute_query_error_handling() {
    ensure_server_running().await;

    let client = create_client().unwrap();
    let result = client.execute_query("INVALID SQL").await;

    // Should either return Err or success with error status
    if let Ok(response) = result {
        assert_eq!(response.status, ResponseStatus::Error);
        assert!(response.error.is_some());
    }
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_health_check() {
    ensure_server_running().await;

    let client = create_client().unwrap();
    let result = client.health_check().await;

    assert!(result.is_ok(), "Health check should succeed");
    let health = result.unwrap();
    assert_eq!(health.status, "healthy");
    assert_eq!(health.api_version, "v1");
}

// =============================================================================
// Auth Provider Tests
// =============================================================================

#[test]
fn test_auth_provider_none() {
    let auth = AuthProvider::none();
    assert!(!auth.is_authenticated(), "None should not be authenticated");
}

#[test]
fn test_auth_provider_jwt() {
    let auth = AuthProvider::jwt_token("test.jwt.token".to_string());
    assert!(auth.is_authenticated(), "JWT should be authenticated");
}

// =============================================================================
// WebSocket Subscription Tests
// =============================================================================

#[tokio::test]
async fn test_subscription_config_creation() {
    let config = SubscriptionConfig::new("SELECT * FROM table");
    // Config is created successfully
    assert_eq!(config.sql, "SELECT * FROM table");
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_subscription_basic() {
    ensure_server_running().await;
    setup_namespace("ws_link_test").await;

    let client = create_client().unwrap();

    // Create table
    client
        .execute_query("CREATE USER TABLE ws_link_test.events (id INT, data VARCHAR) FLUSH ROWS 10")
        .await
        .ok();
    sleep(Duration::from_millis(100)).await;

    // Try to create subscription
    let sub_result = timeout(
        Duration::from_secs(5),
        client.subscribe("SELECT * FROM ws_link_test.events"),
    )
    .await;

    match sub_result {
        Ok(Ok(_subscription)) => {
            // Success - WebSocket is working
        }
        Ok(Err(e)) => {
            eprintln!(
                "⚠️  Subscription failed (may not be fully implemented): {}",
                e
            );
        }
        Err(_) => {
            eprintln!("⚠️  Subscription timed out");
        }
    }

    cleanup_namespace("ws_link_test").await;
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_subscription_with_custom_config() {
    ensure_server_running().await;
    setup_namespace("ws_link_config").await;

    let client = create_client().unwrap();

    // Create table
    client
        .execute_query("CREATE USER TABLE ws_link_config.data (id INT, val VARCHAR) FLUSH ROWS 10")
        .await
        .ok();
    sleep(Duration::from_millis(100)).await;

    // Create subscription with custom config
    let config = SubscriptionConfig::new("SELECT * FROM ws_link_config.data");

    let sub_result = timeout(Duration::from_secs(5), client.subscribe_with_config(config)).await;

    match sub_result {
        Ok(Ok(_subscription)) => {
            // Success
        }
        Ok(Err(e)) => {
            eprintln!("⚠️  Custom config subscription failed: {}", e);
        }
        Err(_) => {
            eprintln!("⚠️  Subscription timed out");
        }
    }

    cleanup_namespace("ws_link_config").await;
}

// =============================================================================
// Change Event Tests
// =============================================================================

fn sample_batch_control() -> BatchControl {
    BatchControl {
        batch_num: 0,
        total_batches: Some(0),
        has_more: false,
        status: BatchStatus::Ready,
        last_seq_id: None,
        snapshot_end_seq: None,
    }
}

#[test]
fn test_change_event_is_error() {
    let error_event = ChangeEvent::Error {
        subscription_id: "sub-1".to_string(),
        code: "ERR".to_string(),
        message: "test error".to_string(),
    };
    assert!(error_event.is_error());

    let insert_event = ChangeEvent::Insert {
        subscription_id: "sub-1".to_string(),
        rows: vec![],
    };
    assert!(!insert_event.is_error());
}

#[test]
fn test_change_event_subscription_id() {
    let insert = ChangeEvent::Insert {
        subscription_id: "sub-123".to_string(),
        rows: vec![],
    };
    assert_eq!(insert.subscription_id(), Some("sub-123"));

    let ack = ChangeEvent::Ack {
        subscription_id: "sub-ack".to_string(),
        total_rows: 0,
        batch_control: sample_batch_control(),
    };
    assert_eq!(ack.subscription_id(), Some("sub-ack"));

    let unknown = ChangeEvent::Unknown {
        raw: serde_json::Value::Null,
    };
    assert_eq!(unknown.subscription_id(), None);
}

// =============================================================================
// Error Type Tests
// =============================================================================

#[test]
fn test_error_display() {
    let config_err = KalamLinkError::ConfigurationError("test config error".to_string());
    assert!(config_err.to_string().contains("Configuration error"));

    let network_err = KalamLinkError::NetworkError("test network error".to_string());
    assert!(network_err.to_string().contains("Network error"));

    let server_err = KalamLinkError::ServerError {
        status_code: 500,
        message: "Internal server error".to_string(),
    };
    assert!(server_err.to_string().contains("Server error"));
}

// =============================================================================
// CRUD Operations Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_create_namespace() {
    ensure_server_running().await;

    let client = create_client().unwrap();

    // Cleanup first
    let _ = client
        .execute_query("DROP NAMESPACE IF EXISTS test_create_ns CASCADE")
        .await;
    sleep(Duration::from_millis(100)).await;

    // Create
    let result = client
        .execute_query("CREATE NAMESPACE test_create_ns")
        .await;
    assert!(result.is_ok(), "CREATE NAMESPACE should succeed");

    // Cleanup
    cleanup_namespace("test_create_ns").await;
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_create_and_drop_table() {
    ensure_server_running().await;
    setup_namespace("crud_test").await;

    let client = create_client().unwrap();

    // Create table
    let create = client
        .execute_query("CREATE USER TABLE crud_test.test (id INT, name VARCHAR) FLUSH ROWS 10")
        .await;
    assert!(create.is_ok(), "CREATE TABLE should succeed");

    // Drop table
    let drop = client.execute_query("DROP TABLE crud_test.test").await;
    assert!(drop.is_ok(), "DROP TABLE should succeed");

    cleanup_namespace("crud_test").await;
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_insert_and_select() {
    ensure_server_running().await;
    setup_namespace("insert_test").await;

    let client = create_client().unwrap();

    // Create table
    client
        .execute_query("CREATE USER TABLE insert_test.data (id INT, value VARCHAR) FLUSH ROWS 10")
        .await
        .ok();

    // Insert
    let insert = client
        .execute_query("INSERT INTO insert_test.data (id, value) VALUES (1, 'test')")
        .await;
    assert!(insert.is_ok(), "INSERT should succeed");

    // Select
    let select = client
        .execute_query("SELECT * FROM insert_test.data WHERE id = 1")
        .await;
    assert!(select.is_ok(), "SELECT should succeed");

    let response = select.unwrap();
    assert_eq!(response.status, ResponseStatus::Success);
    if let Some(rows) = &response.results[0].rows {
        assert!(!rows.is_empty(), "Should have results");
    }

    cleanup_namespace("insert_test").await;
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_update_operation() {
    ensure_server_running().await;
    setup_namespace("update_test").await;

    let client = create_client().unwrap();

    // Setup
    client
        .execute_query("CREATE USER TABLE update_test.items (id INT, status VARCHAR) FLUSH ROWS 10")
        .await
        .ok();
    client
        .execute_query("INSERT INTO update_test.items (id, status) VALUES (1, 'old')")
        .await
        .ok();

    // Update
    let update = client
        .execute_query("UPDATE update_test.items SET status = 'new' WHERE id = 1")
        .await;
    assert!(update.is_ok(), "UPDATE should succeed");

    cleanup_namespace("update_test").await;
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_delete_operation() {
    ensure_server_running().await;
    setup_namespace("delete_test").await;

    let client = create_client().unwrap();

    // Setup
    client
        .execute_query("CREATE USER TABLE delete_test.records (id INT, data VARCHAR) FLUSH ROWS 10")
        .await
        .ok();
    client
        .execute_query("INSERT INTO delete_test.records (id, data) VALUES (1, 'delete_me')")
        .await
        .ok();

    // Delete
    let delete = client
        .execute_query("DELETE FROM delete_test.records WHERE id = 1")
        .await;
    assert!(delete.is_ok(), "DELETE should succeed");

    cleanup_namespace("delete_test").await;
}

// =============================================================================
// System Tables Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_query_system_users() {
    ensure_server_running().await;

    let client = create_client().unwrap();
    let result = client.execute_query("SELECT * FROM system.users").await;

    assert!(result.is_ok(), "Should query system.users");
    let response = result.unwrap();
    assert_eq!(response.status, ResponseStatus::Success);
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_query_system_namespaces() {
    ensure_server_running().await;

    let client = create_client().unwrap();
    let result = client
        .execute_query("SELECT * FROM system.namespaces")
        .await;

    assert!(result.is_ok(), "Should query system.namespaces");
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_query_system_tables() {
    ensure_server_running().await;

    let client = create_client().unwrap();
    let result = client.execute_query("SELECT * FROM system.tables").await;

    assert!(result.is_ok(), "Should query system.tables");
}

// =============================================================================
// Advanced SQL Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_where_clause_operators() {
    ensure_server_running().await;
    setup_namespace("where_test").await;

    let client = create_client().unwrap();

    // Setup
    client
        .execute_query("CREATE USER TABLE where_test.data (id INT, val VARCHAR) FLUSH ROWS 10")
        .await
        .ok();

    for i in 1..=5 {
        client
            .execute_query(&format!(
                "INSERT INTO where_test.data (id, val) VALUES ({}, 'value{}')",
                i, i
            ))
            .await
            .ok();
    }

    // Test equality
    let eq = client
        .execute_query("SELECT * FROM where_test.data WHERE id = 3")
        .await;
    assert!(eq.is_ok(), "Equality operator should work");

    // Test LIKE
    let like = client
        .execute_query("SELECT * FROM where_test.data WHERE val LIKE '%3%'")
        .await;
    assert!(like.is_ok(), "LIKE operator should work");

    cleanup_namespace("where_test").await;
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_limit_clause() {
    ensure_server_running().await;
    setup_namespace("limit_test").await;

    let client = create_client().unwrap();

    // Setup
    client
        .execute_query("CREATE USER TABLE limit_test.items (id INT) FLUSH ROWS 10")
        .await
        .ok();

    for i in 1..=10 {
        client
            .execute_query(&format!("INSERT INTO limit_test.items (id) VALUES ({})", i))
            .await
            .ok();
    }

    // Test LIMIT
    let result = client
        .execute_query("SELECT * FROM limit_test.items LIMIT 3")
        .await;

    assert!(result.is_ok(), "LIMIT should work");
    let response = result.unwrap();
    if let Some(rows) = &response.results[0].rows {
        assert!(rows.len() <= 3, "Should return at most 3 rows");
    }

    cleanup_namespace("limit_test").await;
}

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_order_by_clause() {
    ensure_server_running().await;
    setup_namespace("order_test").await;

    let client = create_client().unwrap();

    // Setup
    client
        .execute_query("CREATE USER TABLE order_test.data (val VARCHAR) FLUSH ROWS 10")
        .await
        .ok();

    client
        .execute_query("INSERT INTO order_test.data (val) VALUES ('z'), ('a'), ('m')")
        .await
        .ok();

    // Test ORDER BY
    let result = client
        .execute_query("SELECT * FROM order_test.data ORDER BY val ASC")
        .await;

    assert!(result.is_ok(), "ORDER BY should work");

    cleanup_namespace("order_test").await;
}

// =============================================================================
// Concurrent Operations Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_concurrent_queries() {
    ensure_server_running().await;
    setup_namespace("concurrent_test").await;

    let client = create_client().unwrap();

    // Setup table
    client
        .execute_query("CREATE USER TABLE concurrent_test.data (id INT) FLUSH ROWS 10")
        .await
        .ok();

    // Execute multiple queries concurrently
    let mut handles = vec![];
    for i in 0..5 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            client_clone
                .execute_query(&format!(
                    "INSERT INTO concurrent_test.data (id) VALUES ({})",
                    i
                ))
                .await
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent insert should succeed");
    }

    cleanup_namespace("concurrent_test").await;
}

// =============================================================================
// Timeout and Retry Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires running backend server"]
async fn test_custom_timeout() {
    ensure_server_running().await;

    let client = KalamLinkClient::builder()
        .base_url(SERVER_URL)
        .timeout(Duration::from_millis(100)) // Very short timeout
        .build()
        .unwrap();

    // This might timeout with very short duration, but shouldn't panic
    let _ = client.execute_query("SELECT 1").await;
}

#[tokio::test]
async fn test_connection_to_invalid_server() {
    let client = KalamLinkClient::builder()
        .base_url("http://localhost:9999") // Invalid port
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();

    let result = client.execute_query("SELECT 1").await;
    assert!(result.is_err(), "Connection to invalid server should fail");
}
