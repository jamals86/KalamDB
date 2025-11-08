#![allow(unused_imports)]
//! Tests for password complexity enforcement.

use kalamdb_commons::{models::UserName, AuthType, NodeId, Role, StorageId, StorageMode, UserId};
use kalamdb_core::app_context::AppContext;
use kalamdb_core::error::KalamDbError;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_store::{RocksDBBackend, RocksDbInit, StorageBackend};
use std::sync::Arc;
use tempfile::TempDir;

async fn setup_executor(enforce_complexity: bool) -> (SqlExecutor, TempDir, Arc<AppContext>, Arc<datafusion::prelude::SessionContext>) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().to_str().unwrap();

    let db_init = RocksDbInit::new(db_path);
    let db = db_init.open().expect("Failed to open RocksDB");

    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
    
    // Create minimal test config
    let mut test_config = kalamdb_commons::config::ServerConfig::default();
    test_config.server.node_id = "test-node".to_string();
    test_config.storage.default_storage_path = temp_dir.path().join("storage").to_str().unwrap().to_string();
    
    // Initialize AppContext with config
    let app_context = AppContext::init(
        backend.clone(),
        NodeId::new("test-node".to_string()),
        temp_dir.path().join("storage").to_str().unwrap().to_string(),
        test_config,
    );
    
    let session_context = app_context.base_session_context();
    
    // Create SqlExecutor with password complexity setting
    let executor = SqlExecutor::new(
        app_context.clone(),
        enforce_complexity,
    );

    (executor, temp_dir, app_context, session_context)
}

async fn create_admin_user(app_context: &Arc<AppContext>) -> UserId {
    use kalamdb_commons::types::User;
    let user_id = UserId::new("complexity_admin");
    let now = chrono::Utc::now().timestamp_millis();

    let user = User {
        id: user_id.clone(),
        username: UserName::new("complexity_admin"),
        password_hash: "hashed".to_string(),
        role: Role::System,
        email: Some("complexity@kalamdb.local".to_string()),
        auth_type: AuthType::Internal,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: Some(StorageId::local()),
        created_at: now,
        updated_at: now,
        last_seen: None,
        deleted_at: None,
    };

    app_context
        .system_tables()
        .users()
        .create_user(user)
        .expect("Failed to insert admin user");
    user_id
}

fn assert_complexity_error(
    result: Result<kalamdb_core::sql::executor::ExecutorResultAlias, KalamDbError>,
    expected_fragment: &str,
) {
    match result {
        Err(KalamDbError::InvalidOperation(msg)) => {
            assert!(
                msg.contains(expected_fragment),
                "Expected error to contain '{}', got '{}'",
                expected_fragment,
                msg
            );
        }
        other => panic!("Expected InvalidOperation error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_complexity_uppercase_required() {
    let (executor, _temp_dir, app_context, session_ctx) = setup_executor(true).await;
    let admin_id = create_admin_user(&app_context).await;
    let exec_ctx = kalamdb_core::sql::executor::ExecutorContextAlias::new(admin_id.clone(), Role::System, session_ctx.clone());

    let result = executor
        .execute(&*session_ctx, 
            "CREATE USER 'no_upper' WITH PASSWORD 'lowercase1!' ROLE user",
            &exec_ctx,
            Vec::new(),
        )
        .await;

    assert_complexity_error(result, "uppercase");
}

#[tokio::test]
async fn test_complexity_lowercase_required() {
    let (executor, _temp_dir, app_context, session_ctx) = setup_executor(true).await;
    let admin_id = create_admin_user(&app_context).await;
    let exec_ctx = kalamdb_core::sql::executor::ExecutorContextAlias::new(admin_id.clone(), Role::System, session_ctx.clone());

    let result = executor
        .execute(&*session_ctx, 
            "CREATE USER 'no_lower' WITH PASSWORD 'UPPERCASE1!' ROLE user",
            &exec_ctx,
            Vec::new(),
        )
        .await;

    assert_complexity_error(result, "lowercase");
}

#[tokio::test]
async fn test_complexity_digit_required() {
    let (executor, _temp_dir, app_context, session_ctx) = setup_executor(true).await;
    let admin_id = create_admin_user(&app_context).await;
    let exec_ctx = kalamdb_core::sql::executor::ExecutorContextAlias::new(admin_id.clone(), Role::System, session_ctx.clone());

    let result = executor
        .execute(&*session_ctx, 
            "CREATE USER 'no_digit' WITH PASSWORD 'NoDigits!' ROLE user",
            &exec_ctx,
            Vec::new(),
        )
        .await;

    assert_complexity_error(result, "digit");
}

#[tokio::test]
async fn test_complexity_special_required() {
    let (executor, _temp_dir, app_context, session_ctx) = setup_executor(true).await;
    let admin_id = create_admin_user(&app_context).await;
    let exec_ctx = kalamdb_core::sql::executor::ExecutorContextAlias::new(admin_id.clone(), Role::System, session_ctx.clone());

    let result = executor
        .execute(&*session_ctx, 
            "CREATE USER 'no_special' WITH PASSWORD 'NoSpecial1' ROLE user",
            &exec_ctx,
            Vec::new(),
        )
        .await;

    assert_complexity_error(result, "special character");
}

#[tokio::test]
async fn test_complexity_valid_password_succeeds() {
    let (executor, _temp_dir, app_context, session_ctx) = setup_executor(true).await;
    let admin_id = create_admin_user(&app_context).await;
    let exec_ctx = kalamdb_core::sql::executor::ExecutorContextAlias::new(admin_id.clone(), Role::System, session_ctx.clone());

    let result = executor
        .execute(&*session_ctx, 
            "CREATE USER 'valid_complex' WITH PASSWORD 'ValidPass1!' ROLE user",
            &exec_ctx,
            Vec::new(),
        )
        .await;

    assert!(result.is_ok(), "Valid password should be accepted");
}

#[tokio::test]
async fn test_complexity_disabled_allows_simple_password() {
    let (executor, _temp_dir, app_context, session_ctx) = setup_executor(false).await;
    let admin_id = create_admin_user(&app_context).await;
    let exec_ctx = kalamdb_core::sql::executor::ExecutorContextAlias::new(admin_id.clone(), Role::System, session_ctx.clone());

    let result = executor
        .execute(&*session_ctx, 
            "CREATE USER 'simple_allowed' WITH PASSWORD 'Simplepass1' ROLE user",
            &exec_ctx,
            Vec::new(),
        )
        .await;

    assert!(
        result.is_ok(),
        "Password complexity disabled should allow simple passwords: {:?}",
        result
    );
}

#[tokio::test]
async fn test_complexity_alter_user_requires_special_character() {
    let (executor, _temp_dir, app_context, session_ctx) = setup_executor(true).await;
    let admin_id = create_admin_user(&app_context).await;
    let exec_ctx = kalamdb_core::sql::executor::ExecutorContextAlias::new(admin_id.clone(), Role::System, session_ctx.clone());

    executor
        .execute(&*session_ctx, 
            "CREATE USER 'alter_target' WITH PASSWORD 'ValidPass1!' ROLE user",
            &exec_ctx,
            Vec::new(),
        )
        .await
        .expect("Initial CREATE USER should succeed");

    let result = executor
        .execute(&*session_ctx, 
            "ALTER USER 'alter_target' SET PASSWORD 'NoSpecial2'",
            &exec_ctx,
            Vec::new(),
        )
        .await;

    assert_complexity_error(result, "special character");
}

#[tokio::test]
async fn test_complexity_alter_user_valid_password_succeeds() {
    let (executor, _temp_dir, app_context, session_ctx) = setup_executor(true).await;
    let admin_id = create_admin_user(&app_context).await;
    let exec_ctx = kalamdb_core::sql::executor::ExecutorContextAlias::new(admin_id.clone(), Role::System, session_ctx.clone());

    executor
        .execute(&*session_ctx, 
            "CREATE USER 'alter_target_ok' WITH PASSWORD 'ValidPass1!' ROLE user",
            &exec_ctx,
            Vec::new(),
        )
        .await
        .expect("Initial CREATE USER should succeed");

    let result = executor
        .execute(&*session_ctx, 
            "ALTER USER 'alter_target_ok' SET PASSWORD 'AnotherPass2@'",
            &exec_ctx,
            Vec::new(),
        )
        .await;

    assert!(
        result.is_ok(),
        "ALTER USER with complex password should succeed: {:?}",
        result
    );
}
