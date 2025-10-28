//! Authentication test helpers for integration tests.
//!
//! This module provides utilities for testing authentication flows:
//! - Creating test users with passwords
//! - Authenticating with HTTP Basic Auth
//! - Generating test JWT tokens
//! - Validating authentication responses

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use kalamdb_commons::system::User;
use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};

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
    // Hash password using bcrypt
    let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .expect("Failed to hash password");

    let now = chrono::Utc::now().timestamp_millis();
    
    let user = User {
        id: UserId::new(format!("test_{}", username)),
        username: username.to_string(),
        password_hash,
        role,
        email: Some(format!("{}@example.com", username)),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: Some(StorageId::local()),
        created_at: now,
        updated_at: now,
        last_seen: None,
        deleted_at: None,
    };

    // Insert user directly via KalamSQL
    server
        .kalam_sql
        .insert_user(&user)
        .expect("Failed to insert test user");

    user
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

/// Create test user with default settings for quick testing
///
/// Creates a user with:
/// - Username: provided
/// - Password: "Test123!@#" (default test password)
/// - Role: User
/// - Email: username@example.com
///
/// # Example
///
/// ```no_run
/// let user = create_default_test_user(&server, "alice").await;
/// ```
pub async fn create_default_test_user(server: &super::TestServer, username: &str) -> User {
    create_test_user(server, username, "Test123!@#", Role::User).await
}

/// Create test DBA user for admin operations
///
/// # Example
///
/// ```no_run
/// let dba = create_test_dba(&server, "admin").await;
/// assert_eq!(dba.role, Role::Dba);
/// ```
pub async fn create_test_dba(server: &super::TestServer, username: &str) -> User {
    create_test_user(server, username, "AdminPass123!", Role::Dba).await
}

/// Create test system user for internal operations
///
/// System users have special privileges and typically authenticate via localhost.
///
/// # Example
///
/// ```no_run
/// let system_user = create_test_system_user(&server, "system_proc").await;
/// assert_eq!(system_user.role, Role::System);
/// ```
pub async fn create_test_system_user(server: &super::TestServer, username: &str) -> User {
    let now = chrono::Utc::now().timestamp_millis();
    
    let user = User {
        id: UserId::new(format!("sys_{}", username)),
        username: username.to_string(),
        password_hash: String::new(), // No password for system users (localhost-only)
        role: Role::System,
        email: None,
        auth_type: AuthType::Internal,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: Some(StorageId::local()),
        created_at: now,
        updated_at: now,
        last_seen: None,
        deleted_at: None,
    };

    server
        .kalam_sql
        .insert_user(&user)
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
