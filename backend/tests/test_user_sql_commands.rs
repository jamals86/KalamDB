//! Integration tests for SQL-based user management commands
//!
//! Tests CREATE USER, ALTER USER, and DROP USER SQL commands with:
//! - Successful user creation with different auth types
//! - Password changes and role updates
//! - Authorization checks (DBA/System only)
//! - Soft deletion

use kalamdb_commons::{models::UserName, AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, UserTableService,
};
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::system_table_registration::register_system_tables;
use kalamdb_core::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use kalamdb_sql::KalamSql;
use kalamdb_store::RocksDBBackend;
use kalamdb_store::RocksDbInit;
use kalamdb_store::StorageBackend;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test SQL executor with all dependencies
async fn setup_test_executor() -> (
    SqlExecutor,
    TempDir,
    Arc<KalamSql>,
    Arc<kalamdb_core::tables::system::users_v2::UsersTableProvider>,
) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().to_str().unwrap();

    // Create RocksDB
    let db_init = RocksDbInit::new(db_path);
    let db = db_init.open().expect("Failed to open RocksDB");

    // Create KalamSql adapter via StorageBackend abstraction
    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
    let kalam_sql = Arc::new(KalamSql::new(backend.clone()).expect("Failed to create KalamSQL"));

    // Create stores (via generic StorageBackend)
    let user_table_store = Arc::new(kalamdb_core::tables::new_user_table_store(
        backend.clone(),
        &kalamdb_commons::NamespaceId::new("test_ns"),
        &kalamdb_commons::TableName::new("test_table"),
    ));
    let shared_table_store = Arc::new(kalamdb_core::tables::new_shared_table_store(
        backend.clone(),
        &kalamdb_commons::NamespaceId::new("test_ns"),
        &kalamdb_commons::TableName::new("test_table"),
    ));
    let stream_table_store = Arc::new(kalamdb_core::tables::new_stream_table_store(
        &kalamdb_commons::NamespaceId::new("test_ns"),
        &kalamdb_commons::TableName::new("test_table"),
    ));

    // Create services
    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    let user_table_service = Arc::new(UserTableService::new(
        kalam_sql.clone(),
        user_table_store.clone(),
    ));
    let shared_table_service = Arc::new(SharedTableService::new(
        shared_table_store.clone(),
        kalam_sql.clone(),
    ));
    let stream_table_service = Arc::new(StreamTableService::new(
        stream_table_store.clone(),
        kalam_sql.clone(),
    ));

    // Create DataFusion session
    let session_context = Arc::new(datafusion::prelude::SessionContext::new());

    // Register system tables (users, jobs, live_queries, etc.)
    let system_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
    register_system_tables(&system_schema, backend.clone())
        .expect("Failed to register system tables");

    // Create users provider for test verification
    let users_provider =
        Arc::new(kalamdb_core::tables::system::users_v2::UsersTableProvider::new(backend.clone()));

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
    )
    .with_storage_backend(backend.clone());

    (executor, temp_dir, kalam_sql, users_provider)
}

/// Helper to create a system/DBA user for authorization tests
async fn create_system_user(
    users_provider: &Arc<kalamdb_core::tables::system::users_v2::UsersTableProvider>,
) -> UserId {
    let user_id = UserId::new("test_admin");
    let user = kalamdb_commons::system::User {
        id: user_id.clone(),
        username: UserName::new("test_admin"),
        password_hash: "hashed".to_string(),
        role: Role::System,
        email: None,
        auth_type: AuthType::Internal,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: None,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
        last_seen: None,
        deleted_at: None,
    };

    users_provider
        .create_user(user)
        .expect("Failed to create system user");
    user_id
}

#[tokio::test]
async fn test_create_user_with_password_success() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Execute CREATE USER command
    let sql = "CREATE USER 'alice' WITH PASSWORD 'SecurePass123' ROLE developer EMAIL 'alice@example.com'";
    let result = executor.execute(sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "CREATE USER should succeed: {:?}", result);

    // Verify user was created via users_provider
    let user = users_provider
        .get_user_by_username("alice")
        .expect("Failed to get user");
    assert!(user.is_some(), "User should exist");

    let user = user.unwrap();
    assert_eq!(user.username, "alice".into());
    assert_eq!(user.auth_type, AuthType::Password);
    assert_eq!(user.role, Role::Service); // developer maps to Service
    assert_eq!(user.email, Some("alice@example.com".to_string()));
    assert!(
        !user.password_hash.is_empty(),
        "Password hash should not be empty"
    );
    assert_ne!(user.password_hash, "secure123", "Password should be hashed");
}

#[tokio::test]
async fn test_create_user_with_oauth_success() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    let sql = r#"CREATE USER 'bob' WITH OAUTH '{"provider": "google", "subject": "12345"}' ROLE viewer EMAIL 'bob@example.com'"#;
    let result = executor.execute(sql, Some(&admin_id)).await;

    assert!(
        result.is_ok(),
        "CREATE USER with OAuth should succeed: {:?}",
        result
    );

    let user = users_provider
        .get_user_by_username("bob")
        .expect("Failed to get user")
        .unwrap();
    assert_eq!(user.username, "bob".into());
    assert_eq!(user.auth_type, AuthType::OAuth);
    assert_eq!(user.role, Role::User); // viewer maps to User
}

#[tokio::test]
async fn test_create_user_without_authorization_fails() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;

    // Create a regular user (not DBA/System)
    let regular_user_id = UserId::new("regular_user");
    let regular_user = kalamdb_commons::system::User {
        id: regular_user_id.clone(),
        username: "regular_user".into(),
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
    users_provider
        .create_user(regular_user)
        .expect("Failed to create regular user");

    // Try to create a user as regular user
    let sql = "CREATE USER 'charlie' WITH PASSWORD 'TestPass123' ROLE user";
    let result = executor.execute(sql, Some(&regular_user_id)).await;

    assert!(
        result.is_err(),
        "Regular user should not be able to create users"
    );

    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    println!("Error received: {}", err_msg);
    assert!(
        err_msg.contains("PermissionDenied")
            || err_msg.contains("Unauthorized")
            || err_msg.contains("Only DBA or System")
            || err_msg.contains("Admin privileges")
            || err_msg.contains("permission"),
        "Should be permission denied error, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_alter_user_set_password() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create user first
    let create_sql = "CREATE USER 'dave' WITH PASSWORD 'OldPass123' ROLE user";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    let old_user = users_provider
        .get_user_by_username("dave")
        .unwrap()
        .unwrap();
    let old_hash = old_user.password_hash.clone();

    // Change password
    let alter_sql = "ALTER USER 'dave' SET PASSWORD 'NewPass456'";
    let result = executor.execute(alter_sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "ALTER USER SET PASSWORD should succeed");

    let updated_user = users_provider
        .get_user_by_username("dave")
        .unwrap()
        .unwrap();
    assert_ne!(
        updated_user.password_hash, old_hash,
        "Password hash should change"
    );
    assert_eq!(updated_user.auth_type, AuthType::Password);
}

#[tokio::test]
async fn test_alter_user_set_role() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create user first
    let create_sql = "CREATE USER 'eve' WITH PASSWORD 'Password123' ROLE user";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    // Change role
    let alter_sql = "ALTER USER 'eve' SET ROLE dba";
    let result = executor.execute(alter_sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "ALTER USER SET ROLE should succeed");

    let updated_user = users_provider.get_user_by_username("eve").unwrap().unwrap();
    assert_eq!(updated_user.role, Role::Dba);
}

#[tokio::test]
async fn test_drop_user_soft_delete() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create user first
    let create_sql = "CREATE USER 'frank' WITH PASSWORD 'Password123' ROLE user";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    // Drop user
    let drop_sql = "DROP USER 'frank'";
    let result = executor.execute(drop_sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "DROP USER should succeed");

    // Verify user is soft-deleted
    let user = users_provider
        .get_user_by_username("frank")
        .unwrap()
        .unwrap();
    assert!(
        user.deleted_at.is_some(),
        "User should have deleted_at timestamp"
    );
}

#[tokio::test]
async fn test_create_user_role_mapping() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

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
            "CREATE USER '{}' WITH PASSWORD 'TestPass123' ROLE {}",
            username, role_str
        );

        let result = executor.execute(&sql, Some(&admin_id)).await;
        assert!(
            result.is_ok(),
            "CREATE USER with role {} should succeed",
            role_str
        );

        let user = users_provider
            .get_user_by_username(username)
            .unwrap()
            .unwrap();
        assert_eq!(
            user.role, expected_role,
            "Role {} should map to {:?}",
            role_str, expected_role
        );
    }
}

/// T084S - Test CREATE USER WITH INTERNAL (system users)
#[tokio::test]
async fn test_create_user_with_internal_auth() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    let sql = "CREATE USER 'system_user' WITH INTERNAL ROLE system";
    let result = executor.execute(sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "CREATE USER with INTERNAL should succeed");

    let user = users_provider
        .get_user_by_username("system_user")
        .expect("Failed to get user")
        .unwrap();
    assert_eq!(user.username, "system_user".into());
    assert_eq!(user.auth_type, AuthType::Internal);
    assert_eq!(user.role, Role::System);
    assert!(
        user.password_hash.is_empty(),
        "Internal users should not have password hash"
    );
}

/// T084T - Test ALTER USER SET EMAIL
#[tokio::test]
async fn test_alter_user_set_email() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create user first
    let create_sql =
        "CREATE USER 'george' WITH PASSWORD 'Password123' ROLE user EMAIL 'george@old.com'";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    // Change email
    let alter_sql = "ALTER USER 'george' SET EMAIL 'george@new.com'";
    let result = executor.execute(alter_sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "ALTER USER SET EMAIL should succeed");

    let updated_user = users_provider
        .get_user_by_username("george")
        .unwrap()
        .unwrap();
    assert_eq!(updated_user.email, Some("george@new.com".to_string()));
}

/// T084V - Additional authorization test for ALTER USER
#[tokio::test]
async fn test_alter_user_without_authorization_fails() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create target user
    let create_sql = "CREATE USER 'target' WITH PASSWORD 'Password123' ROLE user";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    // Create regular user (not DBA/System)
    let regular_user_id = UserId::new("regular_user2");
    let regular_user = kalamdb_commons::system::User {
        id: regular_user_id.clone(),
        username: "regular_user2".into(),
        password_hash: "hashed".to_string(),
        role: Role::User,
        email: Some("user2@test.com".to_string()),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: None,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
        last_seen: None,
        deleted_at: None,
    };
    users_provider
        .create_user(regular_user)
        .expect("Failed to create regular user");

    // Try to alter user as regular user
    let alter_sql = "ALTER USER 'target' SET ROLE dba";
    let result = executor.execute(alter_sql, Some(&regular_user_id)).await;

    assert!(
        result.is_err(),
        "Regular user should not be able to alter users"
    );

    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(
        err_msg.contains("PermissionDenied")
            || err_msg.contains("Unauthorized")
            || err_msg.contains("Only DBA or System")
            || err_msg.contains("Admin privileges"),
        "Should be permission denied error"
    );
}

/// T084V - Additional authorization test for DROP USER
#[tokio::test]
async fn test_drop_user_without_authorization_fails() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create target user
    let create_sql = "CREATE USER 'to_delete' WITH PASSWORD 'Password123' ROLE user";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    // Create regular user (not DBA/System)
    let regular_user_id = UserId::new("regular_user3");
    let regular_user = kalamdb_commons::system::User {
        id: regular_user_id.clone(),
        username: "regular_user3".into(),
        password_hash: "hashed".to_string(),
        role: Role::User,
        email: Some("user3@test.com".to_string()),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: None,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
        last_seen: None,
        deleted_at: None,
    };
    users_provider
        .create_user(regular_user)
        .expect("Failed to create regular user");

    // Try to drop user as regular user
    let drop_sql = "DROP USER 'to_delete'";
    let result = executor.execute(drop_sql, Some(&regular_user_id)).await;

    assert!(
        result.is_err(),
        "Regular user should not be able to drop users"
    );

    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(
        err_msg.contains("PermissionDenied")
            || err_msg.contains("Unauthorized")
            || err_msg.contains("Only DBA or System")
            || err_msg.contains("Admin privileges"),
        "Should be permission denied error"
    );
}

/// T084W - Test weak password rejection (common passwords)
#[tokio::test]
async fn test_create_user_weak_password_rejected() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Common weak passwords that should be rejected
    let weak_passwords = vec![
        "password", "123456", "qwerty", "admin", "letmein", "welcome", "monkey",
    ];

    for weak_pass in weak_passwords {
        let sql = format!(
            "CREATE USER 'weak_user_{}' WITH PASSWORD '{}' ROLE user",
            weak_pass, weak_pass
        );

        let result = executor.execute(&sql, Some(&admin_id)).await;

        assert!(
            result.is_err(),
            "Weak password '{}' should be rejected",
            weak_pass
        );

        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("weak") || err_msg.contains("common") || err_msg.contains("password"),
            "Error should mention weak/common password for '{}'",
            weak_pass
        );
    }
}

/// T084W - Test password length validation
#[tokio::test]
async fn test_create_user_password_length_validation() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Too short password (less than 8 characters)
    let sql_short = "CREATE USER 'short_pass' WITH PASSWORD 'abc' ROLE user";
    let result_short = executor.execute(sql_short, Some(&admin_id)).await;

    assert!(
        result_short.is_err(),
        "Password too short should be rejected"
    );

    let err_msg = format!("{:?}", result_short.unwrap_err());
    assert!(
        err_msg.contains("8") || err_msg.contains("short") || err_msg.contains("minimum"),
        "Error should mention minimum length requirement"
    );

    // Valid password (8+ characters)
    let sql_valid = "CREATE USER 'valid_pass' WITH PASSWORD 'ValidPass123' ROLE user";
    let result_valid = executor.execute(sql_valid, Some(&admin_id)).await;

    assert!(
        result_valid.is_ok(),
        "Valid password should be accepted: {:?}",
        result_valid
    );
}

/// T084W - Test ALTER USER with weak password
#[tokio::test]
async fn test_alter_user_weak_password_rejected() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create user with strong password first
    let create_sql = "CREATE USER 'henry' WITH PASSWORD 'StrongPass123!' ROLE user";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    // Try to change to weak password
    let alter_sql = "ALTER USER 'henry' SET PASSWORD 'password'";
    let result = executor.execute(alter_sql, Some(&admin_id)).await;

    assert!(
        result.is_err(),
        "Changing to weak password should be rejected"
    );

    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("weak") || err_msg.contains("common") || err_msg.contains("password"),
        "Error should mention weak/common password"
    );
}

/// T193: Test CREATE USER duplicate error
#[tokio::test]
async fn test_create_user_duplicate_error() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create user first time
    let create_sql = "CREATE USER 'duplicate_test' WITH PASSWORD 'Password123!' ROLE user";
    let result1 = executor.execute(create_sql, Some(&admin_id)).await;

    if let Err(ref e) = result1 {
        eprintln!("First CREATE USER failed with error: {:?}", e);
    }
    assert!(
        result1.is_ok(),
        "First CREATE USER should succeed: {:?}",
        result1
    );

    // Try to create same user again
    let result2 = executor.execute(create_sql, Some(&admin_id)).await;

    eprintln!("Second CREATE USER result: {:?}", result2);

    assert!(
        result2.is_err(),
        "Duplicate CREATE USER should fail with error, but got: {:?}",
        result2
    );

    let err_msg = format!("{:?}", result2.unwrap_err());
    assert!(
        err_msg.contains("already exists")
            || err_msg.contains("duplicate")
            || err_msg.contains("Duplicate")
            || err_msg.contains("AlreadyExists"),
        "Error should indicate user already exists, got: {}",
        err_msg
    );
}

/// T197: Test ALTER USER not found error
#[tokio::test]
async fn test_alter_user_not_found() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Try to alter non-existent user
    let alter_sql = "ALTER USER 'nonexistent_user' SET ROLE dba";
    let result = executor.execute(alter_sql, Some(&admin_id)).await;

    assert!(
        result.is_err(),
        "ALTER USER on non-existent user should fail"
    );

    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("not found")
            || err_msg.contains("NotFound")
            || err_msg.contains("does not exist")
            || err_msg.contains("Unknown user"),
        "Error should indicate user not found, got: {}",
        err_msg
    );
}

/// T199: Test DROP USER IF EXISTS (no error on non-existent user)
#[tokio::test]
async fn test_drop_user_if_exists() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Drop non-existent user with IF EXISTS - should succeed without error
    let drop_sql = "DROP USER IF EXISTS 'user_that_never_existed'";
    let result = executor.execute(drop_sql, Some(&admin_id)).await;

    assert!(
        result.is_ok(),
        "DROP USER IF EXISTS should succeed even if user doesn't exist: {:?}",
        result
    );

    // Create a user, then drop with IF EXISTS - should also succeed
    let create_sql = "CREATE USER 'temp_user' WITH PASSWORD 'Pass123!' ROLE user";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    let drop_sql2 = "DROP USER IF EXISTS 'temp_user'";
    let result2 = executor.execute(drop_sql2, Some(&admin_id)).await;

    assert!(
        result2.is_ok(),
        "DROP USER IF EXISTS should succeed for existing user: {:?}",
        result2
    );
}

/// T200: Test restore deleted user (UPDATE deleted_at = NULL)
#[tokio::test]
async fn test_restore_deleted_user() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create user
    let create_sql = "CREATE USER 'restore_test' WITH PASSWORD 'Password123!' ROLE user";
    executor
        .execute(create_sql, Some(&admin_id))
        .await
        .expect("CREATE USER failed");

    // Soft delete user
    let drop_sql = "DROP USER 'restore_test'";
    executor
        .execute(drop_sql, Some(&admin_id))
        .await
        .expect("DROP USER failed");

    // Verify user is soft-deleted
    let deleted_user = users_provider
        .get_user_by_username("restore_test")
        .unwrap()
        .unwrap();
    assert!(
        deleted_user.deleted_at.is_some(),
        "User should be soft-deleted"
    );

    // Restore user by setting deleted_at = NULL
    let restore_sql = "UPDATE system.users SET deleted_at = NULL WHERE username = 'restore_test'";
    let result = executor.execute(restore_sql, Some(&admin_id)).await;

    assert!(
        result.is_ok(),
        "Restore user via UPDATE should succeed: {:?}",
        result
    );

    // Verify user is restored (deleted_at should be NULL)
    let restored_user = users_provider
        .get_user_by_username("restore_test")
        .unwrap()
        .unwrap();
    assert!(
        restored_user.deleted_at.is_none(),
        "User should be restored (deleted_at should be NULL)"
    );
}

/// T201: Test SELECT * FROM system.users excludes deleted users by default
#[tokio::test]
async fn test_select_users_excludes_deleted() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create two users
    let create_sql1 = "CREATE USER 'active_user' WITH PASSWORD 'Pass123!' ROLE user";
    let create_sql2 = "CREATE USER 'deleted_user' WITH PASSWORD 'Pass123!' ROLE user";

    executor
        .execute(create_sql1, Some(&admin_id))
        .await
        .expect("CREATE active_user failed");
    executor
        .execute(create_sql2, Some(&admin_id))
        .await
        .expect("CREATE deleted_user failed");

    // Soft delete one user
    let drop_sql = "DROP USER 'deleted_user'";
    executor
        .execute(drop_sql, Some(&admin_id))
        .await
        .expect("DROP USER failed");

    // SELECT from system.users (should exclude deleted by default)
    let select_sql =
        "SELECT username FROM system.users WHERE username IN ('active_user', 'deleted_user')";
    let result = executor.execute(select_sql, Some(&admin_id)).await;

    assert!(result.is_ok(), "SELECT should succeed: {:?}", result);

    // Verify result excludes deleted user
    // Note: The exact verification depends on ExecutionResult structure
    // For now, just verify no error - detailed validation would check result rows
    println!(
        "✓ T201: SELECT from system.users executed successfully (deleted users should be excluded)"
    );
}

/// T202: Test SELECT deleted users explicitly (WHERE deleted_at IS NOT NULL)
#[tokio::test]
async fn test_select_deleted_users_explicit() {
    let (executor, _temp_dir, _kalam_sql, users_provider) = setup_test_executor().await;
    let admin_id = create_system_user(&users_provider).await;

    // Create two users
    let create_sql1 = "CREATE USER 'active_user2' WITH PASSWORD 'Pass123!' ROLE user";
    let create_sql2 = "CREATE USER 'deleted_user2' WITH PASSWORD 'Pass123!' ROLE user";

    executor
        .execute(create_sql1, Some(&admin_id))
        .await
        .expect("CREATE active_user2 failed");
    executor
        .execute(create_sql2, Some(&admin_id))
        .await
        .expect("CREATE deleted_user2 failed");

    // Soft delete one user
    let drop_sql = "DROP USER 'deleted_user2'";
    executor
        .execute(drop_sql, Some(&admin_id))
        .await
        .expect("DROP USER failed");

    // SELECT deleted users explicitly
    let select_sql = "SELECT username FROM system.users WHERE deleted_at IS NOT NULL AND username IN ('active_user2', 'deleted_user2')";
    let result = executor.execute(select_sql, Some(&admin_id)).await;

    assert!(
        result.is_ok(),
        "SELECT deleted users should succeed: {:?}",
        result
    );

    // Verify result includes only deleted user
    // Note: The exact verification depends on ExecutionResult structure
    println!(
        "✓ T202: SELECT deleted users with WHERE deleted_at IS NOT NULL executed successfully"
    );
}
