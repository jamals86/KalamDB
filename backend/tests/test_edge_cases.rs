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

#[path = "integration/common/mod.rs"]
mod common;

use base64::{engine::general_purpose, Engine as _};
use common::TestServer;
use kalamdb_auth::connection::ConnectionInfo;
use kalamdb_commons::{Role, UserId};
use std::sync::Arc;

/// T143A: Test authentication with empty credentials returns 401
#[tokio::test]
async fn test_empty_credentials_401() {
    let server = TestServer::new().await;
    let auth_service = server.auth_service();
    let adapter = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // Test empty username
    let credentials = general_purpose::STANDARD.encode(":");
    let auth_header = format!("Basic {}", credentials);
    let result = auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await;
    assert!(result.is_err(), "Empty credentials should fail");

    // Test only username, no password
    let credentials = general_purpose::STANDARD.encode("user:");
    let auth_header = format!("Basic {}", credentials);
    let result = auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await;
    assert!(result.is_err(), "Username without password should fail");
}

/// T143B: Test malformed Basic Auth header returns 400
#[tokio::test]
async fn test_malformed_basic_auth_400() {
    let server = TestServer::new().await;
    let auth_service = server.auth_service();
    let adapter = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // Test invalid base64
    let auth_header = "Basic INVALID_BASE64!!!";
    let result = auth_service
        .authenticate_with_repo(auth_header, &connection_info, &adapter)
        .await;
    assert!(result.is_err(), "Malformed base64 should fail");

    // Test missing "Basic " prefix
    let credentials = general_purpose::STANDARD.encode("user:pass");
    let result = auth_service
        .authenticate_with_repo(&credentials, &connection_info, &adapter)
        .await;
    assert!(result.is_err(), "Missing Basic prefix should fail");

    // Test Bearer without JWT/OAuth token
    let auth_header = "Bearer";
    let result = auth_service
        .authenticate_with_repo(auth_header, &connection_info, &adapter)
        .await;
    assert!(result.is_err(), "Bearer without token should fail");
}

/// T143C: Test concurrent authentication requests have no race conditions
#[tokio::test]
async fn test_concurrent_auth_no_race_conditions() {
    let server = TestServer::new().await;
    
    // Create test user
    server.create_user("concurrent_user", "TestPass123", Role::User).await;

    let auth_service = server.auth_service();
    let adapter = server.users_repo();

    // Spawn 10 concurrent authentication requests
    let mut handles = vec![];
    for i in 0..10 {
        let auth_service = auth_service.clone();
        let adapter = adapter.clone();

        let handle = tokio::spawn(async move {
            let connection_info = ConnectionInfo::new(Some(format!("127.0.0.1:{}", 8080 + i)));
            let credentials = general_purpose::STANDARD.encode("concurrent_user:TestPass123");
            let auth_header = format!("Basic {}", credentials);
            auth_service
                .authenticate_with_repo(&auth_header, &connection_info, &adapter)
                .await
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

    assert_eq!(
        success_count, 10,
        "All concurrent auth requests should succeed"
    );
}

/// T143D: Test deleted user authentication is denied
#[tokio::test]
async fn test_deleted_user_denied() {
    let server = TestServer::new().await;

    // Create user
    server.create_user("deleted_user", "TestPass123", Role::User).await;

    // Soft delete the user via provider
    let users_provider = server.app_context.system_tables().users();
    users_provider.delete_user(&UserId::new("deleted_user")).expect("Failed to delete user");

    let auth_service = server.auth_service();
    let adapter = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // Try to authenticate with deleted user
    let credentials = general_purpose::STANDARD.encode("deleted_user:TestPass123");
    let auth_header = format!("Basic {}", credentials);
    let result = auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await;

    assert!(result.is_err(), "Deleted user authentication should fail");
    let err = result.err().unwrap();
    let err_msg = format!("{:?}", err);
    assert!(
        err_msg.contains("UserDeleted") || err_msg.contains("deleted"),
        "Error should indicate user is deleted: {}",
        err_msg
    );
}

/// T143E: Test role change applies to next request (not during session)
#[tokio::test]
async fn test_role_change_applies_next_request() {
    let server = TestServer::new().await;

    // Create user with User role
    server.create_user("role_change_user", "TestPass123", Role::User).await;

    let auth_service = server.auth_service();
    let adapter = server.users_repo();
    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // First authentication
    let credentials = general_purpose::STANDARD.encode("role_change_user:TestPass123");
    let auth_header = format!("Basic {}", credentials);
    let result1 = auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await;
    assert!(result1.is_ok());
    let user1 = result1.unwrap();
    assert_eq!(user1.role, Role::User);

    // Change user role to DBA
    let users_provider = server.app_context.system_tables().users();
    let mut user = users_provider.get_user_by_id(&UserId::new("role_change_user")).unwrap().unwrap();
    user.role = Role::Dba;
    users_provider.update_user(user).expect("Failed to update user");

    // Invalidate the user cache to ensure the role change is reflected
    auth_service.invalidate_user_cache("role_change_user").await;

    // Second authentication should reflect new role
    let result2 = auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await;
    assert!(result2.is_ok());
    let user2 = result2.unwrap();
    assert_eq!(
        user2.role,
        Role::Dba,
        "Role change should apply to next authentication"
    );
}

/// T143F: Test maximum password length is rejected
#[tokio::test]
async fn test_max_password_10mb_rejected() {
    // Enable password complexity enforcement
    let server = TestServer::new_with_options(None, true).await;

    // Try to create user with very long password (> 72 characters, bcrypt limit)
    // Must satisfy complexity requirements (uppercase, number, special char)
    let long_password = format!("A1!{}", "a".repeat(1000));
    let sql = format!(
        "CREATE USER 'test_long_pass' WITH PASSWORD '{}' ROLE user",
        long_password
    );

    // Execute as root
    let response = server.execute_sql(&sql).await;

    // Should fail due to password length validation
    assert_eq!(
        response.status,
        kalamdb_api::models::ResponseStatus::Error,
        "Creating user with very long password should fail"
    );
    let err = response.error.unwrap();
    let err_msg = err.message;
    assert!(
        err_msg.contains("password") || err_msg.contains("length") || err_msg.contains("72"),
        "Error should mention password length: {}",
        err_msg
    );
}
