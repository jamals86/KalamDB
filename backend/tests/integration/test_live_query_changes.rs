//! Integration tests for Live Query Change Detection (Phase 6 - T195-T205)
//!
//! Tests comprehensive WebSocket subscription functionality:
//! - INSERT/UPDATE/DELETE detection in real-time
//! - Concurrent operations without message loss
//! - Multiple subscription isolation
//! - High-frequency change delivery
//! - Connection resilience and recovery
//!
//! Uses WebSocket test utilities to validate end-to-end live query behavior.

mod common;

use common::{create_test_jwt, fixtures, websocket::WebSocketClient, start_http_server_for_websocket_tests, TestServer};
use std::time::Duration;
use tokio::time::sleep;

/// Helper to setup HTTP server and test table for WebSocket tests
async fn setup_http_server_and_table(
    namespace: &str,
    table_name: &str,
    user_id: &str,
) -> (TestServer, String) {
    let (server, base_url) = start_http_server_for_websocket_tests().await;
    
    // Create namespace
    fixtures::create_namespace(&server, namespace).await;

    // Create user table with appropriate schema
    let create_table = format!(
        r#"CREATE USER TABLE {}.{} (
            id TEXT,
            content TEXT,
            priority INT,
            created_at BIGINT
        ) STORAGE local"#,
        namespace, table_name
    );

    let response = server.execute_sql_as_user(&create_table, user_id).await;
    assert_eq!(
        response.status, "success",
        "Failed to create table: {:?}",
        response.error
    );
    
    (server, base_url)
}

/// Helper to create a test namespace and user table for live query testing
async fn setup_test_table(server: &TestServer, namespace: &str, table_name: &str, user_id: &str) {
    // Create namespace
    fixtures::create_namespace(server, namespace).await;

    // Create user table with appropriate schema
    let create_table = format!(
        r#"CREATE USER TABLE {}.{} (
            id TEXT,
            content TEXT,
            priority INT,
            created_at BIGINT
        ) STORAGE local"#,
        namespace, table_name
    );

    let response = server.execute_sql_as_user(&create_table, user_id).await;
    assert_eq!(
        response.status, "success",
        "Failed to create table: {:?}",
        response.error
    );
}

/// T196: Test live query detects inserts
///
/// Subscribe to a user table, INSERT 100 rows, verify 100 notifications received.
/// This validates the basic INSERT detection mechanism.
#[tokio::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_live_query_detects_inserts() {
    let (server, base_url) = setup_http_server_and_table("test_ns", "messages", "user1").await;
    let ws_url = format!("{}/v1/ws", base_url.replace("http", "ws"));
    
    // Create JWT token for authentication
    let token = create_test_jwt("user1", "kalamdb-dev-secret-key-change-in-production", 3600);

    // Connect WebSocket with authentication and subscribe
    let mut ws = WebSocketClient::connect_with_auth(&ws_url, Some(&token)).await.unwrap();
    ws.subscribe(
        "messages",
        "SELECT * FROM test_ns.messages WHERE created_at > 0",
    )
    .await
    .unwrap();

    // Insert 100 rows
    for i in 0..100 {
        let insert_sql = format!(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg{}', 'Content {}', {}, {})"#,
            i,
            i,
            i % 10,
            chrono::Utc::now().timestamp_millis()
        );
        server.execute_sql_as_user(&insert_sql, "user1").await;
    }

    // Receive notifications with timeout
    ws.receive_notifications(Duration::from_secs(2)).await.unwrap();

    // Verify 100 notifications received
    let notifications = ws.get_notifications();
    assert_eq!(
        notifications.len(),
        100,
        "Expected 100 INSERT notifications"
    );

    // Verify all notifications are INSERT type
    for notification in notifications {
        assert_eq!(notification.change_type, "INSERT");
        assert!(notification.data.get("id").is_some());
        assert!(notification.data.get("content").is_some());
    }

    ws.disconnect().await.unwrap();
}

/// T197: Test live query detects updates
///
/// Subscribe, INSERT + UPDATE rows, verify notifications contain old/new values.
/// This validates UPDATE detection with old value tracking.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_live_query_detects_updates() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    let mut ws = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    ws.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // Insert initial row
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg1', 'Original', 5, 1000)"#,
            "user1",
        )
        .await;

    sleep(Duration::from_millis(100)).await;

    // Update the row
    server
        .execute_sql_as_user(
            r#"UPDATE test_ns.messages SET content = 'Updated', priority = 10 WHERE id = 'msg1'"#,
            "user1",
        )
        .await;

    sleep(Duration::from_millis(100)).await;

    let notifications = ws.get_notifications();
    assert!(notifications.len() >= 2, "Expected INSERT and UPDATE notifications");

    // Find the UPDATE notification
    let update_notif = notifications
        .iter()
        .find(|n| n.change_type == "UPDATE")
        .expect("Should have UPDATE notification");

    // Verify new values
    assert_eq!(update_notif.data.get("content").unwrap(), "Updated");
    assert_eq!(update_notif.data.get("priority").unwrap(), &10);

    // Verify old values if available
    if let Some(old_values) = &update_notif.old_values {
        assert_eq!(old_values.get("content").unwrap(), "Original");
        assert_eq!(old_values.get("priority").unwrap(), &5);
    }

    ws.disconnect().await.unwrap();
}

/// T198: Test live query detects deletes
///
/// Subscribe, INSERT + DELETE rows, verify _deleted flag in notifications.
/// This validates soft-delete detection.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_live_query_detects_deletes() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    let mut ws = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    ws.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // Insert a row
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg1', 'To be deleted', 1, 1000)"#,
            "user1",
        )
        .await;

    sleep(Duration::from_millis(100)).await;

    // Delete the row (soft delete)
    server
        .execute_sql_as_user(
            r#"DELETE FROM test_ns.messages WHERE id = 'msg1'"#,
            "user1",
        )
        .await;

    sleep(Duration::from_millis(100)).await;

    let notifications = ws.get_notifications();
    assert!(notifications.len() >= 2, "Expected INSERT and DELETE notifications");

    // Find the DELETE notification
    let delete_notif = notifications
        .iter()
        .find(|n| n.change_type == "DELETE")
        .expect("Should have DELETE notification");

    // Verify _deleted flag is set
    assert_eq!(
        delete_notif.data.get("_deleted").unwrap(),
        &true,
        "_deleted flag should be true"
    );
    assert_eq!(delete_notif.data.get("id").unwrap(), "msg1");

    ws.disconnect().await.unwrap();
}

/// T199: Test concurrent writers without message loss
///
/// Spawn 5 concurrent writers inserting rows simultaneously.
/// Verify all notifications are received without loss or duplication.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_concurrent_writers_no_message_loss() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    let mut ws = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    ws.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // Spawn 5 concurrent writers, each writing 20 rows
    let mut handles = vec![];
    for writer_id in 0..5 {
        let server_clone = server.clone();
        let handle = tokio::spawn(async move {
            for i in 0..20 {
                let insert_sql = format!(
                    r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
                       VALUES ('writer{}_msg{}', 'Writer {} Row {}', {}, {})"#,
                    writer_id,
                    i,
                    writer_id,
                    i,
                    i % 10,
                    chrono::Utc::now().timestamp_millis()
                );
                server_clone.execute_sql_as_user(&insert_sql, "user1").await;
            }
        });
        handles.push(handle);
    }

    // Wait for all writers to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Wait for all notifications to arrive
    sleep(Duration::from_secs(1)).await;

    let notifications = ws.get_notifications();

    // Verify we received exactly 100 notifications (5 writers * 20 rows)
    assert_eq!(
        notifications.len(),
        100,
        "Should receive all 100 notifications without loss"
    );

    // Verify no duplicates by checking unique IDs
    let mut unique_ids = std::collections::HashSet::new();
    for notification in notifications {
        let id = notification.data.get("id").unwrap().as_str().unwrap();
        assert!(
            unique_ids.insert(id.to_string()),
            "Duplicate notification for ID: {}",
            id
        );
    }

    ws.disconnect().await.unwrap();
}

/// T200: Test AI message scenario
///
/// Simulate an AI agent writing messages while a human client is subscribed.
/// Verify the human client receives all notifications in real-time.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_ai_message_scenario() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    // Human client subscribes
    let mut human_ws = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    human_ws
        .subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // AI agent writes messages (simulated as rapid inserts)
    for i in 0..50 {
        let insert_sql = format!(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('ai_msg{}', 'AI Response {}', 1, {})"#,
            i,
            i,
            chrono::Utc::now().timestamp_millis()
        );
        server.execute_sql_as_user(&insert_sql, "user1").await;

        // Small delay to simulate realistic AI response timing
        sleep(Duration::from_millis(10)).await;
    }

    // Wait for notifications to propagate
    sleep(Duration::from_millis(500)).await;

    let notifications = human_ws.get_notifications();

    // Verify human received all 50 AI messages
    assert_eq!(
        notifications.len(),
        50,
        "Human client should receive all AI messages"
    );

    // Verify chronological ordering
    let mut prev_id = -1;
    for notification in notifications {
        let id_str = notification.data.get("id").unwrap().as_str().unwrap();
        let id_num: i32 = id_str.replace("ai_msg", "").parse().unwrap();
        assert!(
            id_num > prev_id,
            "Messages should be in chronological order"
        );
        prev_id = id_num;
    }

    human_ws.disconnect().await.unwrap();
}

/// T201: Test mixed operations ordering
///
/// Execute INSERT, UPDATE, DELETE in sequence.
/// Verify notifications arrive in chronological order.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_mixed_operations_ordering() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    let mut ws = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    ws.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // INSERT
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg1', 'First', 1, 1000)"#,
            "user1",
        )
        .await;

    sleep(Duration::from_millis(50)).await;

    // UPDATE
    server
        .execute_sql_as_user(
            r#"UPDATE test_ns.messages SET content = 'Updated' WHERE id = 'msg1'"#,
            "user1",
        )
        .await;

    sleep(Duration::from_millis(50)).await;

    // DELETE
    server
        .execute_sql_as_user(
            r#"DELETE FROM test_ns.messages WHERE id = 'msg1'"#,
            "user1",
        )
        .await;

    sleep(Duration::from_millis(50)).await;

    let notifications = ws.get_notifications();

    // Verify we have at least 3 notifications
    assert!(
        notifications.len() >= 3,
        "Should have INSERT, UPDATE, DELETE notifications"
    );

    // Verify operation order
    assert_eq!(notifications[0].change_type, "INSERT");
    assert_eq!(notifications[1].change_type, "UPDATE");
    assert_eq!(notifications[2].change_type, "DELETE");

    ws.disconnect().await.unwrap();
}

/// T202: Test changes counter accuracy
///
/// Trigger 50 changes, verify system.live_queries changes counter = 50.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_changes_counter_accuracy() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    let mut ws = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    ws.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // Trigger 50 changes
    for i in 0..50 {
        let insert_sql = format!(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg{}', 'Content {}', {}, {})"#,
            i,
            i,
            i % 10,
            chrono::Utc::now().timestamp_millis()
        );
        server.execute_sql_as_user(&insert_sql, "user1").await;
    }

    sleep(Duration::from_millis(500)).await;

    // Query system.live_queries to check changes counter
    let response = server
        .execute_sql("SELECT live_id, changes FROM system.live_queries WHERE user_id = 'user1'")
        .await;

    assert_eq!(response.status, "success");
    assert!(!response.results.is_empty());

    let row = &response.results[0].rows.as_ref().unwrap()[0];
    let changes = row.get("changes").unwrap().as_i64().unwrap();

    assert_eq!(changes, 50, "Changes counter should be 50");

    ws.disconnect().await.unwrap();
}

/// T203: Test multiple listeners on same table
///
/// Create 3 separate subscriptions, verify each receives independent notifications.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_multiple_listeners_same_table() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    // Create 3 WebSocket connections
    let mut ws1 = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    let mut ws2 = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    let mut ws3 = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();

    // All subscribe to the same table
    ws1.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();
    ws2.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();
    ws3.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // Insert 10 rows
    for i in 0..10 {
        let insert_sql = format!(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg{}', 'Content {}', {}, {})"#,
            i,
            i,
            i % 10,
            chrono::Utc::now().timestamp_millis()
        );
        server.execute_sql_as_user(&insert_sql, "user1").await;
    }

    sleep(Duration::from_millis(500)).await;

    // Verify each client received all 10 notifications independently
    assert_eq!(
        ws1.get_notifications().len(),
        10,
        "Client 1 should receive all notifications"
    );
    assert_eq!(
        ws2.get_notifications().len(),
        10,
        "Client 2 should receive all notifications"
    );
    assert_eq!(
        ws3.get_notifications().len(),
        10,
        "Client 3 should receive all notifications"
    );

    ws1.disconnect().await.unwrap();
    ws2.disconnect().await.unwrap();
    ws3.disconnect().await.unwrap();
}

/// T204: Test listener reconnect without data loss
///
/// Subscribe, disconnect, reconnect, verify subscription continues.
/// Note: This tests reconnection behavior, not message recovery during disconnection.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_listener_reconnect_no_data_loss() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    // Initial connection and subscription
    let mut ws = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    ws.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // Insert some data
    for i in 0..10 {
        let insert_sql = format!(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg{}', 'Content {}', {}, {})"#,
            i,
            i,
            i % 10,
            chrono::Utc::now().timestamp_millis()
        );
        server.execute_sql_as_user(&insert_sql, "user1").await;
    }

    sleep(Duration::from_millis(200)).await;
    assert_eq!(ws.get_notifications().len(), 10);

    // Disconnect
    ws.disconnect().await.unwrap();

    // Reconnect with new subscription
    let mut ws2 = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    ws2.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // Insert more data
    for i in 10..20 {
        let insert_sql = format!(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg{}', 'Content {}', {}, {})"#,
            i,
            i,
            i % 10,
            chrono::Utc::now().timestamp_millis()
        );
        server.execute_sql_as_user(&insert_sql, "user1").await;
    }

    sleep(Duration::from_millis(200)).await;

    // Verify new subscription received new data
    assert_eq!(
        ws2.get_notifications().len(),
        10,
        "New subscription should receive new data"
    );

    ws2.disconnect().await.unwrap();
}

/// T205: Test high-frequency changes
///
/// INSERT 1000 rows rapidly, verify all 1000 notifications are delivered.
/// This tests the system's ability to handle high-throughput scenarios.
#[actix_web::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_high_frequency_changes() {
    let server = TestServer::new().await;
    setup_test_table(&server, "test_ns", "messages", "user1").await;

    let mut ws = WebSocketClient::connect("ws://localhost:8080/ws").await.unwrap();
    ws.subscribe("messages", "SELECT * FROM test_ns.messages")
        .await
        .unwrap();

    // Insert 1000 rows as fast as possible
    for i in 0..1000 {
        let insert_sql = format!(
            r#"INSERT INTO test_ns.messages (id, content, priority, created_at)
               VALUES ('msg{}', 'Content {}', {}, {})"#,
            i,
            i,
            i % 10,
            chrono::Utc::now().timestamp_millis()
        );
        server.execute_sql_as_user(&insert_sql, "user1").await;
    }

    // Wait for all notifications to arrive (may take a bit longer for 1000)
    sleep(Duration::from_secs(2)).await;

    let notifications = ws.get_notifications();

    // Verify all 1000 notifications received
    assert_eq!(
        notifications.len(),
        1000,
        "Should receive all 1000 notifications even at high frequency"
    );

    // Verify no duplicates
    let mut unique_ids = std::collections::HashSet::new();
    for notification in notifications {
        let id = notification.data.get("id").unwrap().as_str().unwrap();
        assert!(
            unique_ids.insert(id.to_string()),
            "Found duplicate notification"
        );
    }

    ws.disconnect().await.unwrap();
}

#[cfg(test)]
mod common_tests {
    use super::*;

    /// Test the common test utilities themselves
    #[actix_web::test]
    async fn test_test_server_creation() {
        let server = TestServer::new().await;
        assert!(!server.db_path().is_empty());
    }

    /// Test helper function for setting up test tables
    #[actix_web::test]
    async fn test_setup_test_table() {
        let server = TestServer::new().await;
        setup_test_table(&server, "test_ns", "test_table", "user1").await;
        assert!(server.table_exists("test_ns", "test_table").await);
    }
}
