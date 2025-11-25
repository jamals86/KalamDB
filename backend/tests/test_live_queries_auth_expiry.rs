//! Integration tests for Live Query Auth Expiry
//!
//! Verifies that auth expiry events correctly terminate WebSocket connections.

#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_commons::models::{ConnectionId, LiveId};
use kalamdb_commons::Notification;
use tokio::sync::mpsc;
use std::time::Duration;

#[tokio::test]
async fn test_live_query_auth_expiry() {
    let server = TestServer::new().await;
    let manager = server.app_context.live_query_manager();

    // 1. Setup User and Connection
    let user_id_str = "user_expiry_test";
    let conn_id_str = "conn_expiry_test";
    let connection_id = ConnectionId::new(conn_id_str.to_string());
    
    // Create channel for notifications
    let (tx, mut rx) = mpsc::unbounded_channel::<(LiveId, Notification)>();
    
    // 2. Register Connection
    let user_id = kalamdb_commons::models::UserId::new(user_id_str.to_string());
    
    manager.register_connection(
        user_id.clone(), 
        connection_id.to_string(), 
        Some(tx)
    )
        .await
        .expect("Failed to register connection");
    
    println!("✓ Connection registered");

    // 3. Trigger Auth Expiry
    manager.handle_auth_expiry(&connection_id)
        .await
        .expect("Failed to handle auth expiry");
    
    println!("✓ Auth expiry handled");

    // 4. Verify Connection Closed
    // The receiver should return None when the sender is dropped.
    let result = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    
    match result {
        Ok(None) => println!("✓ Connection closed successfully (channel dropped)"),
        Ok(Some(msg)) => panic!("Received unexpected message: {:?}", msg),
        Err(_) => panic!("Timed out waiting for connection close - sender was not dropped"),
    }
}
