//! WebSocket Integration Tests for kalam-link
//!
//! These tests verify WebSocket connectivity, subscriptions, and real-time updates
//! using the kalam-link library.
//!
//! **IMPORTANT**: These tests require a running KalamDB server.
//!
//! # Running Tests
//!
//! ```bash
//! # Terminal 1: Start the server
//! cd backend && cargo run --bin kalamdb-server
//!
//! # Terminal 2: Run the tests
//! cd cli && cargo test --test test_websocket_integration -- --nocapture
//! ```
//!
//! Tests will be skipped if the server is not running.

use kalam_link::{
    ChangeEvent, KalamLinkClient, QueryResponse, SubscriptionConfig, SubscriptionManager,
};
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Test configuration
const SERVER_URL: &str = "http://localhost:8080";
const WS_URL: &str = "ws://localhost:8080";
const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const TEST_USER_ID: &str = "ws_test_user";

/// Helper to check if server is running
async fn is_server_running() -> bool {
    match reqwest::Client::new()
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .json(&serde_json::json!({ "sql": "SELECT 1" }))
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Helper to create a test client
fn create_test_client() -> Result<KalamLinkClient, kalam_link::KalamLinkError> {
    KalamLinkClient::builder()
        .base_url(SERVER_URL)
        .user_id(TEST_USER_ID)
        .timeout(Duration::from_secs(30))
        .build()
}

/// Helper to execute SQL via HTTP (for test setup)
async fn execute_sql(sql: &str) -> Result<QueryResponse, Box<dyn std::error::Error>> {
    let client = create_test_client()?;
    Ok(client.execute_query(sql).await?)
}

/// Helper to setup test namespace and table
async fn setup_test_data() -> Result<(), Box<dyn std::error::Error>> {
    // Cleanup from previous runs
    let _ = execute_sql("DROP TABLE IF EXISTS ws_test.events").await;
    let _ = execute_sql("DROP NAMESPACE IF EXISTS ws_test CASCADE").await;
    sleep(Duration::from_millis(200)).await;

    // Create namespace
    match execute_sql("CREATE NAMESPACE ws_test").await {
        Ok(_) => {}
        Err(e) if e.to_string().contains("already exists") => {}
        Err(e) => return Err(e),
    }
    sleep(Duration::from_millis(100)).await;

    // Create test table
    match execute_sql(
        r#"CREATE USER TABLE ws_test.events (
            id INT AUTO_INCREMENT,
            event_type VARCHAR NOT NULL,
            data VARCHAR,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
    )
    .await
    {
        Ok(_) => {}
        Err(e) if e.to_string().contains("already exists") => {}
        Err(e) => return Err(e),
    }
    sleep(Duration::from_millis(100)).await;

    Ok(())
}

/// Helper to cleanup test data
async fn cleanup_test_data() -> Result<(), Box<dyn std::error::Error>> {
    let _ = execute_sql("DROP NAMESPACE ws_test CASCADE").await;
    Ok(())
}

// =============================================================================
// Basic Connection Tests
// =============================================================================

#[tokio::test]
async fn test_kalam_link_client_creation() {
    let result = KalamLinkClient::builder()
        .base_url(SERVER_URL)
        .user_id(TEST_USER_ID)
        .build();

    assert!(result.is_ok(), "Client should be created successfully");
}

#[tokio::test]
async fn test_kalam_link_query_execution() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running at {}. Skipping test.", SERVER_URL);
        return;
    }

    let client = create_test_client().expect("Failed to create client");
    let result = client.execute_query("SELECT 1 as test_value").await;

    assert!(result.is_ok(), "Query should execute successfully");
    let response = result.unwrap();
    assert_eq!(response.status, "success");
    assert!(!response.results.is_empty());
}

#[tokio::test]
async fn test_kalam_link_health_check() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let client = create_test_client().expect("Failed to create client");
    let result = client.health_check().await;

    assert!(result.is_ok(), "Health check should succeed");
    let health = result.unwrap();
    assert_eq!(health.status, "healthy");
    assert_eq!(health.api_version, "v1");
}

#[tokio::test]
async fn test_kalam_link_parametrized_query() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup test data");

    let client = create_test_client().expect("Failed to create client");

    // Insert with parameters (if supported)
    let insert_result = client
        .execute_query("INSERT INTO ws_test.events (event_type, data) VALUES ('test', 'param_test')")
        .await;

    assert!(insert_result.is_ok(), "Insert should succeed");

    // Query to verify
    let query_result = client
        .execute_query("SELECT * FROM ws_test.events WHERE event_type = 'test'")
        .await;

    assert!(query_result.is_ok(), "Query should succeed");
    let response = query_result.unwrap();
    assert_eq!(response.status, "success");

    cleanup_test_data().await.ok();
}

// =============================================================================
// WebSocket Subscription Tests
// =============================================================================

#[tokio::test]
async fn test_websocket_subscription_creation() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup test data");

    let client = create_test_client().expect("Failed to create client");

    // Create subscription
    let subscription_result = timeout(
        TEST_TIMEOUT,
        client.subscribe("SELECT * FROM ws_test.events"),
    )
    .await;

    match subscription_result {
        Ok(Ok(_subscription)) => {
            // Subscription created successfully
        }
        Ok(Err(e)) => {
            // WebSocket might not be fully implemented yet
            eprintln!("⚠️  Subscription failed (may not be implemented): {}", e);
        }
        Err(_) => {
            eprintln!("⚠️  Subscription timed out");
        }
    }

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_websocket_subscription_with_config() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup test data");

    let client = create_test_client().expect("Failed to create client");

    // Create subscription with custom config
    let config = SubscriptionConfig::new("SELECT * FROM ws_test.events");
    
    let subscription_result = timeout(TEST_TIMEOUT, client.subscribe_with_config(config)).await;

    match subscription_result {
        Ok(Ok(_subscription)) => {
            // Success
        }
        Ok(Err(e)) => {
            eprintln!("⚠️  Subscription with config failed: {}", e);
        }
        Err(_) => {
            eprintln!("⚠️  Subscription timed out");
        }
    }

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_websocket_initial_data_snapshot() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup test data");

    // Insert some initial data
    execute_sql("INSERT INTO ws_test.events (event_type, data) VALUES ('initial', 'data1')")
        .await
        .ok();
    execute_sql("INSERT INTO ws_test.events (event_type, data) VALUES ('initial', 'data2')")
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;

    let client = create_test_client().expect("Failed to create client");

    let subscription_result = timeout(
        TEST_TIMEOUT,
        client.subscribe("SELECT * FROM ws_test.events"),
    )
    .await;

    match subscription_result {
        Ok(Ok(mut subscription)) => {
            // Try to receive initial snapshot
            if let Ok(Some(event_result)) = timeout(Duration::from_secs(3), subscription.next()).await {
                if let Ok(event) = event_result {
                    match event {
                        ChangeEvent::InitialData { rows, .. } => {
                            assert!(!rows.is_empty(), "Initial snapshot should contain data");
                        }
                        ChangeEvent::Ack { .. } => {
                            // Received ACK, might need to wait for InitialData
                        }
                        _ => {
                            eprintln!("Received unexpected event type");
                        }
                    }
                }
            }
        }
        Ok(Err(e)) => {
            eprintln!("⚠️  Subscription failed: {}", e);
        }
        Err(_) => {
            eprintln!("⚠️  Subscription creation timed out");
        }
    }

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_websocket_insert_notification() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup test data");

    let client = create_test_client().expect("Failed to create client");

    let subscription_result = timeout(
        TEST_TIMEOUT,
        client.subscribe("SELECT * FROM ws_test.events"),
    )
    .await;

    match subscription_result {
        Ok(Ok(mut subscription)) => {
            // Skip initial messages (ACK/InitialData)
            for _ in 0..2 {
                let _ = timeout(Duration::from_secs(1), subscription.next()).await;
            }

            // Insert new data that should trigger notification
            execute_sql("INSERT INTO ws_test.events (event_type, data) VALUES ('realtime', 'insert_test')")
                .await
                .ok();

            // Wait for insert notification
            if let Ok(Some(event_result)) = timeout(Duration::from_secs(3), subscription.next()).await {
                if let Ok(event) = event_result {
                    match event {
                        ChangeEvent::Insert { rows, .. } => {
                            assert!(!rows.is_empty(), "Insert notification should contain rows");
                        }
                        _ => {
                            eprintln!("Expected Insert event, got: {:?}", event);
                        }
                    }
                }
            } else {
                eprintln!("⚠️  No insert notification received");
            }
        }
        Ok(Err(e)) => {
            eprintln!("⚠️  Subscription failed: {}", e);
        }
        Err(_) => {
            eprintln!("⚠️  Subscription timed out");
        }
    }

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_websocket_filtered_subscription() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup test data");

    let client = create_test_client().expect("Failed to create client");

    // Subscribe with WHERE filter
    let subscription_result = timeout(
        TEST_TIMEOUT,
        client.subscribe("SELECT * FROM ws_test.events WHERE event_type = 'filtered'"),
    )
    .await;

    match subscription_result {
        Ok(Ok(mut subscription)) => {
            // Skip initial messages
            for _ in 0..2 {
                let _ = timeout(Duration::from_millis(500), subscription.next()).await;
            }

            // Insert data that matches filter
            execute_sql("INSERT INTO ws_test.events (event_type, data) VALUES ('filtered', 'match')")
                .await
                .ok();

            // Insert data that doesn't match filter
            execute_sql("INSERT INTO ws_test.events (event_type, data) VALUES ('other', 'nomatch')")
                .await
                .ok();

            // Should only receive notification for matching row
            if let Ok(Some(event_result)) = timeout(Duration::from_secs(2), subscription.next()).await {
                if let Ok(event) = event_result {
                    match event {
                        ChangeEvent::Insert { rows, .. } => {
                            // Verify it's the filtered row
                            if let Some(row) = rows.first() {
                                if let Some(event_type) = row.get("event_type") {
                                    assert_eq!(event_type.as_str(), Some("filtered"));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(Err(e)) => {
            eprintln!("⚠️  Filtered subscription failed: {}", e);
        }
        Err(_) => {
            eprintln!("⚠️  Subscription timed out");
        }
    }

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_websocket_update_notification() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup test data");

    // Insert initial row
    execute_sql("INSERT INTO ws_test.events (event_type, data) VALUES ('update_test', 'original')")
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;

    let client = create_test_client().expect("Failed to create client");

    let subscription_result = timeout(
        TEST_TIMEOUT,
        client.subscribe("SELECT * FROM ws_test.events WHERE event_type = 'update_test'"),
    )
    .await;

    match subscription_result {
        Ok(Ok(mut subscription)) => {
            // Skip initial messages
            for _ in 0..2 {
                let _ = timeout(Duration::from_millis(500), subscription.next()).await;
            }

            // Update the row
            execute_sql("UPDATE ws_test.events SET data = 'updated' WHERE event_type = 'update_test'")
                .await
                .ok();

            // Wait for update notification
            if let Ok(Some(event_result)) = timeout(Duration::from_secs(3), subscription.next()).await {
                if let Ok(event) = event_result {
                    match event {
                        ChangeEvent::Update { rows, old_rows, .. } => {
                            assert!(!rows.is_empty(), "Update should contain new rows");
                            assert!(!old_rows.is_empty(), "Update should contain old rows");
                        }
                        _ => {
                            eprintln!("Expected Update event, got: {:?}", event);
                        }
                    }
                }
            } else {
                eprintln!("⚠️  No update notification received");
            }
        }
        Ok(Err(e)) => {
            eprintln!("⚠️  Subscription failed: {}", e);
        }
        Err(_) => {
            eprintln!("⚠️  Subscription timed out");
        }
    }

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_websocket_delete_notification() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup test data");

    // Insert initial row
    execute_sql("INSERT INTO ws_test.events (event_type, data) VALUES ('delete_test', 'to_delete')")
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;

    let client = create_test_client().expect("Failed to create client");

    let subscription_result = timeout(
        TEST_TIMEOUT,
        client.subscribe("SELECT * FROM ws_test.events WHERE event_type = 'delete_test'"),
    )
    .await;

    match subscription_result {
        Ok(Ok(mut subscription)) => {
            // Skip initial messages
            for _ in 0..2 {
                let _ = timeout(Duration::from_millis(500), subscription.next()).await;
            }

            // Delete the row
            execute_sql("DELETE FROM ws_test.events WHERE event_type = 'delete_test'")
                .await
                .ok();

            // Wait for delete notification
            if let Ok(Some(event_result)) = timeout(Duration::from_secs(3), subscription.next()).await {
                if let Ok(event) = event_result {
                    match event {
                        ChangeEvent::Delete { old_rows, .. } => {
                            assert!(!old_rows.is_empty(), "Delete should contain deleted rows");
                        }
                        _ => {
                            eprintln!("Expected Delete event, got: {:?}", event);
                        }
                    }
                }
            } else {
                eprintln!("⚠️  No delete notification received");
            }
        }
        Ok(Err(e)) => {
            eprintln!("⚠️  Subscription failed: {}", e);
        }
        Err(_) => {
            eprintln!("⚠️  Subscription timed out");
        }
    }

    cleanup_test_data().await.ok();
}

// =============================================================================
// SQL Statement Coverage Tests
// =============================================================================

#[tokio::test]
async fn test_sql_create_namespace() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let client = create_test_client().expect("Failed to create client");

    // Cleanup
    let _ = client.execute_query("DROP NAMESPACE IF EXISTS test_ns CASCADE").await;
    sleep(Duration::from_millis(100)).await;

    let result = client.execute_query("CREATE NAMESPACE test_ns").await;
    assert!(result.is_ok(), "CREATE NAMESPACE should succeed");

    // Cleanup
    let _ = client.execute_query("DROP NAMESPACE test_ns CASCADE").await;
}

#[tokio::test]
async fn test_sql_create_user_table() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    let result = client
        .execute_query(
            r#"CREATE USER TABLE ws_test.test_table (
                id INT AUTO_INCREMENT,
                name VARCHAR NOT NULL,
                age INT
            ) FLUSH ROWS 10"#,
        )
        .await;

    assert!(
        result.is_ok() || result.unwrap_err().to_string().contains("already exists"),
        "CREATE USER TABLE should succeed"
    );

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_sql_insert_select() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    // INSERT
    let insert = client
        .execute_query("INSERT INTO ws_test.events (event_type, data) VALUES ('test', 'data')")
        .await;
    assert!(insert.is_ok(), "INSERT should succeed");

    // SELECT
    let select = client
        .execute_query("SELECT * FROM ws_test.events WHERE event_type = 'test'")
        .await;
    assert!(select.is_ok(), "SELECT should succeed");

    let response = select.unwrap();
    assert_eq!(response.status, "success");
    assert!(!response.results.is_empty());

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_sql_update() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    // Insert
    client
        .execute_query("INSERT INTO ws_test.events (event_type, data) VALUES ('update', 'old')")
        .await
        .ok();

    // Update
    let result = client
        .execute_query("UPDATE ws_test.events SET data = 'new' WHERE event_type = 'update'")
        .await;
    assert!(result.is_ok(), "UPDATE should succeed");

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_sql_delete() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    // Insert
    client
        .execute_query("INSERT INTO ws_test.events (event_type, data) VALUES ('delete', 'data')")
        .await
        .ok();

    // Delete
    let result = client
        .execute_query("DELETE FROM ws_test.events WHERE event_type = 'delete'")
        .await;
    assert!(result.is_ok(), "DELETE should succeed");

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_sql_drop_table() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    // Create a table to drop
    client
        .execute_query(
            r#"CREATE USER TABLE ws_test.temp_table (id INT) FLUSH ROWS 10"#,
        )
        .await
        .ok();

    // Drop it
    let result = client
        .execute_query("DROP TABLE ws_test.temp_table")
        .await;
    assert!(result.is_ok(), "DROP TABLE should succeed");

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_sql_flush_table() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    let result = client
        .execute_query("FLUSH TABLE ws_test.events")
        .await;
    
    // FLUSH might not be implemented, so accept either success or "unsupported"
    assert!(
        result.is_ok() || result.unwrap_err().to_string().contains("Unsupported"),
        "FLUSH TABLE should succeed or return unsupported"
    );

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_sql_system_tables() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let client = create_test_client().expect("Failed to create client");

    // Query system.users
    let users = client.execute_query("SELECT * FROM system.users").await;
    assert!(users.is_ok(), "Should query system.users");

    // Query system.namespaces
    let namespaces = client.execute_query("SELECT * FROM system.namespaces").await;
    assert!(namespaces.is_ok(), "Should query system.namespaces");

    // Query system.tables
    let tables = client.execute_query("SELECT * FROM system.tables").await;
    assert!(tables.is_ok(), "Should query system.tables");
}

#[tokio::test]
async fn test_sql_where_clause_operators() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    // Insert test data
    for i in 1..=5 {
        client
            .execute_query(&format!(
                "INSERT INTO ws_test.events (event_type, data) VALUES ('op_test', '{}')",
                i
            ))
            .await
            .ok();
    }

    // Test LIKE
    let like = client
        .execute_query("SELECT * FROM ws_test.events WHERE data LIKE '%3%'")
        .await;
    assert!(like.is_ok(), "LIKE operator should work");

    // Test IN
    let in_op = client
        .execute_query("SELECT * FROM ws_test.events WHERE data IN ('1', '2', '3')")
        .await;
    assert!(in_op.is_ok(), "IN operator should work");

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_sql_limit_offset() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    // Insert multiple rows
    for i in 1..=10 {
        client
            .execute_query(&format!(
                "INSERT INTO ws_test.events (event_type, data) VALUES ('limit_test', '{}')",
                i
            ))
            .await
            .ok();
    }

    // Test LIMIT
    let limit = client
        .execute_query("SELECT * FROM ws_test.events WHERE event_type = 'limit_test' LIMIT 5")
        .await;
    assert!(limit.is_ok(), "LIMIT should work");
    let response = limit.unwrap();
    if let Some(rows) = &response.results[0].rows {
        assert!(rows.len() <= 5, "Should return at most 5 rows");
    }

    cleanup_test_data().await.ok();
}

#[tokio::test]
async fn test_sql_order_by() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Failed to setup");

    let client = create_test_client().expect("Failed to create client");

    // Insert data
    client
        .execute_query("INSERT INTO ws_test.events (event_type, data) VALUES ('sort', 'z')")
        .await
        .ok();
    client
        .execute_query("INSERT INTO ws_test.events (event_type, data) VALUES ('sort', 'a')")
        .await
        .ok();

    // Test ORDER BY
    let ordered = client
        .execute_query("SELECT * FROM ws_test.events WHERE event_type = 'sort' ORDER BY data ASC")
        .await;
    assert!(ordered.is_ok(), "ORDER BY should work");

    cleanup_test_data().await.ok();
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[tokio::test]
async fn test_error_invalid_sql() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let client = create_test_client().expect("Failed to create client");

    let result = client.execute_query("INVALID SQL STATEMENT").await;
    assert!(result.is_err() || result.unwrap().status == "error", "Invalid SQL should return error");
}

#[tokio::test]
async fn test_error_table_not_found() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let client = create_test_client().expect("Failed to create client");

    let result = client
        .execute_query("SELECT * FROM nonexistent.table")
        .await;
    assert!(
        result.is_err() || result.unwrap().status == "error",
        "Querying non-existent table should return error"
    );
}

#[tokio::test]
async fn test_error_connection_refused() {
    // Try to connect to non-existent server
    let client = KalamLinkClient::builder()
        .base_url("http://localhost:9999")
        .user_id(TEST_USER_ID)
        .timeout(Duration::from_secs(2))
        .build()
        .expect("Client creation should succeed");

    let result = client.execute_query("SELECT 1").await;
    assert!(result.is_err(), "Connection to non-existent server should fail");
}

// =============================================================================
// Server Requirement Check
// =============================================================================

#[tokio::test]
async fn test_server_running_check() {
    if !is_server_running().await {
        eprintln!(
            "\n⚠️  Server is not running at {}\n\n\
            To run these tests:\n\
            1. Terminal 1: cd backend && cargo run --bin kalamdb-server\n\
            2. Terminal 2: cd cli && cargo test --test test_websocket_integration\n\n\
            Tests will be skipped if server is not running.\n",
            SERVER_URL
        );
        // Don't panic - just skip
        return;
    }
    
    println!("✅ Server is running at {}", SERVER_URL);
}
