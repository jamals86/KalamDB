//! Integration tests for HTTP Basic Authentication
//!
//! These tests verify the authentication middleware works correctly:
//! - Successful authentication with valid credentials
//! - Rejection of invalid credentials
//! - Rejection of missing Authorization header
//! - Rejection of malformed Authorization header
//!
//! **Test Philosophy**: Follow TDD - these tests should verify the authentication
//! flow through the middleware layer, testing the full integration path.

#[path = "integration/common/mod.rs"]
mod common;

use actix_web::{test, web, App};
use common::{auth_helper, TestServer};
use kalamdb_commons::Role;
use std::sync::Arc;

/// Test successful Basic Auth with valid credentials
#[tokio::test]
async fn test_basic_auth_success() {
    let server = TestServer::new().await;

    // Create test user with password
    let username = "alice";
    let password = "SecurePassword123!";
    auth_helper::create_test_user(&server, username, password, Role::User).await;

    // Test authentication directly using AuthService (no HTTP layer needed)
    use base64::engine::general_purpose;
    use base64::Engine;
    use kalamdb_auth::connection::ConnectionInfo;
    use kalamdb_auth::service::AuthService;

    let auth_service = AuthService::new(
        "test-secret".to_string(),
        vec!["kalamdb-test".to_string()],
        false,
        false,
        Role::User,
    );
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Create Basic Auth header
    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, password));
    let auth_header = format!("Basic {}", credentials);

    // Authenticate
    let result = auth_service
        .authenticate(
            &auth_header,
            &connection_info,
            &Arc::new(server.kalam_sql.adapter().clone()),
        )
        .await;

    // Verify success
    assert!(
        result.is_ok(),
        "Authentication should succeed with valid credentials"
    );
    let authenticated_user = result.unwrap();
    assert_eq!(authenticated_user.username, username);
    assert_eq!(authenticated_user.role, Role::User);

    println!("✓ Basic Auth test passed - User authenticated successfully");
}

/// Test authentication failure with invalid password
#[tokio::test]
async fn test_basic_auth_invalid_credentials() {
    let server = TestServer::new().await;

    // Create test user
    let username = "bob";
    let correct_password = "CorrectPassword123!";
    let wrong_password = "WrongPassword456!";
    auth_helper::create_test_user(&server, username, correct_password, Role::User).await;

    // Test authentication with wrong password
    use base64::engine::general_purpose;
    use base64::Engine;
    use kalamdb_auth::connection::ConnectionInfo;
    use kalamdb_auth::service::AuthService;

    let auth_service = AuthService::new(
        "test-secret".to_string(),
        vec!["kalamdb-test".to_string()],
        false,
        false,
        Role::User,
    );
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Create Basic Auth header with WRONG password
    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, wrong_password));
    let auth_header = format!("Basic {}", credentials);

    // Authenticate
    let result = auth_service
        .authenticate(
            &auth_header,
            &connection_info,
            &Arc::new(server.kalam_sql.adapter().clone()),
        )
        .await;

    // Verify failure
    assert!(
        result.is_err(),
        "Authentication should fail with invalid password"
    );

    println!("✓ Invalid credentials correctly rejected");
}

/// Test authentication failure with missing Authorization header
#[tokio::test]
async fn test_basic_auth_missing_header() {
    use kalamdb_auth::connection::ConnectionInfo;
    use kalamdb_auth::service::AuthService;

    let server = TestServer::new().await;
    let auth_service = AuthService::new(
        "test-secret".to_string(),
        vec!["kalamdb-test".to_string()],
        false,
        false,
        Role::User,
    );
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Empty authorization header
    let result = auth_service
        .authenticate(
            "",
            &connection_info,
            &Arc::new(server.kalam_sql.adapter().clone()),
        )
        .await;

    // Verify failure
    assert!(
        result.is_err(),
        "Authentication should fail with missing header"
    );

    println!("✓ Missing Authorization header correctly rejected");
}

/// Test authentication failure with malformed Authorization header
#[tokio::test]
async fn test_basic_auth_malformed_header() {
    use kalamdb_auth::connection::ConnectionInfo;
    use kalamdb_auth::service::AuthService;

    let server = TestServer::new().await;
    let auth_service = AuthService::new(
        "test-secret".to_string(),
        vec!["kalamdb-test".to_string()],
        false,
        false,
        Role::User,
    );
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Test various malformed headers
    let malformed_headers = vec![
        "Basic",                 // Missing credentials
        "Basic notbase64!@#",    // Invalid base64
        "Bearer token123",       // Wrong auth scheme for basic
        "Basic YWxpY2U=",        // Valid base64 but missing colon
        "BasicYWxpY2U6cGFzcw==", // Missing space after "Basic"
    ];

    for malformed_header in malformed_headers {
        let result = auth_service
            .authenticate(
                malformed_header,
                &connection_info,
                &Arc::new(server.kalam_sql.adapter().clone()),
            )
            .await;

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
    use kalamdb_auth::connection::ConnectionInfo;
    use kalamdb_auth::service::AuthService;

    let server = TestServer::new().await;
    let auth_service = AuthService::new(
        "test-secret".to_string(),
        vec!["kalamdb-test".to_string()],
        false,
        false,
        Role::User,
    );
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // Create auth header for user that doesn't exist
    let credentials = general_purpose::STANDARD.encode("nonexistent:password123");
    let auth_header = format!("Basic {}", credentials);

    let result = auth_service
        .authenticate(
            &auth_header,
            &connection_info,
            &Arc::new(server.kalam_sql.adapter().clone()),
        )
        .await;

    // Verify failure
    assert!(
        result.is_err(),
        "Authentication should fail for non-existent user"
    );

    println!("✓ Nonexistent user correctly rejected");
}
