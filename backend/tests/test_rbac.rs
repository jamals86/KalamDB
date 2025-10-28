//! RBAC integration tests (Phase 5)
//!
//! Verifies role-based access control behavior using SQL executor paths.

#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_commons::{AuthType, Role, StorageMode, UserId};

async fn insert_user(server: &TestServer, username: &str, role: Role) -> UserId {
    // Note: Users provider treats user_id as username key; keep them equal
    let user_id = UserId::new(username);
    let now = chrono::Utc::now().timestamp_millis();
    let user = kalamdb_commons::system::User {
        id: user_id.clone(),
        username: username.to_string(),
        password_hash: "".to_string(),
        role,
        email: Some(format!("{}@test.local", username)),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: None,
        created_at: now,
        updated_at: now,
        last_seen: None,
        deleted_at: None,
    };
    server.kalam_sql.insert_user(&user).expect("insert user");
    user_id
}

#[actix_web::test]
async fn test_user_role_own_tables_access_and_isolation() {
    let server = TestServer::new().await;
    let ns = "rbac_user";
    let u1 = insert_user(&server, "alice", Role::User).await;
    let u2 = insert_user(&server, "bob", Role::User).await;

    // Create namespace and table
    let ns_resp = server.execute_sql("CREATE NAMESPACE rbac_user").await;
    if ns_resp.status != "success" { eprintln!("Create namespace error: {:?}", ns_resp.error); }
    assert_eq!(ns_resp.status, "success");
    let create = format!(
        "CREATE USER TABLE {}.notes (id INT, content TEXT)",
        ns
    );
    let resp = server.execute_sql_as_user(&create, u1.as_str()).await;
    if resp.status != "success" {
        panic!("u1 create user table failed: {:?}", resp);
    }

    // Insert a row as u1
    let ins = format!("INSERT INTO {}.notes (id, content) VALUES (1, 'hi')", ns);
    let resp = server.execute_sql_as_user(&ins, u1.as_str()).await;
    assert_eq!(resp.status, "success");

    // Read as u1 → sees 1 row
    let sel = format!("SELECT * FROM {}.notes", ns);
    let resp = server.execute_sql_as_user(&sel, u1.as_str()).await;
    assert_eq!(resp.status, "success");
    let rows = resp.results[0].rows.as_ref().unwrap();
    assert_eq!(rows.len(), 1, "u1 should see own rows");

    // Read as u2 → per-user isolation should show 0 rows
    let resp = server.execute_sql_as_user(&sel, u2.as_str()).await;
    assert_eq!(resp.status, "success");
    let total = resp
        .results
        .first()
        .and_then(|r| r.rows.as_ref())
        .map(|v| v.len())
        .unwrap_or(0);
    assert_eq!(total, 0, "u2 should not see u1 data");
}

#[actix_web::test]
async fn test_user_cannot_manage_users() {
    let server = TestServer::new().await;
    let user = insert_user(&server, "charlie", Role::User).await;

    // Regular user cannot CREATE USER
    let sql = "CREATE USER 'eve' WITH PASSWORD 'x' ROLE user";
    let resp = server.execute_sql_as_user(sql, user.as_str()).await;
    if resp.status != "error" { eprintln!("Unexpected status for user create: {:?}", resp); }
    assert_eq!(resp.status, "error", "user should be forbidden to manage users");
}

#[actix_web::test]
async fn test_dba_can_manage_users() {
    let server = TestServer::new().await;
    let dba = insert_user(&server, "admin_dba", Role::Dba).await;

    let sql = "CREATE USER 'svc1' WITH PASSWORD 'StrongPass123!' ROLE service";
    let resp = server.execute_sql_as_user(sql, dba.as_str()).await;
    if resp.status != "success" { eprintln!("DBA create user error: {:?}", resp.error); }
    assert_eq!(resp.status, "success", "dba can create users");
}

#[actix_web::test]
async fn test_system_role_all_access_smoke() {
    let server = TestServer::new().await;
    let sys = insert_user(&server, "sys", Role::System).await;

    // System can perform namespace admin operations
    let resp = server
        .execute_sql_as_user("CREATE NAMESPACE rbac_sys_ns", sys.as_str())
        .await;
    eprintln!("System CREATE NAMESPACE resp: status={} error={:?}", resp.status, resp.error);
    assert_eq!(resp.status, "success");

    // CREATE USER should work
    let resp = server
        .execute_sql_as_user("CREATE USER 'zzz' WITH PASSWORD 'StrongPass123!' ROLE user", sys.as_str())
        .await;
    assert_eq!(resp.status, "success");
}
