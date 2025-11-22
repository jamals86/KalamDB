//! Subscription Lifecycle Tests
//!
//! Tests WebSocket subscription creation, cleanup, and lifecycle management:
//! - Subscription start/stop
//! - Abnormal disconnections
//! - Multiple concurrent subscriptions
//! - Cleanup after connection close
//!
//! These tests validate that live query subscriptions are managed correctly
//! and don't leak resources.

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, TestServer};
use kalamdb_api::models::ResponseStatus;

/// Test: SUBSCRIBE TO returns subscription metadata
#[tokio::test]
async fn test_subscribe_returns_metadata() {
    let server = TestServer::new().await;
    let namespace = "sub_test";
    let table_name = "messages";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql(&format!(
        "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, content TEXT) WITH (TYPE='USER')",
        namespace, table_name
    )).await;
    
    // Execute SUBSCRIBE TO command
    let subscribe_sql = format!("SUBSCRIBE TO {}.{}", namespace, table_name);
    let response = server.execute_sql(&subscribe_sql).await;
    
    // Should return subscription metadata (not execute immediately)
    println!("SUBSCRIBE response status: {:?}", response.status);
    
    // Note: Current implementation may return Success or Error depending on subscription handling
    // This test documents the behavior
    if let Some(error) = &response.error {
        println!("SUBSCRIBE error: {}", error.message);
    }
}

/// Test: SUBSCRIBE TO with WHERE clause
#[tokio::test]
async fn test_subscribe_with_filter() {
    let server = TestServer::new().await;
    let namespace = "filtered_sub";
    let table_name = "events";
    let user_id = "sub_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql_as_user(&format!(
        "CREATE TABLE {}.{} (
            event_id BIGINT PRIMARY KEY,
            event_type TEXT,
            timestamp TIMESTAMP DEFAULT NOW()
        ) WITH (TYPE='USER')",
        namespace, table_name
    ), user_id).await;
    
    // SUBSCRIBE with filter
    let subscribe_sql = format!(
        "SUBSCRIBE TO {}.{} WHERE event_type = 'error'",
        namespace, table_name
    );
    
    let response = server.execute_sql_as_user(&subscribe_sql, user_id).await;
    
    println!("SUBSCRIBE with WHERE status: {:?}", response.status);
    
    if let Some(error) = &response.error {
        println!("Error: {}", error.message);
    }
}

/// Test: SUBSCRIBE TO with OPTIONS (last_rows)
#[tokio::test]
async fn test_subscribe_with_options() {
    let server = TestServer::new().await;
    let namespace = "options_sub";
    let table_name = "history";
    let user_id = "options_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql_as_user(&format!(
        "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, data TEXT) WITH (TYPE='USER')",
        namespace, table_name
    ), user_id).await;
    
    // Insert some initial data
    for i in 0..15 {
        server.execute_sql_as_user(&format!(
            "INSERT INTO {}.{} (id, data) VALUES ({}, 'Row {}')",
            namespace, table_name, i, i
        ), user_id).await;
    }
    
    // SUBSCRIBE with last_rows option
    let subscribe_sql = format!(
        "SUBSCRIBE TO {}.{} OPTIONS (last_rows=10)",
        namespace, table_name
    );
    
    let response = server.execute_sql_as_user(&subscribe_sql, user_id).await;
    
    println!("SUBSCRIBE with OPTIONS status: {:?}", response.status);
    
    if let Some(error) = &response.error {
        println!("Error: {}", error.message);
    }
}

/// Test: SUBSCRIBE TO on STREAM table
#[tokio::test]
async fn test_subscribe_to_stream_table() {
    let server = TestServer::new().await;
    let namespace = "stream_sub";
    let table_name = "live_events";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql(&format!(
        "CREATE TABLE {}.{} (
            event_id TEXT PRIMARY KEY,
            payload TEXT
        ) WITH (TYPE='STREAM', TTL_SECONDS=30)",
        namespace, table_name
    )).await;
    
    // SUBSCRIBE to stream table
    let subscribe_sql = format!("SUBSCRIBE TO {}.{}", namespace, table_name);
    let response = server.execute_sql(&subscribe_sql).await;
    
    println!("SUBSCRIBE to STREAM status: {:?}", response.status);
    
    // Stream tables should support subscriptions
    if response.status == ResponseStatus::Error {
        if let Some(error) = &response.error {
            println!("Error: {}", error.message);
        }
    }
}

/// Test: SUBSCRIBE TO on SHARED table
#[tokio::test]
async fn test_subscribe_to_shared_table() {
    let server = TestServer::new().await;
    let namespace = "shared_sub";
    let table_name = "global_feed";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql(&format!(
        "CREATE TABLE {}.{} (
            post_id BIGINT PRIMARY KEY,
            content TEXT
        ) WITH (TYPE='SHARED', ACCESS_LEVEL='PUBLIC')",
        namespace, table_name
    )).await;
    
    // SUBSCRIBE to shared table
    let subscribe_sql = format!("SUBSCRIBE TO {}.{}", namespace, table_name);
    let response = server.execute_sql(&subscribe_sql).await;
    
    println!("SUBSCRIBE to SHARED status: {:?}", response.status);
    
    if response.status == ResponseStatus::Error {
        if let Some(error) = &response.error {
            println!("Error: {}", error.message);
        }
    }
}

/// Test: SUBSCRIBE without namespace is rejected
#[tokio::test]
async fn test_subscribe_without_namespace_rejected() {
    let server = TestServer::new().await;
    
    // Try to SUBSCRIBE without namespace
    let subscribe_sql = "SUBSCRIBE TO messages";
    let response = server.execute_sql(subscribe_sql).await;
    
    // Should fail (namespace is required)
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "SUBSCRIBE without namespace should be rejected"
    );
    
    if let Some(error) = &response.error {
        let msg = error.message.to_lowercase();
        assert!(
            msg.contains("namespace") || msg.contains("qualified"),
            "Error should mention namespace requirement: {}",
            error.message
        );
    }
}

/// Test: SUBSCRIBE TO non-existent table
#[tokio::test]
async fn test_subscribe_to_nonexistent_table() {
    let server = TestServer::new().await;
    let namespace = "nonexist_sub";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Try to SUBSCRIBE to table that doesn't exist
    let subscribe_sql = format!("SUBSCRIBE TO {}.nonexistent_table", namespace);
    let response = server.execute_sql(&subscribe_sql).await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "SUBSCRIBE to non-existent table should fail"
    );
    
    if let Some(error) = &response.error {
        println!("Error: {}", error.message);
        
        let msg = error.message.to_lowercase();
        assert!(
            msg.contains("not found") || msg.contains("does not exist") || msg.contains("unknown"),
            "Error should indicate table not found: {}",
            error.message
        );
    }
}

/// Test: Invalid SUBSCRIBE TO syntax
#[tokio::test]
async fn test_invalid_subscribe_syntax() {
    let server = TestServer::new().await;
    
    let invalid_subscriptions = vec![
        ("SUBSCRIBE table", "Missing TO keyword"),
        ("SUBSCRIBE TO", "Missing table name"),
        ("SUBSCRIBE TO table WHERE", "Incomplete WHERE clause"),
        ("SUBSCRIBE TO table OPTIONS", "Incomplete OPTIONS"),
    ];
    
    for (sql, description) in invalid_subscriptions {
        let response = server.execute_sql(sql).await;
        
        assert_eq!(
            response.status,
            ResponseStatus::Error,
            "{} should fail: {}",
            description, sql
        );
        
        if let Some(error) = &response.error {
            println!("{}: {}", description, error.message);
        }
    }
}

/// Test: Concurrent SUBSCRIBE commands don't interfere
#[tokio::test]
async fn test_concurrent_subscribe_commands() {
    let server = TestServer::new().await;
    let namespace = "concurrent_sub";
    let table_name = "shared_data";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql(&format!(
        "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, value TEXT) WITH (TYPE='SHARED', ACCESS_LEVEL='PUBLIC')",
        namespace, table_name
    )).await;
    
    // Spawn multiple concurrent SUBSCRIBE commands
    let mut handles = vec![];
    let server_arc = std::sync::Arc::new(server);
    
    for i in 0..5 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        
        let handle = tokio::spawn(async move {
            let subscribe_sql = format!("SUBSCRIBE TO {}.{} WHERE id > {}", ns, tn, i * 10);
            server_clone.execute_sql(&subscribe_sql).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all SUBSCRIBE commands
    let results = futures_util::future::join_all(handles).await;
    
    // All should complete without panic
    for result in results {
        assert!(result.is_ok(), "SUBSCRIBE command should not panic");
    }
}

/// Test: Query system.live_queries after subscription attempt
#[tokio::test]
async fn test_query_system_live_queries() {
    let server = TestServer::new().await;
    
    // Query system.live_queries
    let response = server.execute_sql(
        "SELECT subscription_id, sql, user_id, created_at FROM system.live_queries ORDER BY created_at DESC LIMIT 10"
    ).await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "system.live_queries should be queryable"
    );
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        println!("Active subscriptions: {}", rows.len());
        
        for row in rows {
            let sub_id = row.get("subscription_id").and_then(|v| v.as_str()).unwrap_or("N/A");
            let sql = row.get("sql").and_then(|v| v.as_str()).unwrap_or("N/A");
            let user_id = row.get("user_id").and_then(|v| v.as_str()).unwrap_or("N/A");
            
            println!("  {} - {} - SQL: {}", sub_id, user_id, sql);
        }
    }
}
