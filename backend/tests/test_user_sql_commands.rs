//! Integration tests for SQL-based user management commands
//!
//! Tests CREATE USER, ALTER USER, and DROP USER SQL commands with:
//! - Successful user creation with different auth types
//! - Password changes and role updates
//! - Authorization checks (DBA/System only)
//! - Soft deletion

#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_api::models::ResponseStatus;

#[tokio::test]
async fn test_create_user_with_password_success() {
    let server = TestServer::new().await;
    let admin_id = "root"; // Default system user

    // Execute CREATE USER command
    let sql = "CREATE USER 'alice' WITH PASSWORD 'SecurePass123' ROLE developer EMAIL 'alice@example.com'";
    let result = server.execute_sql_as_user(sql, admin_id).await;

    assert_eq!(
        result.status,
        ResponseStatus::Success,
        "CREATE USER should succeed: {:?}",
        result.error
    );

    // Verify user was created via system.users
    let query = "SELECT * FROM system.users WHERE username = 'alice'";
    let result = server.execute_sql_as_user(query, admin_id).await;

    assert!(!result.results.is_empty());
    let rows = result.results[0].rows.as_ref().unwrap();
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    assert_eq!(row.get("username").unwrap().as_str().unwrap(), "alice");
    assert_eq!(row.get("auth_type").unwrap().as_str().unwrap(), "password");
    assert_eq!(row.get("role").unwrap().as_str().unwrap(), "service"); // developer -> service
    assert_eq!(
        row.get("email").unwrap().as_str().unwrap(),
        "alice@example.com"
    );
}

#[tokio::test]
async fn test_create_user_with_oauth_success() {
    let server = TestServer::new().await;
    let admin_id = "root";

    // Note: JSON string in SQL needs careful escaping
    let sql = r#"CREATE USER 'bob' WITH OAUTH '{"provider": "google", "subject": "12345"}' ROLE viewer EMAIL 'bob@example.com'"#;
    let result = server.execute_sql_as_user(sql, admin_id).await;

    assert_eq!(
        result.status,
        ResponseStatus::Success,
        "CREATE USER with OAuth should succeed: {:?}",
        result.error
    );

    let query = "SELECT * FROM system.users WHERE username = 'bob'";
    let result = server.execute_sql_as_user(query, admin_id).await;

    let rows = result.results[0].rows.as_ref().unwrap();
    let row = &rows[0];

    assert_eq!(row.get("username").unwrap().as_str().unwrap(), "bob");
    assert_eq!(row.get("auth_type").unwrap().as_str().unwrap(), "oauth");
    assert_eq!(row.get("role").unwrap().as_str().unwrap(), "user"); // viewer -> user
}

#[tokio::test]
async fn test_create_user_without_authorization_fails() {
    let server = TestServer::new().await;
    let admin_id = "root";

    // Create a regular user
    let create_sql = "CREATE USER 'regular_user' WITH PASSWORD 'Pass123!' ROLE user";
    server.execute_sql_as_user(create_sql, admin_id).await;

    // Try to create a user as regular user
    let sql = "CREATE USER 'charlie' WITH PASSWORD 'TestPass123' ROLE user";
    let result = server.execute_sql_as_user(sql, "regular_user").await;

    assert_eq!(
        result.status,
        ResponseStatus::Error,
        "Regular user should not be able to create users"
    );
    // The error message might vary, but it should be an error
}

#[tokio::test]
async fn test_alter_user_set_password() {
    let server = TestServer::new().await;
    let admin_id = "root";

    // Create user first
    let create_sql = "CREATE USER 'dave' WITH PASSWORD 'OldPass123' ROLE user";
    server.execute_sql_as_user(create_sql, admin_id).await;

    // Get old hash
    let query = "SELECT password_hash FROM system.users WHERE username = 'dave'";
    let result = server.execute_sql_as_user(query, admin_id).await;
    let old_hash = result.results[0].rows.as_ref().unwrap()[0]
        .get("password_hash")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Change password
    let alter_sql = "ALTER USER 'dave' SET PASSWORD 'NewPass456'";
    let result = server.execute_sql_as_user(alter_sql, admin_id).await;
    assert_eq!(result.status, ResponseStatus::Success);

    // Verify hash changed
    let result = server.execute_sql_as_user(query, admin_id).await;
    let new_hash = result.results[0].rows.as_ref().unwrap()[0]
        .get("password_hash")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    assert_ne!(old_hash, new_hash);
}

#[tokio::test]
async fn test_alter_user_set_role() {
    let server = TestServer::new().await;
    let admin_id = "root";

    // Create user first
    let create_sql = "CREATE USER 'eve' WITH PASSWORD 'Password123' ROLE user";
    server.execute_sql_as_user(create_sql, admin_id).await;

    // Change role
    let alter_sql = "ALTER USER 'eve' SET ROLE dba";
    let result = server.execute_sql_as_user(alter_sql, admin_id).await;
    assert_eq!(result.status, ResponseStatus::Success);

    // Verify role
    let query = "SELECT role FROM system.users WHERE username = 'eve'";
    let result = server.execute_sql_as_user(query, admin_id).await;
    let role = result.results[0].rows.as_ref().unwrap()[0]
        .get("role")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(role, "dba");
}

#[tokio::test]
async fn test_drop_user_soft_delete() {
    let server = TestServer::new().await;
    let admin_id = "root";

    // Create user first
    let create_sql = "CREATE USER 'frank' WITH PASSWORD 'Password123' ROLE user";
    server.execute_sql_as_user(create_sql, admin_id).await;

    // Drop user
    let drop_sql = "DROP USER 'frank'";
    let result = server.execute_sql_as_user(drop_sql, admin_id).await;
    assert_eq!(result.status, ResponseStatus::Success);

    // Verify user is soft-deleted (deleted_at IS NOT NULL)
    let query_deleted =
        "SELECT deleted_at FROM system.users WHERE username = 'frank' AND deleted_at IS NOT NULL";
    let result = server.execute_sql_as_user(query_deleted, admin_id).await;

    assert!(!result.results.is_empty());
    if let Some(rows) = &result.results[0].rows {
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].get("deleted_at").unwrap().is_null());
    } else {
        panic!("Expected rows");
    }
}

#[tokio::test]
async fn test_create_user_weak_password_rejected() {
    // Enable password complexity enforcement
    let server = TestServer::new_with_options(None, true).await;
    let admin_id = "root";

    let weak_passwords = vec![
        "password", "123456", "qwerty", "admin", "letmein", "welcome", "monkey",
    ];

    for weak_pass in weak_passwords {
        let sql = format!(
            "CREATE USER 'weak_user_{}' WITH PASSWORD '{}' ROLE user",
            weak_pass, weak_pass
        );

        let result = server.execute_sql_as_user(&sql, admin_id).await;
        assert_eq!(result.status, ResponseStatus::Error);
        let error_msg = result.error.unwrap().message;
        assert!(error_msg.contains("weak") || error_msg.contains("Password must include"));
    }
}

#[tokio::test]
async fn test_create_user_duplicate_error() {
    let server = TestServer::new().await;
    let admin_id = "root";

    let create_sql = "CREATE USER 'duplicate_test' WITH PASSWORD 'Password123!' ROLE user";
    server.execute_sql_as_user(create_sql, admin_id).await;

    // Try again
    let result = server.execute_sql_as_user(create_sql, admin_id).await;
    assert_eq!(result.status, ResponseStatus::Error);
    assert!(result.error.unwrap().message.contains("already exists"));
}

#[tokio::test]
async fn test_alter_user_not_found() {
    let server = TestServer::new().await;
    let admin_id = "root";

    let alter_sql = "ALTER USER 'nonexistent_user' SET ROLE dba";
    let result = server.execute_sql_as_user(alter_sql, admin_id).await;
    assert_eq!(result.status, ResponseStatus::Error);
    assert!(result.error.unwrap().message.contains("not found"));
}

#[tokio::test]
async fn test_drop_user_if_exists() {
    let server = TestServer::new().await;
    let admin_id = "root";

    let drop_sql = "DROP USER IF EXISTS 'user_that_never_existed'";
    let result = server.execute_sql_as_user(drop_sql, admin_id).await;
    assert_eq!(result.status, ResponseStatus::Success);
}
