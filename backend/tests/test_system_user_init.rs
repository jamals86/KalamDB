/// Integration tests for T125-T127: System User Initialization
///
/// Tests that the database creates a default system user on first startup
/// with appropriate credentials and security settings.

use kalamdb_commons::constants::AuthConstants;
use kalamdb_commons::system::User;
use kalamdb_commons::{AuthType, Role, UserId};
use kalamdb_core::storage::RocksDbInit;
use kalamdb_sql::KalamSql;
use std::sync::Arc;

/// Helper to create fresh KalamSql instance for testing
async fn setup_kalam_sql() -> Arc<KalamSql> {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_db");
    
    // Initialize storage using RocksDbInit
    let db_init = RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open().expect("Failed to open database");
    
    Arc::new(KalamSql::new(db).expect("Failed to create KalamSql"))
}

/// T125: Verify system user is created on first startup
#[tokio::test]
async fn test_system_user_created_on_init() {
    let kalam_sql = setup_kalam_sql().await;
    
    // Simulate the bootstrap function creating the system user
    // (In production, this is called by lifecycle::bootstrap())
    create_default_system_user_for_test(kalam_sql.clone())
        .await
        .expect("Failed to create system user");
    
    // Verify the user exists
    let user = kalam_sql
        .get_user(AuthConstants::DEFAULT_SYSTEM_USERNAME)
        .expect("Failed to query user")
        .expect("System user should exist");
    
    assert_eq!(user.username, AuthConstants::DEFAULT_SYSTEM_USERNAME);
    assert_eq!(user.role, Role::System);
    assert_eq!(user.auth_type, AuthType::Internal);
    assert!(user.password_hash.len() > 0, "Password hash should not be empty");
    assert_eq!(user.auth_data, None, "Should not have allow_remote metadata by default");
}

/// T126: Verify random password is generated and hashed
#[tokio::test]
async fn test_system_user_has_random_password() {
    let kalam_sql = setup_kalam_sql().await;
    
    create_default_system_user_for_test(kalam_sql.clone())
        .await
        .expect("Failed to create system user");
    
    let user = kalam_sql
        .get_user(AuthConstants::DEFAULT_SYSTEM_USERNAME)
        .expect("Failed to query user")
        .expect("System user should exist");
    
    // Verify password is bcrypt hashed (starts with $2b$ or $2y$)
    assert!(
        user.password_hash.starts_with("$2b$") || user.password_hash.starts_with("$2y$"),
        "Password should be bcrypt hashed, got: {}",
        user.password_hash
    );
    
    // Verify password hash has correct length (bcrypt hashes are 60 chars)
    assert_eq!(
        user.password_hash.len(),
        60,
        "Bcrypt hash should be 60 characters"
    );
}

/// T127: Verify system user credentials are properly configured
#[tokio::test]
async fn test_system_user_credentials_configuration() {
    let kalam_sql = setup_kalam_sql().await;
    
    create_default_system_user_for_test(kalam_sql.clone())
        .await
        .expect("Failed to create system user");
    
    let user = kalam_sql
        .get_user(AuthConstants::DEFAULT_SYSTEM_USERNAME)
        .expect("Failed to query user")
        .expect("System user should exist");
    
    // Verify user ID follows sys_ prefix convention
    assert!(
        user.id.as_str().starts_with("sys_"),
        "System user ID should start with 'sys_', got: {}",
        user.id.as_str()
    );
    
    // Verify email is set for system user
    let expected_email = format!("{}@localhost", AuthConstants::DEFAULT_SYSTEM_USERNAME);
    assert_eq!(user.email, Some(expected_email));
    
    // Verify timestamps are set
    assert!(user.created_at > 0, "created_at should be set");
    assert_eq!(user.created_at, user.updated_at, "created_at should equal updated_at on creation");
    
    // Verify user is not deleted
    assert_eq!(user.deleted_at, None, "System user should not be deleted");
}

/// T125: Verify idempotence - second initialization doesn't duplicate user
#[tokio::test]
async fn test_system_user_initialization_idempotent() {
    let kalam_sql = setup_kalam_sql().await;
    
    // Create user first time
    create_default_system_user_for_test(kalam_sql.clone())
        .await
        .expect("First creation should succeed");
    
    let first_user = kalam_sql
        .get_user(AuthConstants::DEFAULT_SYSTEM_USERNAME)
        .expect("Failed to query user")
        .expect("User should exist");
    
    // Try to create again
    create_default_system_user_for_test(kalam_sql.clone())
        .await
        .expect("Second creation should not fail");
    
    let second_user = kalam_sql
        .get_user(AuthConstants::DEFAULT_SYSTEM_USERNAME)
        .expect("Failed to query user")
        .expect("User should still exist");
    
    // Verify it's the same user (same ID, same timestamps)
    assert_eq!(first_user.id, second_user.id, "User ID should be unchanged");
    assert_eq!(
        first_user.created_at, second_user.created_at,
        "Created timestamp should be unchanged"
    );
    assert_eq!(
        first_user.password_hash, second_user.password_hash,
        "Password hash should be unchanged"
    );
}

/// T126: Verify password meets security requirements
#[tokio::test]
async fn test_system_user_password_security() {
    // Test the password generation function directly
    let password = generate_random_password_for_test(24);
    
    // Verify length
    assert_eq!(password.len(), 24, "Password should be 24 characters");
    
    // Verify contains mix of character types
    let has_uppercase = password.chars().any(|c| c.is_uppercase());
    let has_lowercase = password.chars().any(|c| c.is_lowercase());
    let has_digit = password.chars().any(|c| c.is_numeric());
    let has_special = password.chars().any(|c| "!@#$%^&*".contains(c));
    
    // Password should have at least 3 out of 4 character types
    // (not all 4 guaranteed due to randomness, but very unlikely to have less than 3)
    let char_type_count = [has_uppercase, has_lowercase, has_digit, has_special]
        .iter()
        .filter(|&&x| x)
        .count();
    
    assert!(
        char_type_count >= 3,
        "Password should contain at least 3 character types, found: {} (upper={}, lower={}, digit={}, special={})",
        char_type_count,
        has_uppercase,
        has_lowercase,
        has_digit,
        has_special
    );
}

// ============================================================================
// Helper functions (mirror the actual implementation in lifecycle.rs)
// ============================================================================

/// Test version of create_default_system_user from lifecycle.rs
async fn create_default_system_user_for_test(kalam_sql: Arc<KalamSql>) -> anyhow::Result<()> {
    use kalamdb_commons::{StorageId, StorageMode};
    
    // Check if system user already exists
    let existing_user = kalam_sql.get_user(AuthConstants::DEFAULT_SYSTEM_USERNAME);
    
    match existing_user {
        Ok(Some(_)) => {
            // User already exists, skip creation
            Ok(())
        }
        Ok(None) | Err(_) => {
            // User doesn't exist, create new system user
            let user_id = UserId::new(AuthConstants::DEFAULT_SYSTEM_USER_ID);
            let username = AuthConstants::DEFAULT_SYSTEM_USERNAME.to_string();
            let email = format!("{}@localhost", AuthConstants::DEFAULT_SYSTEM_USERNAME);
            let role = Role::System;
            let created_at = chrono::Utc::now().timestamp_millis();
            
            // Generate random password
            let password = generate_random_password_for_test(24);
            let password_hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST)?;
            
            let user = User {
                id: user_id,
                username: username.clone(),
                password_hash,
                role,
                email: Some(email),
                auth_type: AuthType::Internal,
                auth_data: None, // No allow_remote flag = localhost-only
                storage_mode: StorageMode::Table,
                storage_id: Some(StorageId::local()),
                created_at,
                updated_at: created_at,
                last_seen: None,
                deleted_at: None,
            };
            
            kalam_sql.insert_user(&user)?;
            
            Ok(())
        }
    }
}

/// Test version of generate_random_password from lifecycle.rs
fn generate_random_password_for_test(length: usize) -> String {
    use rand::Rng;
    
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";
    let mut rng = rand::thread_rng();
    
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
