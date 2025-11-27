//! Integration tests for Live Query Auth Expiry
//!
//! Verifies that auth expiry events correctly terminate WebSocket connections.

#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_commons::models::{ConnectionId, ConnectionInfo, UserId};
use std::time::Duration;

#[tokio::test]
async fn test_live_query_auth_expiry() {
    let server = TestServer::new().await;
    let manager = server.app_context.live_query_manager();
    let registry = manager.registry();

    // 1. Setup User and Connection
    let user_id_str = "user_expiry_test";
    let conn_id_str = "conn_expiry_test";
    let connection_id = ConnectionId::new(conn_id_str.to_string());
    let user_id = UserId::new(user_id_str.to_string());
    
    // 2. Register Connection using ConnectionsManager
    let conn_info = ConnectionInfo::new(None);
    let registration = registry.register_connection(connection_id.clone(), conn_info)
        .expect("Failed to register connection");
    let connection_state = registration.state;
    let mut rx = registration.notification_rx;
    
    // Authenticate the connection
    connection_state.write().mark_authenticated(user_id.clone());
    registry.on_authenticated(&connection_id, user_id.clone());
    
    println!("✓ Connection registered and authenticated");

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
