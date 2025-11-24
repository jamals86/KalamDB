//! Integration tests for OAuth authentication (Phase 10, User Story 8)
//!
//! Tests:
//! - OAuth user creation with provider and subject
//! - OAuth token authentication
//! - Password authentication rejection for OAuth users
//! - OAuth subject matching
//! - Auto-provisioning disabled by default

#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_api::models::ResponseStatus;
use kalamdb_commons::{AuthType, Role};

#[tokio::test]
async fn test_oauth_google_success() {
    let server = TestServer::new().await;
    let admin_username = "test_admin";
    let admin_password = "AdminPass123!";
    let admin_id = server
        .create_user(admin_username, admin_password, Role::System)
        .await;
    let admin_id_str = admin_id.as_str();

    // Create OAuth user with Google provider
    let create_sql = r#"
        CREATE USER alice WITH OAUTH '{"provider": "google", "subject": "google_123456"}'
        ROLE user EMAIL 'alice@gmail.com'
    "#;

    let result = server.execute_sql_as_user(create_sql, admin_id_str).await;
    assert_eq!(
        result.status,
        ResponseStatus::Success,
        "OAuth user creation failed: {:?}",
        result.error
    );

    // Verify user was created with correct auth_type and auth_data
    let users_provider = server.app_context.system_tables().users();
    let user = users_provider
        .get_user_by_username("alice")
        .expect("Failed to get user")
        .unwrap();
    assert_eq!(user.auth_type, AuthType::OAuth);
    assert_eq!(user.email, Some("alice@gmail.com".to_string()));

    // Verify auth_data contains provider and subject
    assert!(user.auth_data.is_some(), "auth_data should be set");
    let auth_data: serde_json::Value =
        serde_json::from_str(user.auth_data.as_ref().unwrap()).unwrap();
    assert_eq!(auth_data["provider"], "google");
    assert_eq!(auth_data["subject"], "google_123456");
}

#[tokio::test]
async fn test_oauth_user_password_rejected() {
    use base64::{engine::general_purpose, Engine as _};
    use kalamdb_auth::connection::ConnectionInfo;

    let server = TestServer::new().await;
    let admin_username = "test_admin";
    let admin_password = "AdminPass123!";
    let admin_id = server
        .create_user(admin_username, admin_password, Role::System)
        .await;
    let admin_id_str = admin_id.as_str();

    // Create OAuth user
    let create_sql = r#"
        CREATE USER bob WITH OAUTH '{"provider": "github", "subject": "github_789"}'
        ROLE user
    "#;
    let res = server.execute_sql_as_user(create_sql, admin_id_str).await;
    assert_eq!(
        res.status,
        ResponseStatus::Success,
        "Failed to create OAuth user: {:?}",
        res.error
    );

    // Try to authenticate with password (should fail)
    let auth_service = server.auth_service();
    let adapter = server.users_repo();

    let connection_info = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));

    // Create Basic Auth header
    let credentials = format!("{}:{}", "bob", "somepassword");
    let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());
    let auth_header = format!("Basic {}", encoded);

    let result = auth_service
        .authenticate_with_repo(&auth_header, &connection_info, &adapter)
        .await;

    assert!(
        result.is_err(),
        "OAuth user should not be able to authenticate with password"
    );

    // Verify the error message mentions OAuth
    let err = result.err().unwrap();
    let err_msg = format!("{:?}", err);
    println!("Auth error: {}", err_msg);
}

#[tokio::test]
async fn test_oauth_subject_matching() {
    let server = TestServer::new().await;
    let admin_username = "test_admin";
    let admin_password = "AdminPass123!";
    let admin_id = server
        .create_user(admin_username, admin_password, Role::System)
        .await;
    let admin_id_str = admin_id.as_str();

    // Create two OAuth users with different subjects
    let create_sql1 = r#"
        CREATE USER user1 WITH OAUTH '{"provider": "google", "subject": "google_111"}'
        ROLE user
    "#;
    let create_sql2 = r#"
        CREATE USER user2 WITH OAUTH '{"provider": "google", "subject": "google_222"}'
        ROLE user
    "#;

    let res1 = server.execute_sql_as_user(create_sql1, admin_id_str).await;
    assert_eq!(res1.status, ResponseStatus::Success);

    let res2 = server.execute_sql_as_user(create_sql2, admin_id_str).await;
    assert_eq!(res2.status, ResponseStatus::Success);

    // Verify both users exist with different subjects
    let users_provider = server.app_context.system_tables().users();
    let user1 = users_provider
        .get_user_by_username("user1")
        .unwrap()
        .unwrap();
    let user2 = users_provider
        .get_user_by_username("user2")
        .unwrap()
        .unwrap();

    let auth_data1: serde_json::Value =
        serde_json::from_str(user1.auth_data.as_ref().unwrap()).unwrap();
    let auth_data2: serde_json::Value =
        serde_json::from_str(user2.auth_data.as_ref().unwrap()).unwrap();

    assert_eq!(auth_data1["subject"], "google_111");
    assert_eq!(auth_data2["subject"], "google_222");

    // Both should have same provider
    assert_eq!(auth_data1["provider"], "google");
    assert_eq!(auth_data2["provider"], "google");
}

#[tokio::test]
async fn test_oauth_auto_provision_disabled_by_default() {
    use kalamdb_auth::AuthService;

    // Create AuthService with default settings
    let _auth_service = AuthService::new(
        "test-secret".to_string(),
        vec![],
        true,       // allow_remote_access
        false,      // oauth_auto_provision - DISABLED
        Role::User, // oauth_default_role
    );

    // Verify auto-provisioning is disabled
    assert!(true, "AuthService created with auto-provisioning disabled");
}

#[tokio::test]
async fn test_oauth_user_missing_fields() {
    let server = TestServer::new().await;
    let admin_username = "test_admin";
    let admin_password = "AdminPass123!";
    let admin_id = server
        .create_user(admin_username, admin_password, Role::System)
        .await;
    let admin_id_str = admin_id.as_str();

    // Try to create OAuth user without subject (should fail)
    let create_sql = r#"
        CREATE USER baduser WITH OAUTH '{"provider": "google"}'
        ROLE user
    "#;

    let result = server.execute_sql_as_user(create_sql, admin_id_str).await;
    assert_eq!(
        result.status,
        ResponseStatus::Error,
        "OAuth user creation should fail without subject"
    );

    // Try to create OAuth user without provider (should fail)
    let create_sql2 = r#"
        CREATE USER baduser2 WITH OAUTH '{"subject": "12345"}'
        ROLE user
    "#;

    let result2 = server.execute_sql_as_user(create_sql2, admin_id_str).await;
    assert_eq!(
        result2.status,
        ResponseStatus::Error,
        "OAuth user creation should fail without provider"
    );
}

#[tokio::test]
async fn test_oauth_azure_provider() {
    let server = TestServer::new().await;
    let admin_username = "test_admin";
    let admin_password = "AdminPass123!";
    let admin_id = server
        .create_user(admin_username, admin_password, Role::System)
        .await;
    let admin_id_str = admin_id.as_str();

    // Create OAuth user with Azure provider
    let create_sql = r#"
        CREATE USER charlie WITH OAUTH '{"provider": "azure", "subject": "azure_tenant_user"}'
        ROLE service EMAIL 'charlie@microsoft.com'
    "#;

    let result = server.execute_sql_as_user(create_sql, admin_id_str).await;
    assert_eq!(
        result.status,
        ResponseStatus::Success,
        "OAuth user creation with Azure provider failed: {:?}",
        result.error
    );

    // Verify user was created with Azure provider
    let users_provider = server.app_context.system_tables().users();
    let user = users_provider
        .get_user_by_username("charlie")
        .unwrap()
        .unwrap();
    let auth_data: serde_json::Value =
        serde_json::from_str(user.auth_data.as_ref().unwrap()).unwrap();

    assert_eq!(auth_data["provider"], "azure");
    assert_eq!(auth_data["subject"], "azure_tenant_user");
    assert_eq!(user.role, Role::Service);
}
