//! Integration tests for audit logging.
//!
//! Verifies that privileged operations write entries to `system.audit_log`.

use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, UserTableService,
};
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::stores::{SharedTableStore, StreamTableStore, UserTableStore};
use kalamdb_sql::KalamSql;
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

    let user_table_store = Arc::new(UserTableStore::new(backend.clone()));
    let shared_table_store = Arc::new(SharedTableStore::new(backend.clone()));
    let stream_table_store = Arc::new(StreamTableStore::new(backend.clone()));

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
    .with_storage_backend(backend.clone());

    (executor, temp_dir, kalam_sql)
}

async fn create_system_user(kalam_sql: &Arc<KalamSql>) -> UserId {
    let user_id = UserId::new("audit_admin");
    let now = chrono::Utc::now().timestamp_millis();

    let user = kalamdb_commons::system::User {
        id: user_id.clone(),
        username: "audit_admin".to_string(),
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
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    executor
        .execute(
            "CREATE USER 'audit_user' WITH PASSWORD 'StrongPass123!' ROLE user",
            Some(&admin_id),
        )
        .await
        .expect("CREATE USER failed");

    executor
        .execute("ALTER USER 'audit_user' SET ROLE dba", Some(&admin_id))
        .await
        .expect("ALTER USER failed");

    executor
        .execute("DROP USER 'audit_user'", Some(&admin_id))
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
    let (executor, _temp_dir, kalam_sql) = setup_test_executor().await;
    let admin_id = create_system_user(&kalam_sql).await;

    executor
        .execute("CREATE NAMESPACE analytics", Some(&admin_id))
        .await
        .expect("CREATE NAMESPACE failed");

    executor
        .execute(
            "CREATE TABLE analytics.events (id INT, value TEXT)",
            Some(&admin_id),
        )
        .await
        .expect("CREATE TABLE failed");

    executor
        .execute(
            "ALTER TABLE analytics.events SET ACCESS public",
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
