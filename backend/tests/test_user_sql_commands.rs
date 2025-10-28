//! Integration tests for SQL-based user management commands
//!
//! Tests CREATE USER, ALTER USER, and DROP USER SQL commands with:
//! - Successful user creation with different auth types
//! - Password changes and role updates
//! - Authorization checks (DBA/System only)
//! - Soft deletion

use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, UserTableService,
};
use kalamdb_store::RocksDbInit;
use kalamdb_sql::KalamSql;
use kalamdb_store::{UserTableStore, SharedTableStore, StreamTableStore, RocksDBBackend};
use kalamdb_store::storage_trait::StorageBackend;
use kalamdb_commons::{AuthType, NamespaceId, Role, StorageMode, UserId};
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test SQL executor with all dependencies
async fn setup_test_executor() -> (SqlExecutor, TempDir, Arc<KalamSql>) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().to_str().unwrap();

    // Create RocksDB
    let db_init = RocksDbInit::new(db_path);
    let db = db_init.open().expect("Failed to open RocksDB");
    
    // Create KalamSql adapter via StorageBackend abstraction
    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
    let kalam_sql = Arc::new(KalamSql::new(backend).expect("Failed to create KalamSQL"));

    // Create stores
    let user_table_store = Arc::new(UserTableStore::new(db.clone()).expect("Failed to create user table store"));
    let shared_table_store = Arc::new(SharedTableStore::new(db.clone()).expect("Failed to create shared table store"));
    let stream_table_store = Arc::new(StreamTableStore::new(db.clone()).expect("Failed to create stream table store"));

    // Create services
    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    let user_table_service = Arc::new(UserTableService::new(kalam_sql.clone(), user_table_store.clone()));
    let shared_table_service = Arc::new(SharedTableService::new(shared_table_store.clone(), kalam_sql.clone()));
    let stream_table_service = Arc::new(StreamTableService::new(stream_table_store.clone(), kalam_sql.clone()));

    // Create DataFusion session
    let session_context = Arc::new(datafusion::prelude::SessionContext::new());

    // Create executor
    let executor = SqlExecutor::new(
        namespace_service,
        session_context,
        user_table_service,
        shared_table_service,
        stream_table_service,
    )
    .with_stores(
        user_table_store,
        shared_table_store,
        stream_table_store,
        kalam_sql.clone(),
    );

    (executor, temp_dir, kalam_sql)
}

/// Helper to create a system/DBA user for authorization tests
async fn create_system_user(kalam_sql: &Arc<KalamSql>) -> UserId {
    let user_id = UserId::new("test_admin");
    let user = kalamdb_commons::system::User {
        id: user_id.clone(),
        username: "test_admin".to_string(),
        password_hash: "hashed".to_string(),
        role: Role::System,
        email: Some("admin@test.com".to_string()),
        auth_type: AuthType::Internal,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: None,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
        last_seen: None,
        deleted_at: None,
    };
    
    kalam_sql.insert_user(&user).expect("Failed to create system user");
    user_id
}

#[tokio::test]
async fn test_create_user_with_password_success() {
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    // Execute CREATE USER command
    let sql = "CREATE USER 'alice' WITH PASSWORD 'secure123' ROLE developer EMAIL 'alice@example.com'";
    let result = executor.execute(sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "CREATE USER should succeed: {:?}", result);
    
    // Verify user was created
    let user = kalam_sql.get_user("alice").expect("Failed to get user");
    assert!(user.is_some(), "User should exist");
    
    let user = user.unwrap();
    assert_eq!(user.username, "alice");
    assert_eq!(user.auth_type, AuthType::Password);
    assert_eq!(user.role, Role::Service); // developer maps to Service
    assert_eq!(user.email, Some("alice@example.com".to_string()));
    assert!(!user.password_hash.is_empty(), "Password hash should not be empty");
    assert_ne!(user.password_hash, "secure123", "Password should be hashed");
}

#[tokio::test]
async fn test_create_user_with_oauth_success() {
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    let sql = "CREATE USER 'bob' WITH OAUTH ROLE viewer EMAIL 'bob@example.com'";
    let result = executor.execute(sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "CREATE USER with OAuth should succeed");
    
    let user = kalam_sql.get_user("bob").expect("Failed to get user").unwrap();
    assert_eq!(user.username, "bob");
    assert_eq!(user.auth_type, AuthType::OAuth);
    assert_eq!(user.role, Role::User); // viewer maps to User
}

#[tokio::test]
async fn test_create_user_without_authorization_fails() {
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    
    // Create a regular user (not DBA/System)
    let regular_user_id = UserId::new("regular_user");
    let regular_user = kalamdb_commons::system::User {
        id: regular_user_id.clone(),
        username: "regular_user".to_string(),
        password_hash: "hashed".to_string(),
        role: Role::User, // Regular user role
        email: Some("user@test.com".to_string()),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: None,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
        last_seen: None,
        deleted_at: None,
    };
    kalam_sql.insert_user(&regular_user).expect("Failed to create regular user");

    // Try to create a user as regular user
    let sql = "CREATE USER 'charlie' WITH PASSWORD 'test123' ROLE user";
    let result = executor.execute(sql, Some(&regular_user_id)).await;

    assert!(result.is_err(), "Regular user should not be able to create users");
    
    let err = result.unwrap_err();
    assert!(
        format!("{:?}", err).contains("PermissionDenied") || 
        format!("{:?}", err).contains("Only DBA or System"),
        "Should be permission denied error"
    );
}

#[tokio::test]
async fn test_alter_user_set_password() {
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    // Create user first
    let create_sql = "CREATE USER 'dave' WITH PASSWORD 'oldpass' ROLE user";
    executor.execute(create_sql, Some(&admin_id)).await.expect("CREATE USER failed");
    
    let old_user = kalam_sql.get_user("dave").unwrap().unwrap();
    let old_hash = old_user.password_hash.clone();

    // Change password
    let alter_sql = "ALTER USER 'dave' SET PASSWORD 'newpass'";
    let result = executor.execute(alter_sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "ALTER USER SET PASSWORD should succeed");
    
    let updated_user = kalam_sql.get_user("dave").unwrap().unwrap();
    assert_ne!(updated_user.password_hash, old_hash, "Password hash should change");
    assert_eq!(updated_user.auth_type, AuthType::Password);
}

#[tokio::test]
async fn test_alter_user_set_role() {
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    // Create user first
    let create_sql = "CREATE USER 'eve' WITH PASSWORD 'pass' ROLE user";
    executor.execute(create_sql, Some(&admin_id)).await.expect("CREATE USER failed");

    // Change role
    let alter_sql = "ALTER USER 'eve' SET ROLE dba";
    let result = executor.execute(alter_sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "ALTER USER SET ROLE should succeed");
    
    let updated_user = kalam_sql.get_user("eve").unwrap().unwrap();
    assert_eq!(updated_user.role, Role::Dba);
}

#[tokio::test]
async fn test_drop_user_soft_delete() {
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    // Create user first
    let create_sql = "CREATE USER 'frank' WITH PASSWORD 'pass' ROLE user";
    executor.execute(create_sql, Some(&admin_id)).await.expect("CREATE USER failed");

    // Drop user
    let drop_sql = "DROP USER 'frank'";
    let result = executor.execute(drop_sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "DROP USER should succeed");
    
    // Verify user is soft-deleted
    let user = kalam_sql.get_user("frank").unwrap().unwrap();
    assert!(user.deleted_at.is_some(), "User should have deleted_at timestamp");
}

#[tokio::test]
async fn test_create_user_role_mapping() {
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    // Test various role aliases
    let test_cases = vec![
        ("user1", "dba", Role::Dba),
        ("user2", "admin", Role::Dba),
        ("user3", "developer", Role::Service),
        ("user4", "analyst", Role::Service),
        ("user5", "service", Role::Service),
        ("user6", "viewer", Role::User),
        ("user7", "readonly", Role::User),
        ("user8", "user", Role::User),
        ("user9", "system", Role::System),
    ];

    for (username, role_str, expected_role) in test_cases {
        let sql = format!(
            "CREATE USER '{}' WITH PASSWORD 'pass' ROLE {}",
            username, role_str
        );
        
        let result = executor.execute(&sql, Some(&admin_id)).await;
        assert!(result.is_ok(), "CREATE USER with role {} should succeed", role_str);
        
        let user = kalam_sql.get_user(username).unwrap().unwrap();
        assert_eq!(
            user.role, expected_role,
            "Role {} should map to {:?}",
            role_str, expected_role
        );
    }
}
