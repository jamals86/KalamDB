//! Tests for last_seen tracking during authentication.
//!
//! Verifies that authentication updates the `last_seen` field at most once per day.

#[path = "integration/common/mod.rs"]
mod common;

use base64::{engine::general_purpose, Engine as _};
use common::TestServer;
use kalamdb_auth::connection::ConnectionInfo;
use kalamdb_commons::{AuthType, Role, StorageMode, UserId};
use tokio::time::{sleep, Duration};

fn basic_auth_header(username: &str, password: &str) -> String {
    let credentials = format!("{}:{}", username, password);
    let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());
    format!("Basic {}", encoded)
}

#[tokio::test]
async fn test_last_seen_updates_on_authentication() {
    let server = TestServer::new().await;
    let auth_service = server.auth_service();
    let adapter = server.users_repo();
    
    let username = "seen_once";
    let password = "StrongPass1!";
    
    // Create user
    server.create_user(username, password, Role::User).await;

    let auth_header = basic_auth_header(username, password);
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await
        .expect("Authentication should succeed");

    // Allow background task to persist update (if any)
    sleep(Duration::from_millis(100)).await;

    let users_provider = server.app_context.system_tables().users();
    let user = users_provider
        .get_user_by_username(username)
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

    let server = TestServer::new().await;
    let auth_service = server.auth_service();
    let adapter = server.users_repo();
    
    let username = "seen_daily";
    let password = "StrongPass1!";
    
    // Create user with last_seen = yesterday
    let user_id = UserId::new(username);
    let now = chrono::Utc::now().timestamp_millis();
    let yesterday = (chrono::Utc::now() - ChronoDuration::days(1)).timestamp_millis();
    let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");

    let user = kalamdb_commons::system::User {
        id: user_id.clone(),
        username: username.into(),
        password_hash,
        role: Role::User,
        email: Some(format!("{}@example.com", username)),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: None,
        created_at: now,
        updated_at: now,
        last_seen: Some(yesterday),
        deleted_at: None,
    };

    let users_provider = server.app_context.system_tables().users();
    users_provider.create_user(user).expect("Failed to insert test user");

    let auth_header = basic_auth_header(username, password);
    let connection_info = ConnectionInfo::new(Some("127.0.0.1".to_string()));

    // First auth - should update last_seen to NOW
    auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await
        .expect("Authentication should succeed");
        
    sleep(Duration::from_millis(100)).await;
    
    let user_after_first = users_provider
        .get_user_by_username(username)
        .expect("Failed to fetch user")
        .expect("User should exist");
    let first_seen = user_after_first.last_seen.expect("last_seen should be set");
    
    assert!(first_seen > yesterday, "last_seen should be updated");

    // Second auth - should NOT update last_seen (because it's same day)
    auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await
        .expect("Second authentication should succeed");
        
    sleep(Duration::from_millis(100)).await;
    
    let user_after_second = users_provider
        .get_user_by_username(username)
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
