//! Tests for last_seen tracking during authentication.
//!
//! Verifies that authentication updates the `last_seen` field at most once per day.

use base64::engine::general_purpose;
use base64::Engine as _;
use kalamdb_auth::connection::ConnectionInfo;
use kalamdb_auth::AuthService;
use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_sql::{KalamSql, RocksDbAdapter};
use kalamdb_store::{RocksDBBackend, RocksDbInit, StorageBackend};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

async fn setup_auth_environment() -> (TempDir, Arc<KalamSql>, Arc<RocksDbAdapter>, AuthService) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().to_str().unwrap();

    let db_init = RocksDbInit::new(db_path);
    let db = db_init.open().expect("Failed to open RocksDB");

    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));
    let kalam_sql = Arc::new(KalamSql::new(backend).expect("Failed to create KalamSQL"));
    let adapter = Arc::new(kalam_sql.adapter().clone());

    let auth_service = AuthService::new(
        "test-secret".to_string(),
        vec![],
        true,  // allow_remote_access to simplify tests
        false, // oauth auto-provision
        Role::User,
    );

    (temp_dir, kalam_sql, adapter, auth_service)
}

async fn insert_password_user(
    kalam_sql: &Arc<KalamSql>,
    username: &str,
    password: &str,
    last_seen: Option<i64>,
) -> UserId {
    let user_id = UserId::new(format!("user_{}", username));
    let now = chrono::Utc::now().timestamp_millis();
    let password_hash = bcrypt::hash(password, 12).expect("Failed to hash password");

    let user = kalamdb_commons::system::User {
        id: user_id.clone(),
        username: username.to_string(),
        password_hash,
        role: Role::User,
        email: Some(format!("{}@example.com", username)),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: Some(StorageId::local()),
        created_at: now,
        updated_at: now,
        last_seen,
        deleted_at: None,
    };

    kalam_sql
        .insert_user(&user)
        .expect("Failed to insert test user");
    user_id
}

fn basic_auth_header(username: &str, password: &str) -> String {
    let credentials = format!("{}:{}", username, password);
    let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());
    format!("Basic {}", encoded)
}

#[tokio::test]
async fn test_last_seen_updates_on_authentication() {
    let (_temp_dir, kalam_sql, adapter, auth_service) = setup_auth_environment().await;
    let username = "seen_once";
    let password = "StrongPass1!";
    insert_password_user(&kalam_sql, username, password, None).await;

    let auth_header = basic_auth_header(username, password);
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    auth_service
        .authenticate(&auth_header, &connection_info, &adapter)
        .await
        .expect("Authentication should succeed");

    // Allow background task to persist update
    sleep(Duration::from_millis(100)).await;

    let user = kalam_sql
        .get_user(username)
        .expect("Failed to fetch user")
        .expect("User should exist");

    assert!(
        user.last_seen.is_some(),
        "last_seen should be set after authentication"
    );
}

#[tokio::test]
async fn test_last_seen_updates_only_once_per_day() {
    use chrono::Duration as ChronoDuration;

    let (_temp_dir, kalam_sql, adapter, auth_service) = setup_auth_environment().await;
    let username = "seen_daily";
    let password = "StrongPass1!";
    let yesterday = (chrono::Utc::now() - ChronoDuration::days(1)).timestamp_millis();
    insert_password_user(&kalam_sql, username, password, Some(yesterday)).await;

    let auth_header = basic_auth_header(username, password);
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    auth_service
        .authenticate(&auth_header, &connection_info, &adapter)
        .await
        .expect("Authentication should succeed");
    sleep(Duration::from_millis(100)).await;
    let user_after_first = kalam_sql
        .get_user(username)
        .expect("Failed to fetch user")
        .expect("User should exist");
    let first_seen = user_after_first.last_seen.expect("last_seen should be set");

    auth_service
        .authenticate(&auth_header, &connection_info, &adapter)
        .await
        .expect("Second authentication should succeed");
    sleep(Duration::from_millis(100)).await;
    let user_after_second = kalam_sql
        .get_user(username)
        .expect("Failed to fetch user")
        .expect("User should exist");
    let second_seen = user_after_second
        .last_seen
        .expect("last_seen should remain set");

    assert_eq!(
        first_seen, second_seen,
        "last_seen should not change on multiple authentications within the same day"
    );
}
