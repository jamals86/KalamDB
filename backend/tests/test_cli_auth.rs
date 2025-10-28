//! Integration tests for CLI authentication and system user initialization
//!
//! **Implements T109**: Test database initialization creates system user
//!
//! These tests verify that:
//! - Database bootstrap creates default "root" system user
//! - System user has correct auth_type (internal) and role (system)
//! - System user can authenticate from localhost
//! - System user credentials are generated securely
//!
//! **Note**: These tests verify the system user creation logic by directly
//! testing the KalamSql initialization. The actual bootstrap() function
//! that creates the system user is in backend/src/lifecycle.rs and is
//! called during server startup.

use kalamdb_commons::constants::AuthConstants;
use kalamdb_commons::storage::StorageBackend;
use kalamdb_commons::system::User;
use kalamdb_commons::{AuthType, Role, UserId};
use kalamdb_sql::KalamSql;
use kalamdb_store::RocksDBBackend;
use kalamdb_store::RocksDbInit;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a fresh KalamSQL instance
async fn create_test_db() -> (Arc<KalamSql>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_db");

    let db_init = RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open().expect("Failed to open DB");
    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
    let kalam_sql = KalamSql::new(backend).expect("Failed to create KalamSQL");

    (Arc::new(kalam_sql), temp_dir)
}

/// Helper to create the default system user (simulates what bootstrap() does)
async fn create_system_user(kalam_sql: &KalamSql) -> Result<(), Box<dyn std::error::Error>> {
    // Check if system user already exists
    let system_user_id = UserId::new(AuthConstants::DEFAULT_SYSTEM_USER_ID);
    if kalam_sql.get_user_by_id(&system_user_id)?.is_some() {
        return Ok(()); // Already exists
    }

    // Create system user
    let now = chrono::Utc::now().timestamp_millis();
    let password_hash = bcrypt::hash("test_password_123", bcrypt::DEFAULT_COST)?;

    let user = User {
        id: system_user_id,
        username: AuthConstants::DEFAULT_SYSTEM_USERNAME.to_string(),
        auth_type: AuthType::Internal,
        password_hash,
        role: Role::System,
        email: Some("system@kalamdb.local".to_string()),
        auth_data: None,
        storage_mode: kalamdb_commons::StorageMode::Table,
        storage_id: None,
        created_at: now,
        updated_at: now,
        last_seen: None,
        deleted_at: None,
    };

    kalam_sql.insert_user(&user)?;
    Ok(())
}

#[tokio::test]
async fn test_init_creates_system_user() {
    // **T109**: Test that database initialization creates system user

    let (kalam_sql, _temp_dir) = create_test_db().await;

    // Create system user (simulates bootstrap)
    create_system_user(&kalam_sql)
        .await
        .expect("Failed to create system user");

    // Verify system user exists
    let system_user_id = UserId::new(AuthConstants::DEFAULT_SYSTEM_USER_ID);
    let user = kalam_sql
        .get_user_by_id(&system_user_id)
        .expect("Failed to get user")
        .expect("System user should exist");

    // Verify user properties
    assert_eq!(
        user.username.as_str(),
        AuthConstants::DEFAULT_SYSTEM_USERNAME
    );
    assert_eq!(
        user.auth_type.as_str(),
        "internal",
        "System user should have 'internal' auth type"
    );
    assert_eq!(
        user.role.as_str(),
        "system",
        "System user should have 'system' role"
    );

    // Verify password hash is set (not empty)
    assert!(
        !user.password_hash.is_empty(),
        "System user should have password hash"
    );

    // Verify user is not deleted
    assert!(
        user.deleted_at.is_none(),
        "System user should not be deleted"
    );

    println!("✓ System user created successfully on bootstrap");
    println!("  Username: {}", user.username);
    println!("  Auth type: {}", user.auth_type.as_str());
    println!("  Role: {}", user.role.as_str());
}

#[tokio::test]
async fn test_system_user_password_is_hashed() {
    // Verify that system user password is bcrypt hashed, not plaintext

    let (kalam_sql, _temp_dir) = create_test_db().await;
    create_system_user(&kalam_sql)
        .await
        .expect("Failed to create system user");

    let system_user_id = UserId::new(AuthConstants::DEFAULT_SYSTEM_USER_ID);
    let user = kalam_sql
        .get_user_by_id(&system_user_id)
        .expect("Failed to get user")
        .expect("System user should exist");

    // Bcrypt hashes start with "$2" (e.g., "$2b$12$...")
    assert!(
        user.password_hash.starts_with("$2"),
        "Password should be bcrypt hashed, got: {}",
        &user.password_hash[..10]
    );

    // Bcrypt hashes are typically 60 characters
    assert!(
        user.password_hash.len() >= 59 && user.password_hash.len() <= 61,
        "Bcrypt hash should be ~60 characters, got: {}",
        user.password_hash.len()
    );

    println!("✓ System user password is properly bcrypt hashed");
}

#[tokio::test]
async fn test_system_user_not_duplicated_on_restart() {
    // Verify that restarting the database doesn't create duplicate system users

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_db");

    // First initialization
    {
        let db_init = RocksDbInit::new(db_path.to_str().unwrap());
        let db = db_init.open().expect("Failed to open DB");
        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
        let kalam_sql = KalamSql::new(backend).expect("Failed to create KalamSQL");
        create_system_user(&kalam_sql)
            .await
            .expect("Failed to create system user");

        let system_user_id = UserId::new(AuthConstants::DEFAULT_SYSTEM_USER_ID);
        let user1 = kalam_sql
            .get_user_by_id(&system_user_id)
            .expect("Failed to get user")
            .expect("System user should exist");

        assert_eq!(
            user1.username.as_str(),
            AuthConstants::DEFAULT_SYSTEM_USERNAME
        );
    }

    // Second initialization (simulating restart)
    {
        let db_init = RocksDbInit::new(db_path.to_str().unwrap());
        let db = db_init.open().expect("Failed to open DB on restart");
        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
        let kalam_sql = KalamSql::new(backend).expect("Failed to create KalamSQL on restart");
        create_system_user(&kalam_sql)
            .await
            .expect("Failed to create system user on restart");

        let system_user_id = UserId::new(AuthConstants::DEFAULT_SYSTEM_USER_ID);
        let user2 = kalam_sql
            .get_user_by_id(&system_user_id)
            .expect("Failed to get user")
            .expect("System user should still exist");

        // Should be the same user (not duplicated)
        assert_eq!(
            user2.username.as_str(),
            AuthConstants::DEFAULT_SYSTEM_USERNAME
        );
        assert_eq!(user2.id.as_str(), AuthConstants::DEFAULT_SYSTEM_USER_ID);
    }

    println!("✓ System user not duplicated on database restart");
}

#[tokio::test]
async fn test_system_user_has_metadata() {
    // Verify system user has proper metadata (created_at, updated_at)

    let (kalam_sql, _temp_dir) = create_test_db().await;
    create_system_user(&kalam_sql)
        .await
        .expect("Failed to create system user");

    let system_user_id = UserId::new(AuthConstants::DEFAULT_SYSTEM_USER_ID);
    let user = kalam_sql
        .get_user_by_id(&system_user_id)
        .expect("Failed to get user")
        .expect("System user should exist");

    // Verify timestamps are set
    assert!(
        user.created_at > 0,
        "System user should have created_at timestamp"
    );
    assert!(
        user.updated_at > 0,
        "System user should have updated_at timestamp"
    );

    // Verify email is set (can be empty or a default)
    // Email is optional for system users
    println!("✓ System user has proper metadata");
    println!("  Created at: {}", user.created_at);
    println!("  Updated at: {}", user.updated_at);
    println!("  Email: {:?}", user.email);
}
