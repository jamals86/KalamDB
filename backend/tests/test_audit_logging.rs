//! Integration tests for audit logging.
//!
//! Verifies that privileged operations write entries to `system.audit_log`.

use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, UserTableService,
};
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::sql::DataFusionSessionFactory;
use kalamdb_store::{RocksDBBackend, RocksDbInit, StorageBackend};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tempfile::TempDir;

async fn setup_test_executor() -> (SqlExecutor, TempDir, Arc<KalamSql>) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().to_str().unwrap();

    let db_init = RocksDbInit::new(db_path);
    let db = db_init.open().expect("Failed to open RocksDB");

    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
    let kalam_sql = Arc::new(KalamSql::new(backend.clone()).expect("Failed to create KalamSQL"));

    // Register default 'local' storage
    let storage_base_path = temp_dir.path().join("storage");
    std::fs::create_dir_all(&storage_base_path).expect("Failed to create storage base directory");
    let now = chrono::Utc::now().timestamp_millis();
    let default_storage = kalamdb_sql::Storage {
        storage_id: StorageId::new("local"),
        storage_name: "Local Filesystem".to_string(),
        description: Some("Default local filesystem storage".to_string()),
        storage_type: "filesystem".to_string(),
        base_directory: storage_base_path.to_str().unwrap().to_string(),
        credentials: None,
        shared_tables_template: "shared/{namespace}/{tableName}".to_string(),
        user_tables_template: "users/{userId}/tables/{namespace}/{tableName}".to_string(),
        created_at: now,
        updated_at: now,
    };
    kalam_sql
        .insert_storage(&default_storage)
        .expect("Failed to create default storage");

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

    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    let user_table_service = Arc::new(UserTableService::new());
    let shared_table_service = Arc::new(SharedTableService::new());
    let stream_table_service = Arc::new(StreamTableService::new());

    // Use DataFusionSessionFactory to properly configure "kalam" catalog
    let session_factory =
        DataFusionSessionFactory::new().expect("Failed to create session factory");
    let session_context = Arc::new(session_factory.create_session());

    let executor = SqlExecutor::new(
        namespace_service,
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

    (executor, temp_dir, kalam_sql, session_context)
}

async fn create_system_user(kalam_sql: &Arc<KalamSql>) -> UserId {
    let user_id = UserId::new("audit_admin");
    let now = chrono::Utc::now().timestamp_millis();

    let user = kalamdb_commons::system::User {
        id: user_id.clone(),
        username: "audit_admin".into(),
        password_hash: "hashed".to_string(),
        role: Role::System,
        email: Some("audit@kalamdb.local".to_string()),
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
        .expect("Failed to insert system user");
    user_id
}

fn find_audit_entry<'a>(
    entries: &'a [kalamdb_commons::system::AuditLogEntry],
    action: &str,
    target: &str,
) -> &'a kalamdb_commons::system::AuditLogEntry {
    entries
        .iter()
        .find(|entry| entry.action == action && entry.target == target)
        .unwrap_or_else(|| panic!("Audit entry {} for target {} not found", action, target))
}

#[tokio::test]
async fn test_audit_log_for_user_management() {
    let (executor, _temp_dir, kalam_sql, session_ctx) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    executor
        .execute(
            &session_ctx,
            "CREATE USER 'audit_user' WITH PASSWORD 'StrongPass123!' ROLE user",
            Some(&admin_id),
        )
        .await
        .expect("CREATE USER failed");

    executor
        .execute(&*session_ctx, "ALTER USER 'audit_user' SET ROLE dba", Some(&admin_id))
        .await
        .expect("ALTER USER failed");

    executor
        .execute(&*session_ctx, "DROP USER 'audit_user'", Some(&admin_id))
        .await
        .expect("DROP USER failed");

    let logs = kalam_sql
        .scan_all_audit_logs()
        .expect("Failed to read audit log");

    let create_entry = find_audit_entry(&logs, "user.create", "audit_user");
    assert_eq!(create_entry.actor_user_id, admin_id);
    assert_eq!(create_entry.action, "user.create");
    let create_details: JsonValue =
        serde_json::from_str(create_entry.details.as_ref().unwrap()).unwrap();
    assert_eq!(create_details["role"], "User");
    assert_eq!(create_details["auth_type"], "Password");

    let alter_entry = find_audit_entry(&logs, "user.alter", "audit_user");
    let alter_details: JsonValue =
        serde_json::from_str(alter_entry.details.as_ref().unwrap()).unwrap();
    assert_eq!(alter_details["changes"]["role"], "Dba");

    let drop_entry = find_audit_entry(&logs, "user.drop", "audit_user");
    let drop_details: JsonValue =
        serde_json::from_str(drop_entry.details.as_ref().unwrap()).unwrap();
    assert_eq!(drop_details["soft_deleted"], JsonValue::Bool(true));
}

#[tokio::test]
async fn test_audit_log_for_table_access_change() {
    let (executor, _temp_dir, kalam_sql, session_ctx) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    executor
        .execute(&*session_ctx, "CREATE NAMESPACE analytics", Some(&admin_id))
        .await
        .expect("CREATE NAMESPACE failed");

    executor
        .execute(
            &session_ctx,
            "CREATE SHARED TABLE analytics.events (id INT, value TEXT) ACCESS LEVEL private",
            Some(&admin_id),
        )
        .await
        .expect("CREATE SHARED TABLE failed");

    executor
        .execute(
            &session_ctx,
            "ALTER TABLE analytics.events SET ACCESS LEVEL public",
            Some(&admin_id),
        )
        .await
        .expect("ALTER TABLE failed");

    let logs = kalam_sql
        .scan_all_audit_logs()
        .expect("Failed to read audit log");
    let entry = find_audit_entry(&logs, "table.set_access", "analytics.events");
    let details: JsonValue = serde_json::from_str(entry.details.as_ref().unwrap()).unwrap();
    assert_eq!(details["access_level"], "Public");
}
