//! RBAC integration tests (Phase 5)
//!
//! Verifies role-based access control behavior using SQL executor paths.

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, QueryResultTestExt, TestServer};
use kalam_link::models::ResponseStatus;
use kalamdb_commons::models::{AuthType, Role, StorageMode, UserId, UserName};

async fn insert_user(server: &TestServer, username: &str, role: Role) -> UserId {
    // Note: Users provider treats user_id as username key; keep them equal
    let user_id = UserId::new(username);
    let now = chrono::Utc::now().timestamp_millis();
    let user = kalamdb_commons::system::User {
        id: user_id.clone(),
        username: UserName::new(username),
        password_hash: "".to_string(),
        role,
        email: Some(format!("{}@test.local", username)),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: None,
        failed_login_attempts: 0,
        locked_until: None,
        last_login_at: None,
        created_at: now,
        updated_at: now,
        last_seen: None,
        deleted_at: None,
    };
    server
        .app_context
        .system_tables()
        .users()
        .create_user(user)
        .expect("insert user");
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
    if ns_resp.status != ResponseStatus::Success {
        eprintln!("Create namespace error: {:?}", ns_resp.error);
    }
    assert_eq!(ns_resp.status, ResponseStatus::Success);
    let create = format!(
        "CREATE TABLE {}.notes (id INT PRIMARY KEY, content TEXT) WITH (TYPE = 'USER')",
        ns
    );
    let resp = server.execute_sql_as_user(&create, u1.as_str()).await;
    println!("create user table resp = {:?}", resp);
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "create user table resp: {:?}",
        resp
    );

    // Insert a row as u1
    let ins = format!("INSERT INTO {}.notes (id, content) VALUES (1, 'hi')", ns);
    let resp = server.execute_sql_as_user(&ins, u1.as_str()).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Read as u1 → sees 1 row
    let sel = format!("SELECT * FROM {}.notes", ns);
    let resp = server.execute_sql_as_user(&sel, u1.as_str()).await;
    println!("select as u1 resp = {:?}", resp);
    assert_eq!(resp.status, ResponseStatus::Success);
    let rows = resp.results[0].rows.as_ref().unwrap();
    assert_eq!(rows.len(), 1, "u1 should see own rows");

    // Read as u2 → per-user isolation should show 0 rows
    let resp = server.execute_sql_as_user(&sel, u2.as_str()).await;
    assert_eq!(resp.status, ResponseStatus::Success);
    let total = resp
        .results
        .first()
        .and_then(|r| r.rows.as_ref())
        .map(|v| v.len())
        .unwrap_or(0);
    assert_eq!(total, 0, "u2 should not see u1 data");
}

#[actix_web::test]async fn test_service_role_cross_user_access() {
    let server = TestServer::new().await;
    let ns = "rbac_service";
    let svc = insert_user(&server, "svc", Role::Service).await;
    let alice = insert_user(&server, "svc_alice", Role::User).await;
    let bob = insert_user(&server, "svc_bob", Role::User).await;

    fixtures::create_namespace(&server, ns).await;

    let create = format!(
        "CREATE TABLE {}.orders (id INT PRIMARY KEY, content TEXT) WITH (TYPE = 'USER')",
        ns
    );
    let resp = server.execute_sql_as_user(&create, alice.as_str()).await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "user should be able to create table"
    );

    let insert_alice = format!(
        "INSERT INTO {}.orders (id, content) VALUES (1, 'alice note')",
        ns
    );
    let insert_bob = format!(
        "INSERT INTO {}.orders (id, content) VALUES (2, 'bob note')",
        ns
    );
    server
        .execute_sql_as_user(&insert_alice, alice.as_str())
        .await;
    server.execute_sql_as_user(&insert_bob, bob.as_str()).await;

    let select = format!("SELECT content FROM {}.orders ORDER BY content", ns);
    let resp = server.execute_sql_as_user(&select, svc.as_str()).await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "service select should succeed"
    );

    let rows = resp.results[0].rows_as_maps();
    let contents: std::collections::HashSet<_> = rows
        .iter()
        .filter_map(|row| row.get("content").and_then(|v| v.as_str()))
        .collect();

    assert!(contents.contains("alice note"), "should include alice data");
    assert!(contents.contains("bob note"), "should include bob data");
    assert_eq!(rows.len(), 2, "service should see all user rows");
}

#[actix_web::test]
async fn test_service_role_flush_operations() {
    let server = TestServer::new().await;
    let ns = "rbac_flush";
    let svc = insert_user(&server, "svc_flush", Role::Service).await;
    let user = insert_user(&server, "flush_user", Role::User).await;

    fixtures::create_namespace(&server, ns).await;

    let create = format!(
        "CREATE TABLE {}.events (id INT PRIMARY KEY, message TEXT) WITH (TYPE = 'USER')",
        ns
    );
    let resp = server.execute_sql_as_user(&create, svc.as_str()).await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "service should create user table"
    );

    for i in 0..3 {
        let insert = format!(
            "INSERT INTO {}.events (id, message) VALUES ({}, 'msg {}')",
            ns, i, i
        );
        server.execute_sql_as_user(&insert, user.as_str()).await;
    }

    let flush = format!("FLUSH TABLE {}.events", ns);
    let resp = server.execute_sql_as_user(&flush, svc.as_str()).await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "service flush should succeed"
    );
    assert!(resp
        .results
        .first()
        .and_then(|r| r.message.as_ref())
        .map(|msg| msg.contains("Flush started"))
        .unwrap_or(true));
}

#[actix_web::test]
async fn test_service_role_cannot_manage_users() {
    let server = TestServer::new().await;
    let svc = insert_user(&server, "svc_admin", Role::Service).await;

    let sql = "CREATE USER 'managed' WITH PASSWORD 'StrongPass123!' ROLE user";
    let resp = server.execute_sql_as_user(sql, svc.as_str()).await;
    assert_eq!(
        resp.status,
        ResponseStatus::Error,
        "service should not manage users"
    );
}

#[actix_web::test]
async fn test_user_cannot_manage_users() {
    let server = TestServer::new().await;
    let user = insert_user(&server, "charlie", Role::User).await;

    // Regular user cannot CREATE USER
    let sql = "CREATE USER 'eve' WITH PASSWORD 'x' ROLE user";
    let resp = server.execute_sql_as_user(sql, user.as_str()).await;
    if resp.status != ResponseStatus::Error {
        eprintln!("Unexpected status for user create: {:?}", resp);
    }
    assert_eq!(
        resp.status,
        ResponseStatus::Error,
        "user should be forbidden to manage users"
    );
}

#[actix_web::test]
async fn test_dba_can_manage_users() {
    let server = TestServer::new().await;
    let dba = insert_user(&server, "admin_dba", Role::Dba).await;

    let sql = "CREATE USER 'svc1' WITH PASSWORD 'StrongPass123!' ROLE service";
    let resp = server.execute_sql_as_user(sql, dba.as_str()).await;
    if resp.status != ResponseStatus::Success {
        eprintln!("DBA create user error: {:?}", resp.error);
    }
    assert_eq!(resp.status, ResponseStatus::Success, "dba can create users");
}

#[actix_web::test]
async fn test_system_role_all_access_smoke() {
    let server = TestServer::new().await;
    let sys = insert_user(&server, "sys", Role::System).await;

    // System can perform namespace admin operations
    let resp = server
        .execute_sql_as_user("CREATE NAMESPACE rbac_sys_ns", sys.as_str())
        .await;
    eprintln!(
        "System CREATE NAMESPACE resp: status={} error={:?}",
        resp.status, resp.error
    );
    assert_eq!(resp.status, ResponseStatus::Success);

    // CREATE USER should work
    let resp = server
        .execute_sql_as_user(
            "CREATE USER 'zzz' WITH PASSWORD 'StrongPass123!' ROLE user",
            sys.as_str(),
        )
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);
}
