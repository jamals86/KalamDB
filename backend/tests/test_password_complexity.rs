//! Tests for password complexity enforcement.

use kalamdb_commons::{models::UserName, AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_core::error::KalamDbError;
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, UserTableService,
};
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use kalamdb_sql::KalamSql;
use kalamdb_store::{RocksDBBackend, RocksDbInit, StorageBackend};
use std::sync::Arc;
use tempfile::TempDir;

async fn setup_executor(enforce_complexity: bool) -> (SqlExecutor, TempDir, Arc<KalamSql>) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().to_str().unwrap();

    let db_init = RocksDbInit::new(db_path);
    let db = db_init.open().expect("Failed to open RocksDB");

    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
    let kalam_sql = Arc::new(KalamSql::new(backend.clone()).expect("Failed to create KalamSQL"));

    let user_table_store = Arc::new(kalamdb_core::tables::new_user_table_store(backend.clone(), &kalamdb_commons::NamespaceId::new("test_ns"), &kalamdb_commons::TableName::new("test_table")));
    let shared_table_store = Arc::new(kalamdb_core::tables::new_shared_table_store(backend.clone(), &kalamdb_commons::NamespaceId::new("test_ns"), &kalamdb_commons::TableName::new("test_table")));
    let stream_table_store = Arc::new(kalamdb_core::tables::new_stream_table_store(&kalamdb_commons::NamespaceId::new("test_ns"), &kalamdb_commons::TableName::new("test_table")));

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

    let session_context = Arc::new(datafusion::prelude::SessionContext::new());

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
    .with_storage_backend(backend.clone())
    .with_password_complexity(enforce_complexity);

    (executor, temp_dir, kalam_sql)
}

async fn create_admin_user(kalam_sql: &Arc<KalamSql>) -> UserId {
    let user_id = UserId::new("complexity_admin");
    let now = chrono::Utc::now().timestamp_millis();

    let user = kalamdb_commons::system::User {
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

    kalam_sql
        .insert_user(&user)
        .expect("Failed to insert admin user");
    user_id
}

fn assert_complexity_error(
    result: Result<kalamdb_core::sql::executor::ExecutionResult, KalamDbError>,
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
    let (executor, _temp_dir, kalam_sql) = setup_executor(true).await;
    let admin_id = create_admin_user(&kalam_sql).await;

    let result = executor
        .execute(
            "CREATE USER 'no_upper' WITH PASSWORD 'lowercase1!' ROLE user",
            Some(&admin_id),
        )
        .await;

    assert_complexity_error(result, "uppercase");
}

#[tokio::test]
async fn test_complexity_lowercase_required() {
    let (executor, _temp_dir, kalam_sql) = setup_executor(true).await;
    let admin_id = create_admin_user(&kalam_sql).await;

    let result = executor
        .execute(
            "CREATE USER 'no_lower' WITH PASSWORD 'UPPERCASE1!' ROLE user",
            Some(&admin_id),
        )
        .await;

    assert_complexity_error(result, "lowercase");
}

#[tokio::test]
async fn test_complexity_digit_required() {
    let (executor, _temp_dir, kalam_sql) = setup_executor(true).await;
    let admin_id = create_admin_user(&kalam_sql).await;

    let result = executor
        .execute(
            "CREATE USER 'no_digit' WITH PASSWORD 'NoDigits!' ROLE user",
            Some(&admin_id),
        )
        .await;

    assert_complexity_error(result, "digit");
}

#[tokio::test]
async fn test_complexity_special_required() {
    let (executor, _temp_dir, kalam_sql) = setup_executor(true).await;
    let admin_id = create_admin_user(&kalam_sql).await;

    let result = executor
        .execute(
            "CREATE USER 'no_special' WITH PASSWORD 'NoSpecial1' ROLE user",
            Some(&admin_id),
        )
        .await;

    assert_complexity_error(result, "special character");
}

#[tokio::test]
async fn test_complexity_valid_password_succeeds() {
    let (executor, _temp_dir, kalam_sql) = setup_executor(true).await;
    let admin_id = create_admin_user(&kalam_sql).await;

    let result = executor
        .execute(
            "CREATE USER 'valid_complex' WITH PASSWORD 'ValidPass1!' ROLE user",
            Some(&admin_id),
        )
        .await;

    assert!(result.is_ok(), "Valid password should be accepted");
}

#[tokio::test]
async fn test_complexity_disabled_allows_simple_password() {
    let (executor, _temp_dir, kalam_sql) = setup_executor(false).await;
    let admin_id = create_admin_user(&kalam_sql).await;

    let result = executor
        .execute(
            "CREATE USER 'simple_allowed' WITH PASSWORD 'Simplepass1' ROLE user",
            Some(&admin_id),
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
    let (executor, _temp_dir, kalam_sql) = setup_executor(true).await;
    let admin_id = create_admin_user(&kalam_sql).await;

    executor
        .execute(
            "CREATE USER 'alter_target' WITH PASSWORD 'ValidPass1!' ROLE user",
            Some(&admin_id),
        )
        .await
        .expect("Initial CREATE USER should succeed");

    let result = executor
        .execute(
            "ALTER USER 'alter_target' SET PASSWORD 'NoSpecial2'",
            Some(&admin_id),
        )
        .await;

    assert_complexity_error(result, "special character");
}

#[tokio::test]
async fn test_complexity_alter_user_valid_password_succeeds() {
    let (executor, _temp_dir, kalam_sql) = setup_executor(true).await;
    let admin_id = create_admin_user(&kalam_sql).await;

    executor
        .execute(
            "CREATE USER 'alter_target_ok' WITH PASSWORD 'ValidPass1!' ROLE user",
            Some(&admin_id),
        )
        .await
        .expect("Initial CREATE USER should succeed");

    let result = executor
        .execute(
            "ALTER USER 'alter_target_ok' SET PASSWORD 'AnotherPass2@'",
            Some(&admin_id),
        )
        .await;

    assert!(
        result.is_ok(),
        "ALTER USER with complex password should succeed: {:?}",
        result
    );
}
