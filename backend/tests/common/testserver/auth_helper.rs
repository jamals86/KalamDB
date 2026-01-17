#![allow(dead_code)]
//! Authentication test helpers for integration tests.
//!
//! This module provides utilities for testing authentication flows:
//! - Creating test users with passwords
//! - Authenticating with HTTP Basic Auth
//! - Generating test JWT tokens
//! - Validating authentication responses

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use kalamdb_commons::types::User;
use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_core::error::KalamDbError;
use kalamdb_core::sql::executor::models::ExecutionContext;
use serde::{Deserialize, Serialize};

/// Create a test user with password authentication
///
/// # Arguments
///
/// * `server` - Test server instance
/// * `username` - Username for the test user
/// * `password` - Plain-text password (will be hashed with bcrypt)
/// * `role` - User role (User, Service, Dba, or System)
///
/// # Returns
///
/// The created User object
///
/// # Example
///
/// ```no_run
/// let user = create_test_user(&server, "alice", "SecurePass123!", Role::User).await;
/// assert_eq!(user.username, "alice");
/// ```
pub async fn create_test_user(
    server: &super::TestServer,
    username: &str,
    password: &str,
    role: Role,
) -> User {
    let now = chrono::Utc::now().timestamp_millis();

    // Use username as the user ID (not "test_{username}")
    let user_id = UserId::new(username);

    // Create user via SQL executor (bypassing HTTP layer)
    let role_str = match role {
        Role::User => "user",
        Role::Service => "service",
        Role::Dba => "dba",
        Role::System => "system",
    };

    let create_user_sql = format!(
        "CREATE USER '{}' WITH PASSWORD '{}' ROLE {} EMAIL '{}@example.com'",
        username, password, role_str, username
    );

    // Use system user to create the user
    let system_user_id = UserId::new("system");
    let session = server.app_context.base_session_context();
    let exec_ctx = ExecutionContext::new(system_user_id, Role::System, session);

    let users_provider = server.app_context.system_tables().users();
    if let Ok(Some(mut existing)) = users_provider.get_user_by_username(username) {
        existing.password_hash =
            bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");
        existing.role = role;
        existing.email = Some(format!("{}@example.com", username));
        existing.auth_type = AuthType::Password;
        existing.auth_data = None;
        existing.failed_login_attempts = 0;
        existing.locked_until = None;
        existing.deleted_at = None;
        existing.updated_at = chrono::Utc::now().timestamp_millis();
        users_provider.update_user(existing).expect("Failed to update test user");
    } else {
        let result = server
            .sql_executor
            .execute(create_user_sql.as_str(), &exec_ctx, Vec::new())
            .await;

        if let Err(e) = &result {
            if matches!(e, KalamDbError::AlreadyExists(_)) {
                if let Ok(Some(mut existing)) = users_provider.get_user_by_username(username) {
                    existing.password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
                        .expect("Failed to hash password");
                    existing.role = role;
                    existing.email = Some(format!("{}@example.com", username));
                    existing.auth_type = AuthType::Password;
                    existing.auth_data = None;
                    existing.failed_login_attempts = 0;
                    existing.locked_until = None;
                    existing.deleted_at = None;
                    existing.updated_at = chrono::Utc::now().timestamp_millis();
                    users_provider.update_user(existing).expect("Failed to update test user");
                } else {
                    panic!("Failed to load existing test user after create conflict");
                }
            } else {
                panic!("Failed to create test user: {:?}", e);
            }
        }
    }

    eprintln!("✓ Created test user: {}", username);

    // Return user object for test verification
    User {
        id: user_id,
        username: username.into(),
        password_hash: String::new(), // Not needed for tests
        role,
        email: Some(format!("{}@example.com", username)),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: Some(StorageId::local()),
        failed_login_attempts: 0,
        locked_until: None,
        last_login_at: None,
        created_at: now,
        updated_at: now,
        last_seen: None,
        deleted_at: None,
    }
}

/// Create HTTP Basic Auth header value
///
/// # Arguments
///
/// * `username` - Username for authentication
/// * `password` - Password for authentication
///
/// # Returns
///
/// Authorization header value in format "Basic <base64(username:password)>"
///
/// # Example
///
/// ```no_run
/// let auth_header = create_basic_auth_header("alice", "SecurePass123!");
/// assert!(auth_header.starts_with("Basic "));
/// ```
pub fn create_basic_auth_header(username: &str, password: &str) -> String {
    let credentials = format!("{}:{}", username, password);
    let encoded = BASE64.encode(credentials.as_bytes());
    format!("Basic {}", encoded)
}

/// Authenticate with HTTP Basic Auth and verify response
///
/// Helper function for integration tests that need to authenticate requests.
///
/// # Arguments
///
/// * `username` - Username for authentication
/// * `password` - Password for authentication
///
/// # Returns
///
/// Tuple of (Authorization header string, expected success status)
///
/// # Example
///
/// ```no_run
/// let (auth_header, should_succeed) = authenticate_basic("alice", "SecurePass123!");
/// // Use auth_header in HTTP request
/// ```
pub fn authenticate_basic(username: &str, password: &str) -> (String, bool) {
    let auth_header = create_basic_auth_header(username, password);
    // For now, assume authentication should succeed
    // Tests will verify actual success/failure based on user existence and credentials
    (auth_header, true)
}

/// Create JWT token for testing
///
/// # Arguments
///
/// * `username` - Username to include in JWT claims
/// * `secret` - JWT secret key for signing
/// * `exp_seconds` - Token expiration time in seconds from now
///
/// # Returns
///
/// JWT token string
///
/// # Example
///
/// ```no_run
/// let token = create_jwt_token("alice", "my-secret-key", 3600);
/// let auth_header = format!("Bearer {}", token);
/// ```
pub fn create_jwt_token(username: &str, secret: &str, exp_seconds: i64) -> String {
    #[derive(Debug, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        iss: String,
        exp: usize,
        iat: usize,
        username: String,
        email: Option<String>,
        role: String,
    }

    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: format!("user_{}", username),
        iss: "kalamdb-test".to_string(),
        exp: (now as i64 + exp_seconds) as usize,
        iat: now,
        username: username.to_string(),
        email: Some(format!("{}@example.com", username)),
        role: "user".to_string(),
    };

    let header = Header::new(Algorithm::HS256);
    let encoding_key = EncodingKey::from_secret(secret.as_bytes());
    encode(&header, &claims, &encoding_key).expect("Failed to create JWT token")
}

/// Create test system user for internal operations
///
/// # Example
///
/// ```no_run
/// let system_user = create_system_user(&server, "system").await;
/// assert_eq!(system_user.role, Role::System);
/// ```
pub async fn create_system_user(server: &super::TestServer, username: &str) -> User {
    let now = chrono::Utc::now().timestamp_millis();

    let user = User {
        id: UserId::new(format!("sys_{}", username)),
        username: username.into(),
        password_hash: String::new(), // No password for system users (localhost-only)
        role: Role::System,
        email: None,
        auth_type: AuthType::Internal,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: Some(StorageId::local()),
        failed_login_attempts: 0,
        locked_until: None,
        last_login_at: None,
        created_at: now,
        updated_at: now,
        last_seen: None,
        deleted_at: None,
    };

    server
        .app_context
        .system_tables()
        .users()
        .create_user(user.clone())
        .expect("Failed to insert system user");

    user
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_basic_auth_header() {
        let header = create_basic_auth_header("alice", "password123");
        assert!(header.starts_with("Basic "));

        // Decode and verify
        let encoded = header.strip_prefix("Basic ").unwrap();
        let decoded = String::from_utf8(BASE64.decode(encoded).unwrap()).unwrap();
        assert_eq!(decoded, "alice:password123");
    }

    #[test]
    fn test_authenticate_basic() {
        let (auth_header, should_succeed) = authenticate_basic("bob", "SecurePass!");
        assert!(auth_header.starts_with("Basic "));
        assert!(should_succeed); // Default assumption
    }
}
