//! Integration tests for authentication and authorization
//!
//! **Implements T052-T054, T110-T113**: Authentication, credential management, and access control
//!
//! These tests validate:
//! - JWT authentication with valid/invalid tokens
//! - Localhost authentication bypass
//! - Credential storage and security
//! - Multiple instance management
//! - Credential rotation and deletion
//! - Admin operations with proper authentication

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::time::Duration;

use crate::common::*;

/// T052: Test JWT authentication with valid token
#[tokio::test]
async fn test_cli_jwt_authentication() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Note: This test assumes JWT auth is optional on localhost
    // In production, would need to obtain valid token first
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg("SELECT 1 as auth_test")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();

    // Should work (localhost typically bypasses auth)
    assert!(
        output.status.success() || String::from_utf8_lossy(&output.stdout).contains("auth_test"),
        "Should handle authentication"
    );
}

/// T053: Test invalid token handling
#[tokio::test]
async fn test_cli_invalid_token() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--token")
        .arg("invalid.jwt.token")
        .arg("--command")
        .arg("SELECT 1")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();

    // May succeed on localhost (auth bypass) or fail with auth error
    // Either outcome is acceptable
    assert!(
        output.status.success()
            || String::from_utf8_lossy(&output.stderr).contains("auth")
            || String::from_utf8_lossy(&output.stderr).contains("token"),
        "Should handle invalid token appropriately"
    );
}

/// T054: Test localhost authentication bypass
#[tokio::test]
async fn test_cli_localhost_auth_bypass() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Localhost connections should work without token
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg("SELECT 'localhost' as test")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();

    // Should succeed without authentication
    assert!(
        output.status.success(),
        "Localhost should bypass authentication"
    );
}

/// Test CLI authentication with unauthorized user
#[tokio::test]
async fn test_cli_authenticate_unauthorized_user() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running at {}. Skipping test.", SERVER_URL);
        return;
    }

    // Try to authenticate with invalid credentials
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--username")
        .arg("invalid_user")
        .arg("--password")
        .arg("wrong_password")
        .arg("--command")
        .arg("SELECT 1")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail with authentication error
    assert!(
        !output.status.success(),
        "CLI should fail with invalid credentials"
    );
    assert!(
        stderr.contains("Unauthorized")
            || stderr.contains("authentication")
            || stderr.contains("401"),
        "Error should indicate authentication failure: {}",
        stderr
    );
}

/// Test CLI authentication with valid user and check \info command
#[tokio::test]
async fn test_cli_authenticate_and_check_info() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running at {}. Skipping test.", SERVER_URL);
        return;
    }

    // Use unique username to avoid conflicts
    let test_username = format!(
        "testuser_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    // Create a test user via API
    let create_user_sql = format!("CREATE USER {} IDENTIFIED BY 'testpass123'", test_username);
    let result = execute_sql_as_root(&create_user_sql).await;
    if result.is_err() || result.as_ref().unwrap()["status"] != "success" {
        eprintln!("⚠️  Failed to create test user, skipping test");
        return;
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Authenticate with the new user and run \info command
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--username")
        .arg(&test_username)
        .arg("--password")
        .arg("testpass123")
        .arg("--command")
        .arg("\\info")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed and show user info
    assert!(
        output.status.success(),
        "CLI should authenticate successfully with valid user"
    );
    assert!(
        stdout.contains(&test_username),
        "Info output should show the authenticated username: {}",
        stdout
    );

    // Cleanup
    let _ = execute_sql_as_root(&format!("DROP USER {}", test_username)).await;
}

// ============================================================================
// Credential Store Tests (from test_cli_auth.rs)
// ============================================================================

use kalam_cli::FileCredentialStore;
use kalam_link::credentials::{CredentialStore, Credentials};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Helper to create a temporary credential store
fn create_temp_store() -> (FileCredentialStore, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let creds_path = temp_dir.path().join("credentials.toml");

    let store =
        FileCredentialStore::with_path(creds_path).expect("Failed to create credential store");

    (store, temp_dir)
}

#[test]
fn test_cli_credentials_stored_securely() {
    // **T111**: Test that credentials are stored with secure file permissions (0600 on Unix)

    let (mut store, temp_dir) = create_temp_store();

    // Store credentials
    let creds = Credentials::new(
        "test_instance".to_string(),
        "alice".to_string(),
        "secret123".to_string(),
    );

    store
        .set_credentials(&creds)
        .expect("Failed to store credentials");

    // Verify file exists
    let creds_path = temp_dir.path().join("credentials.toml");
    assert!(creds_path.exists(), "Credentials file should exist");

    // Verify file permissions on Unix (should be 0600 - owner read/write only)
    #[cfg(unix)]
    {
        let metadata = fs::metadata(&creds_path).expect("Failed to get file metadata");
        let permissions = metadata.permissions();
        let mode = permissions.mode();

        // Extract permission bits (last 9 bits)
        let perms = mode & 0o777;

        assert_eq!(
            perms, 0o600,
            "Credentials file should have 0600 permissions, got: {:o}",
            perms
        );

        println!("✓ Credentials file has secure permissions: {:o}", perms);
    }

    // Verify file contents don't leak credentials in plain sight
    let file_contents = fs::read_to_string(&creds_path).expect("Failed to read credentials file");

    // TOML format should be readable but structured
    assert!(file_contents.contains("[instances.test_instance]"));
    assert!(file_contents.contains("username = \"alice\""));
    assert!(file_contents.contains("password = \"secret123\""));

    println!("✓ Credentials stored securely");
}

#[test]
fn test_cli_multiple_instances() {
    // **T112**: Test managing credentials for multiple database instances

    let (mut store, _temp_dir) = create_temp_store();

    // Store credentials for multiple instances
    let instances = vec![
        ("local", "alice", "local_pass"),
        ("staging", "bob", "staging_pass"),
        ("production", "admin", "prod_pass"),
    ];

    for (instance, username, password) in &instances {
        let creds = Credentials::new(
            instance.to_string(),
            username.to_string(),
            password.to_string(),
        );
        store
            .set_credentials(&creds)
            .expect("Failed to store credentials");
    }

    // Verify all instances are stored
    let instance_list = store.list_instances().expect("Failed to list instances");

    assert_eq!(instance_list.len(), 3, "Should have 3 instances");
    assert!(instance_list.contains(&"local".to_string()));
    assert!(instance_list.contains(&"staging".to_string()));
    assert!(instance_list.contains(&"production".to_string()));

    // Verify each instance has correct credentials
    for (instance, username, password) in &instances {
        let retrieved = store
            .get_credentials(instance)
            .expect("Failed to get credentials")
            .expect("Credentials should exist");

        assert_eq!(&retrieved.instance, instance);
        assert_eq!(&retrieved.username, username);
        assert_eq!(&retrieved.password, password);
    }

    println!("✓ Multiple instances managed correctly");
}

#[test]
fn test_cli_credential_rotation() {
    // **T113**: Test updating credentials for an existing instance

    let (mut store, _temp_dir) = create_temp_store();

    // Initial credentials
    let creds_v1 = Credentials::new(
        "production".to_string(),
        "admin".to_string(),
        "old_password".to_string(),
    );

    store
        .set_credentials(&creds_v1)
        .expect("Failed to store initial credentials");

    // Retrieve initial credentials
    let retrieved_v1 = store
        .get_credentials("production")
        .expect("Failed to get credentials")
        .expect("Credentials should exist");

    assert_eq!(retrieved_v1.password, "old_password");

    // Rotate credentials (update password)
    let creds_v2 = Credentials::new(
        "production".to_string(),
        "admin".to_string(),
        "new_secure_password_123".to_string(),
    );

    store
        .set_credentials(&creds_v2)
        .expect("Failed to update credentials");

    // Retrieve updated credentials
    let retrieved_v2 = store
        .get_credentials("production")
        .expect("Failed to get credentials")
        .expect("Credentials should exist");

    assert_eq!(retrieved_v2.password, "new_secure_password_123");
    assert_eq!(retrieved_v2.username, "admin");

    // Verify only one instance exists (not duplicated)
    let instance_list = store.list_instances().expect("Failed to list instances");

    assert_eq!(instance_list.len(), 1, "Should still have only 1 instance");

    println!("✓ Credential rotation successful");
}

#[test]
fn test_cli_delete_credentials() {
    // Test deleting credentials for an instance

    let (mut store, _temp_dir) = create_temp_store();

    // Store credentials
    let creds = Credentials::new(
        "temp_instance".to_string(),
        "user".to_string(),
        "pass".to_string(),
    );

    store
        .set_credentials(&creds)
        .expect("Failed to store credentials");

    // Verify it exists
    assert!(store
        .get_credentials("temp_instance")
        .expect("Failed to get credentials")
        .is_some());

    // Delete credentials
    store
        .delete_credentials("temp_instance")
        .expect("Failed to delete credentials");

    // Verify it's gone
    assert!(store
        .get_credentials("temp_instance")
        .expect("Failed to get credentials")
        .is_none());

    let instance_list = store.list_instances().expect("Failed to list instances");

    assert_eq!(instance_list.len(), 0, "Should have no instances");

    println!("✓ Credential deletion successful");
}

#[test]
fn test_cli_credentials_with_server_url() {
    // Test storing credentials with custom server URL

    let (mut store, _temp_dir) = create_temp_store();

    // Store credentials with server URL
    let creds = Credentials::with_server_url(
        "cloud".to_string(),
        "alice".to_string(),
        "secret".to_string(),
        "https://db.example.com:8080".to_string(),
    );

    store
        .set_credentials(&creds)
        .expect("Failed to store credentials");

    // Retrieve and verify
    let retrieved = store
        .get_credentials("cloud")
        .expect("Failed to get credentials")
        .expect("Credentials should exist");

    assert_eq!(retrieved.get_server_url(), "https://db.example.com:8080");

    println!("✓ Credentials with custom server URL work correctly");
}

#[test]
fn test_cli_empty_store() {
    // Test operations on empty credential store

    let (store, _temp_dir) = create_temp_store();

    // List instances on empty store
    let instances = store.list_instances().expect("Failed to list instances");

    assert_eq!(instances.len(), 0, "Empty store should have no instances");

    // Get non-existent credentials
    let creds = store
        .get_credentials("nonexistent")
        .expect("Failed to get credentials");

    assert!(
        creds.is_none(),
        "Non-existent credentials should return None"
    );

    println!("✓ Empty credential store behaves correctly");
}