//! Tests for last_seen tracking.
//!
//! NOTE: last_seen tracking is not part of the unified authentication module.
//! The stateless authenticate() function returns authentication results without
//! side effects. last_seen updates would need to be handled at the handler level.
//!
//! These tests verify basic authentication behavior and are placeholders for
//! future last_seen implementation at the HTTP/WebSocket handler level.

#[path = "../common/mod.rs"]
mod common;

use base64::{engine::general_purpose, Engine as _};
use common::TestServer;
use kalamdb_auth::{authenticate, AuthRequest};
use kalamdb_commons::{Role, models::ConnectionInfo};

fn basic_auth_header(username: &str, password: &str) -> String {
    let credentials = format!("{}:{}", username, password);
    let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());
    format!("Basic {}", encoded)
}

#[tokio::test]
async fn test_authentication_returns_user() {
    let server = TestServer::new().await;
    let user_repo = server.users_repo();

    let username = "auth_user";
    let password = "StrongPass1!";

    // Create user
    server.create_user(username, password, Role::User).await;

    let auth_header = basic_auth_header(username, password);
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));
    let auth_request = AuthRequest::Header(auth_header);

    let result = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result.is_ok(), "Authentication should succeed");
    
    let auth_result = result.unwrap();
    assert_eq!(auth_result.user.username, username);
    assert_eq!(auth_result.user.role, Role::User);
}

#[tokio::test]
async fn test_multiple_authentications_succeed() {
    let server = TestServer::new().await;
    let user_repo = server.users_repo();

    let username = "multi_auth_user";
    let password = "StrongPass1!";

    // Create user
    server.create_user(username, password, Role::User).await;

    let auth_header = basic_auth_header(username, password);
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // First authentication
    let auth_request = AuthRequest::Header(auth_header.clone());
    let result1 = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result1.is_ok(), "First authentication should succeed");

    // Second authentication (same user, same credentials)
    let auth_request = AuthRequest::Header(auth_header);
    let result2 = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result2.is_ok(), "Second authentication should succeed");
    
    // Both should return the same user
    let user1 = result1.unwrap();
    let user2 = result2.unwrap();
    assert_eq!(user1.user.username, user2.user.username);
}
