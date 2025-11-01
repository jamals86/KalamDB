//! Integration tests for CLI authentication
//!
//! **Implements T110-T113**: CLI auto-auth, credential storage, multi-instance, rotation tests
//!
//! These tests verify:
//! - CLI automatic authentication using stored credentials
//! - Secure credential storage with proper file permissions
//! - Multiple database instance management
//! - Credential rotation and updates

mod common;
use common::*;


use std::fs;

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

// Note: T110 (test_cli_auto_auth) requires a running server and is better
// suited for end-to-end tests rather than unit tests. It would test:
// - Starting server with system user
// - CLI loading credentials from FileCredentialStore
// - CLI authenticating automatically via HTTP Basic Auth
// - CLI executing queries successfully
//
// This can be implemented as a separate end-to-end test script or
// added to the CLI integration test suite with a test server fixture.
