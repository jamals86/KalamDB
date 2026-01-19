//! Integration tests for edge cases in authentication and authorization
//!
//! Tests:
//! - Empty credentials
//! - Malformed Basic Auth headers
//! - Concurrent authentication requests
//! - Deleted user authentication
//! - Role changes during session
//! - Maximum password length
//! - Shared table default access levels

use super::test_support::TestServer;
use base64::{engine::general_purpose, Engine as _};
use kalamdb_auth::{authenticate, AuthRequest};
use kalamdb_commons::{models::ConnectionInfo, Role, UserId};
use std::time::{SystemTime, UNIX_EPOCH};

/// T143A: Test authentication with empty credentials returns error
#[tokio::test]
async fn test_empty_credentials_401() {
    let server = TestServer::new_shared().await;
    let user_repo = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // Test empty username
    let credentials = general_purpose::STANDARD.encode(":");
    let auth_header = format!("Basic {}", credentials);
    let auth_request = AuthRequest::Header(auth_header);
    let result = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result.is_err(), "Empty credentials should fail");

    // Test only username, no password
    let credentials = general_purpose::STANDARD.encode("user:");
    let auth_header = format!("Basic {}", credentials);
    let auth_request = AuthRequest::Header(auth_header);
    let result = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result.is_err(), "Username without password should fail");
}

/// T143B: Test malformed Basic Auth header returns error
#[tokio::test]
async fn test_malformed_basic_auth_400() {
    let server = TestServer::new_shared().await;
    let user_repo = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // Test invalid base64
    let auth_header = "Basic INVALID_BASE64!!!";
    let auth_request = AuthRequest::Header(auth_header.to_string());
    let result = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result.is_err(), "Malformed base64 should fail");

    // Test missing "Basic " prefix
    let credentials = general_purpose::STANDARD.encode("user:pass");
    let auth_request = AuthRequest::Header(credentials);
    let result = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result.is_err(), "Missing Basic prefix should fail");

    // Test Bearer without JWT/OAuth token
    let auth_header = "Bearer";
    let auth_request = AuthRequest::Header(auth_header.to_string());
    let result = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result.is_err(), "Bearer without token should fail");
}

/// T143C: Test concurrent authentication requests have no race conditions
#[tokio::test]
async fn test_concurrent_auth_no_race_conditions() {
    let server = TestServer::new_shared().await;
    let username = format!(
        "concurrent_user_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX_EPOCH")
            .as_nanos()
    );

    // Create test user
    server.create_user(&username, "TestPass123", Role::User).await;

    let user_repo = server.users_repo();

    // Spawn 10 concurrent authentication requests
    let mut handles = vec![];
    for i in 0..10 {
        let user_repo = user_repo.clone();
        let username = username.clone();

        let handle = tokio::spawn(async move {
            let connection_info = ConnectionInfo::new(Some(format!("127.0.0.1:{}", 8080 + i)));
            let credentials = general_purpose::STANDARD.encode(format!("{}:TestPass123", username));
            let auth_header = format!("Basic {}", credentials);
            let auth_request = AuthRequest::Header(auth_header);
            authenticate(auth_request, &connection_info, &user_repo).await
        });

        handles.push(handle);
    }

    // Wait for all requests to complete
    let results = futures_util::future::join_all(handles).await;

    // All should succeed
    let mut success_count = 0;
    for result in results {
        if let Ok(Ok(_)) = result {
            success_count += 1;
        }
    }

    assert_eq!(success_count, 10, "All concurrent auth requests should succeed");
}

/// T143D: Test deleted user authentication is denied
#[tokio::test]
async fn test_deleted_user_denied() {
    let server = TestServer::new_shared().await;

    // Create user
    server.create_user("deleted_user", "TestPass123", Role::User).await;

    // Soft delete the user via provider
    let users_provider = server.app_context.system_tables().users();
    users_provider
        .delete_user(&UserId::new("deleted_user"))
        .expect("Failed to delete user");

    let user_repo = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // Try to authenticate with deleted user
    let credentials = general_purpose::STANDARD.encode("deleted_user:TestPass123");
    let auth_header = format!("Basic {}", credentials);
    let auth_request = AuthRequest::Header(auth_header);
    let result = authenticate(auth_request, &connection_info, &user_repo).await;

    assert!(result.is_err(), "Deleted user authentication should fail");
    let err = result.err().unwrap();
    let err_msg = format!("{:?}", err);
    // The unified auth returns generic "Invalid username or password" for security
    // (doesn't reveal whether user exists or is deleted)
    assert!(
        err_msg.contains("UserDeleted")
            || err_msg.contains("deleted")
            || err_msg.contains("Invalid"),
        "Error should indicate auth failure: {}",
        err_msg
    );
}

/// T143E: Test role change applies to next request (not during session)
#[tokio::test]
async fn test_role_change_applies_next_request() {
    let server = TestServer::new_shared().await;

    // Create user with User role
    server.create_user("role_change_user", "TestPass123", Role::User).await;

    let user_repo = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // First authentication
    let credentials = general_purpose::STANDARD.encode("role_change_user:TestPass123");
    let auth_header = format!("Basic {}", credentials);
    let auth_request = AuthRequest::Header(auth_header.clone());
    let result1 = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result1.is_ok());
    let user1 = result1.unwrap();
    assert_eq!(user1.user.role, Role::User);

    // Change user role to DBA
    let users_provider = server.app_context.system_tables().users();
    let mut user = users_provider
        .get_user_by_id(&UserId::new("role_change_user"))
        .unwrap()
        .unwrap();
    user.role = Role::Dba;
    users_provider.update_user(user).expect("Failed to update user");

    // Second authentication should reflect new role
    // (unified auth reads directly from repo, no caching)
    let auth_request = AuthRequest::Header(auth_header);
    let result2 = authenticate(auth_request, &connection_info, &user_repo).await;
    assert!(result2.is_ok());
    let user2 = result2.unwrap();
    assert_eq!(user2.user.role, Role::Dba, "Role should be updated to DBA");
}

/// T143F: Test maximum password length handling
#[tokio::test]
async fn test_maximum_password_length() {
    let server = TestServer::new_shared().await;

    // bcrypt has a maximum of 72 bytes
    let max_password = "A".repeat(72);

    // Create user with maximum length password
    server.create_user("max_pass_user", &max_password, Role::User).await;

    let user_repo = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // Try to authenticate
    let credentials = general_purpose::STANDARD.encode(format!("max_pass_user:{}", max_password));
    let auth_header = format!("Basic {}", credentials);
    let auth_request = AuthRequest::Header(auth_header);
    let result = authenticate(auth_request, &connection_info, &user_repo).await;

    assert!(
        result.is_ok(),
        "Authentication with maximum length password should succeed: {:?}",
        result.err()
    );
}
