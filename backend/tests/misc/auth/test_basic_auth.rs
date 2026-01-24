//! Integration tests for HTTP Basic Authentication
//!
//! These tests verify the unified authentication module works correctly:
//! - Successful authentication with valid credentials
//! - Rejection of invalid credentials
//! - Rejection of missing Authorization header
//! - Rejection of malformed Authorization header
//!
//! **Test Philosophy**: Follow TDD - these tests verify the unified authentication
//! flow that is used by both HTTP and WebSocket handlers.

use super::test_support::{auth_helper, TestServer};
use kalamdb_commons::{models::{ConnectionInfo, UserName}, Role};
use std::sync::Arc;

/// Test successful Basic Auth with valid credentials
#[tokio::test]
async fn test_basic_auth_success() {
    let server = TestServer::new_shared().await;

    // Create test user with password
    let username = "alice";
    let password = "SecurePassword123!";
    auth_helper::create_test_user(&server, username, password, Role::User).await;

    // Test authentication using unified auth module
    use base64::engine::general_purpose;
    use base64::Engine;
    use kalamdb_api::repositories::user_repo::CoreUsersRepo;
    use kalamdb_auth::{authenticate, AuthRequest, UserRepository};

    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Create Basic Auth header
    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, password));
    let auth_header = format!("Basic {}", credentials);
    let auth_request = AuthRequest::Header(auth_header);

    // Create user repository adapter
    let user_repo: Arc<dyn UserRepository> =
        Arc::new(CoreUsersRepo::new(server.app_context.system_tables().users()));

    // Authenticate using unified auth
    let result = authenticate(auth_request, &connection_info, &user_repo).await;

    // Verify success
    if let Err(e) = &result {
        eprintln!("Authentication error: {:?}", e);
    }
    assert!(
        result.is_ok(),
        "Authentication should succeed with valid credentials: {:?}",
        result.as_ref().err()
    );
    let auth_result = result.unwrap();
    assert_eq!(auth_result.user.username, UserName::from(username));
    assert_eq!(auth_result.user.role, Role::User);

    println!("✓ Basic Auth test passed - User authenticated successfully");
}

/// Test authentication failure with invalid password
#[tokio::test]
async fn test_basic_auth_invalid_credentials() {
    let server = TestServer::new_shared().await;

    // Create test user
    let username = "bob";
    let correct_password = "CorrectPassword123!";
    let wrong_password = "WrongPassword456!";
    auth_helper::create_test_user(&server, username, correct_password, Role::User).await;

    // Test authentication with wrong password
    use base64::engine::general_purpose;
    use base64::Engine;
    use kalamdb_api::repositories::user_repo::CoreUsersRepo;
    use kalamdb_auth::{authenticate, AuthRequest, UserRepository};

    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Create Basic Auth header with WRONG password
    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, wrong_password));
    let auth_header = format!("Basic {}", credentials);
    let auth_request = AuthRequest::Header(auth_header);

    // Create user repository adapter
    let user_repo: Arc<dyn UserRepository> =
        Arc::new(CoreUsersRepo::new(server.app_context.system_tables().users()));

    // Authenticate using unified auth
    let result = authenticate(auth_request, &connection_info, &user_repo).await;

    // Verify failure
    assert!(result.is_err(), "Authentication should fail with invalid password");

    println!("✓ Invalid credentials correctly rejected");
}

/// Test authentication failure with missing Authorization header
#[tokio::test]
async fn test_basic_auth_missing_header() {
    use kalamdb_api::repositories::user_repo::CoreUsersRepo;
    use kalamdb_auth::{authenticate, AuthRequest, UserRepository};

    let server = TestServer::new_shared().await;
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Create user repository adapter
    let user_repo: Arc<dyn UserRepository> =
        Arc::new(CoreUsersRepo::new(server.app_context.system_tables().users()));

    // Empty authorization header
    let auth_request = AuthRequest::Header("".to_string());
    let result = authenticate(auth_request, &connection_info, &user_repo).await;

    // Verify failure
    assert!(result.is_err(), "Authentication should fail with missing header");

    println!("✓ Missing Authorization header correctly rejected");
}

/// Test authentication failure with malformed Authorization header
#[tokio::test]
async fn test_basic_auth_malformed_header() {
    use kalamdb_api::repositories::user_repo::CoreUsersRepo;
    use kalamdb_auth::{authenticate, AuthRequest, UserRepository};

    let server = TestServer::new_shared().await;
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Create user repository adapter
    let user_repo: Arc<dyn UserRepository> =
        Arc::new(CoreUsersRepo::new(server.app_context.system_tables().users()));

    // Test various malformed headers
    let malformed_headers = vec![
        "Basic",                 // Missing credentials
        "Basic notbase64!@#",    // Invalid base64
        "Bearer token123",       // Bearer without valid JWT
        "Basic YWxpY2U=",        // Valid base64 but missing colon
        "BasicYWxpY2U6cGFzcw==", // Missing space after "Basic"
    ];

    for malformed_header in malformed_headers {
        let auth_request = AuthRequest::Header(malformed_header.to_string());
        let result = authenticate(auth_request, &connection_info, &user_repo).await;

        assert!(
            result.is_err(),
            "Authentication should fail for malformed header: {}",
            malformed_header
        );

        println!("✓ Malformed header rejected: {}", malformed_header);
    }
}

/// Test authentication with non-existent user
#[tokio::test]
async fn test_basic_auth_nonexistent_user() {
    use base64::engine::general_purpose;
    use base64::Engine;
    use kalamdb_api::repositories::user_repo::CoreUsersRepo;
    use kalamdb_auth::{authenticate, AuthRequest, UserRepository};

    let server = TestServer::new_shared().await;
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Create user repository adapter
    let user_repo: Arc<dyn UserRepository> =
        Arc::new(CoreUsersRepo::new(server.app_context.system_tables().users()));

    // Create auth header for user that doesn't exist
    let credentials = general_purpose::STANDARD.encode("nonexistent:password123");
    let auth_header = format!("Basic {}", credentials);
    let auth_request = AuthRequest::Header(auth_header);

    let result = authenticate(auth_request, &connection_info, &user_repo).await;

    // Verify failure
    assert!(result.is_err(), "Authentication should fail for non-existent user");

    println!("✓ Nonexistent user correctly rejected");
}
