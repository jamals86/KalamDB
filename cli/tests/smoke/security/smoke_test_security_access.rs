use crate::common::*;
use kalam_link::{AuthProvider, KalamLinkClient, KalamLinkTimeouts};
use std::time::Duration;

const MAX_SQL_QUERY_LENGTH: usize = 1024 * 1024;

/// Helper to get user_id from username by querying system.users as root
fn get_user_id_for_username(username: &str) -> Option<String> {
    let query = format!("SELECT user_id FROM system.users WHERE username = '{}'", username);
    let result = execute_sql_as_root_via_client_json(&query).ok()?;

    let json: serde_json::Value = serde_json::from_str(&result).ok()?;
    let rows = get_rows_as_hashmaps(&json)?;

    if let Some(row) = rows.first() {
        let user_id_value = row.get("user_id").map(extract_typed_value)?;
        return user_id_value.as_str().map(|s| s.to_string());
    }
    None
}

fn expect_unauthorized(result: Result<String, Box<dyn std::error::Error>>, context: &str) {
    assert!(result.is_err(), "Expected authorization failure: {}", context);
    if let Err(err) = result {
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unauthorized")
                || msg.contains("not authorized")
                || msg.contains("permission")
                || msg.contains("privilege")
                || msg.contains("access denied"),
            "Expected authorization error for {}: {}",
            context,
            err
        );
    }
}

fn subscribe_as_user(username: &str, password: &str, query: &str) -> Result<(), String> {
    let base_url = leader_url().unwrap_or_else(|| {
        get_available_server_urls()
            .first()
            .cloned()
            .unwrap_or_else(|| server_url().to_string())
    });

    let client = KalamLinkClient::builder()
        .base_url(&base_url)
        .auth(auth_provider_for_user_on_url(&base_url, username, password))
        .timeouts(
            KalamLinkTimeouts::builder()
                .connection_timeout_secs(5)
                .receive_timeout_secs(30)
                .send_timeout_secs(30)
                .subscribe_timeout_secs(10)
                .auth_timeout_secs(10)
                .initial_data_timeout(Duration::from_secs(15))
                .build(),
        )
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to build runtime: {}", e))?;

    let subscribe_result = rt.block_on(async move { client.subscribe(query).await });
    match subscribe_result {
        Ok(_subscription) => Err("Subscription succeeded unexpectedly".to_string()),
        Err(err) => Err(err.to_string()),
    }
}

fn expect_rejected(result: Result<String, Box<dyn std::error::Error>>, context: &str) {
    assert!(result.is_err(), "Expected rejection: {}", context);
    if let Err(err) = result {
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unauthorized")
                || msg.contains("not authorized")
                || msg.contains("permission")
                || msg.contains("privilege")
                || msg.contains("denied")
                || msg.contains("forbidden")
                || msg.contains("invalid")
                || msg.contains("constraint")
                || msg.contains("access denied"),
            "Expected rejection error for {}: {}",
            context,
            err
        );
    }
}

// Regular users cannot access system tables even via batch statements or subselects
#[ntest::timeout(180000)]
#[test]
fn smoke_security_system_tables_blocked_in_batch() {
    if !is_server_running() {
        eprintln!(
            "Skipping smoke_security_system_tables_blocked_in_batch: server not running at {}",
            server_url()
        );
        return;
    }

    let user_name = generate_unique_namespace("smoke_sys_batch");
    let user_pass = "smoke_pass_123";

    let create_user_sql =
        format!("CREATE USER {} WITH PASSWORD '{}' ROLE 'user'", user_name, user_pass);
    execute_sql_as_root_via_client(&create_user_sql).expect("Failed to create user");

    let batch_queries = vec![
        "SELECT 1; SELECT * FROM system.users",
        "SELECT 1; SELECT * FROM system.tables",
        "SELECT 1; SELECT * FROM (SELECT * FROM system.users) AS u",
        "SELECT 1; SELECT u.username FROM system.users u JOIN (SELECT user_id FROM system.users) s ON u.user_id = s.user_id",
        "SELECT 1; SELECT * FROM system.users WHERE username IN (SELECT username FROM system.users)",
        "WITH u AS (SELECT * FROM system.users) SELECT * FROM u",
        "SELECT 1; EXPLAIN SELECT * FROM system.users",
        "SELECT (SELECT username FROM system.users LIMIT 1) AS usr FROM system.users",
        "SELECT (SELECT COUNT(*) FROM system.users) AS cnt",
        "SELECT * FROM system.users WHERE user_id = (SELECT user_id FROM system.users LIMIT 1)",
        // Note: EXISTS in CASE WHEN is not yet implemented in DataFusion, skipping
        // "SELECT CASE WHEN EXISTS(SELECT 1 FROM system.users) THEN 'found' END",
        "SELECT 1 UNION SELECT user_id FROM system.users",
        "SELECT 1 EXCEPT SELECT user_id FROM system.users",
        "SELECT 1 INTERSECT SELECT user_id FROM system.users",
    ];

    for sql in batch_queries {
        let result = execute_sql_via_client_as(&user_name, user_pass, sql);
        expect_rejected(result, &format!("system table batch query: {}", sql));
    }

    let _ = execute_sql_as_root_via_client(&format!("DROP USER {}", user_name));
}

// Regular users cannot access private shared tables even with batch statements
#[ntest::timeout(180000)]
#[test]
fn smoke_security_private_shared_table_blocked_in_batch() {
    if !is_server_running() {
        eprintln!(
            "Skipping smoke_security_private_shared_table_blocked_in_batch: server not running at {}",
            server_url()
        );
        return;
    }

    let namespace = generate_unique_namespace("smoke_private_shared_ns");
    let table = generate_unique_table("smoke_private_shared_tbl");
    let full_table = format!("{}.{}", namespace, table);

    let regular_user = generate_unique_namespace("smoke_private_user");
    let password = "smoke_pass_123";

    execute_sql_as_root_via_client(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    let create_table_sql = format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY, name TEXT) WITH (TYPE='SHARED', ACCESS_LEVEL='PRIVATE')",
        full_table
    );
    execute_sql_as_root_via_client(&create_table_sql).expect("Failed to create shared table");

    execute_sql_as_root_via_client(&format!(
        "INSERT INTO {} (id, name) VALUES (1, 'secret')",
        full_table
    ))
    .expect("Failed to insert root row");

    execute_sql_as_root_via_client(&format!(
        "CREATE USER {} WITH PASSWORD '{}' ROLE 'user'",
        regular_user, password
    ))
    .expect("Failed to create regular user");

    let batch_sql = format!(
        "SELECT * FROM {table}; INSERT INTO {table} (id, name) VALUES (2, 'x'); UPDATE {table} SET name = 'y' WHERE id = 1; DELETE FROM {table} WHERE id = 1;",
        table = full_table
    );
    let batch_result = execute_sql_via_client_as(&regular_user, password, &batch_sql);
    expect_unauthorized(batch_result, "private shared table batch statement");

    // Also verify single statements are blocked
    let select_result = execute_sql_via_client_as(
        &regular_user,
        password,
        &format!("SELECT * FROM {}", full_table),
    );
    expect_unauthorized(select_result, "private shared table SELECT");

    let insert_result = execute_sql_via_client_as(
        &regular_user,
        password,
        &format!("INSERT INTO {} (id, name) VALUES (3, 'x')", full_table),
    );
    expect_unauthorized(insert_result, "private shared table INSERT");

    let delete_result = execute_sql_via_client_as(
        &regular_user,
        password,
        &format!("DELETE FROM {} WHERE id = 1", full_table),
    );
    expect_unauthorized(delete_result, "private shared table DELETE");

    let _ = execute_sql_as_root_via_client(&format!("DROP USER {}", regular_user));
    let _ = execute_sql_as_root_via_client(&format!("DROP TABLE IF EXISTS {}", full_table));
    let _ = execute_sql_as_root_via_client(&format!("DROP NAMESPACE IF EXISTS {}", namespace));
}

// Regular users cannot impersonate service/DBA/system users via AS USER (including batch)
#[ntest::timeout(180000)]
#[test]
fn smoke_security_regular_user_cannot_impersonate_privileged_users_in_batch() {
    if !is_server_running() {
        eprintln!(
            "Skipping smoke_security_regular_user_cannot_impersonate_privileged_users_in_batch: server not running at {}",
            server_url()
        );
        return;
    }

    let namespace = generate_unique_namespace("smoke_imp_ns");
    let table = generate_unique_table("smoke_imp_tbl");
    let full_table = format!("{}.{}", namespace, table);

    let regular_user = generate_unique_namespace("smoke_regular");
    let service_user = generate_unique_namespace("smoke_service");
    let dba_user = generate_unique_namespace("smoke_dba");
    let password = "smoke_pass_123";

    execute_sql_as_root_via_client(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    let create_table_sql = format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY, name TEXT) WITH (TYPE='USER')",
        full_table
    );
    execute_sql_as_root_via_client(&create_table_sql).expect("Failed to create table");

    execute_sql_as_root_via_client(&format!(
        "CREATE USER {} WITH PASSWORD '{}' ROLE 'user'",
        regular_user, password
    ))
    .expect("Failed to create regular user");

    execute_sql_as_root_via_client(&format!(
        "CREATE USER {} WITH PASSWORD '{}' ROLE 'service'",
        service_user, password
    ))
    .expect("Failed to create service user");

    execute_sql_as_root_via_client(&format!(
        "CREATE USER {} WITH PASSWORD '{}' ROLE 'dba'",
        dba_user, password
    ))
    .expect("Failed to create dba user");

    let service_user_id =
        get_user_id_for_username(&service_user).expect("Failed to get service user_id");
    let dba_user_id = get_user_id_for_username(&dba_user).expect("Failed to get dba user_id");
    let system_user_id = get_user_id_for_username("root")
        .or_else(|| get_user_id_for_username("system"))
        .expect("Failed to get system user_id");

    let attempts = vec![
        (service_user_id, "service"),
        (dba_user_id, "dba"),
        (system_user_id, "system"),
    ];

    for (target_user_id, label) in attempts {
        let batch_sql = format!(
            "INSERT INTO {} (id, name) VALUES (1, 'x') AS USER '{}'; SELECT 1;",
            full_table, target_user_id
        );
        let result = execute_sql_via_client_as(&regular_user, password, &batch_sql);
        expect_unauthorized(result, &format!("AS USER batch for {}", label));
    }

    let _ = execute_sql_as_root_via_client(&format!("DROP USER {}", regular_user));
    let _ = execute_sql_as_root_via_client(&format!("DROP USER {}", service_user));
    let _ = execute_sql_as_root_via_client(&format!("DROP USER {}", dba_user));
    let _ = execute_sql_as_root_via_client(&format!("DROP TABLE IF EXISTS {}", full_table));
    let _ = execute_sql_as_root_via_client(&format!("DROP NAMESPACE IF EXISTS {}", namespace));
}

// Regular users cannot subscribe to system tables or private shared tables
#[ntest::timeout(180000)]
#[test]
fn smoke_security_subscription_blocked_for_system_and_private_shared() {
    if !is_server_running() {
        eprintln!(
            "Skipping smoke_security_subscription_blocked_for_system_and_private_shared: server not running at {}",
            server_url()
        );
        return;
    }

    let namespace = generate_unique_namespace("smoke_sub_private_ns");
    let table = generate_unique_table("smoke_sub_private_tbl");
    let full_table = format!("{}.{}", namespace, table);

    let regular_user = generate_unique_namespace("smoke_sub_user");
    let password = "smoke_pass_123";

    execute_sql_as_root_via_client(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    let create_private_sql = format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY, name TEXT) WITH (TYPE='SHARED', ACCESS_LEVEL='PRIVATE')",
        full_table
    );
    execute_sql_as_root_via_client(&create_private_sql).expect("Failed to create shared table");

    let public_table = generate_unique_table("smoke_sub_public_tbl");
    let full_public = format!("{}.{}", namespace, public_table);
    let create_public_sql = format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY, name TEXT) WITH (TYPE='SHARED', ACCESS_LEVEL='PUBLIC')",
        full_public
    );
    execute_sql_as_root_via_client(&create_public_sql).expect("Failed to create public shared table");

    execute_sql_as_root_via_client(&format!(
        "CREATE USER {} WITH PASSWORD '{}' ROLE 'user'",
        regular_user, password
    ))
    .expect("Failed to create regular user");

    let system_query = "SELECT * FROM system.users";
    let system_sub = subscribe_as_user(&regular_user, password, system_query);
    assert!(system_sub.is_err(), "Expected system table subscription to fail");

    let shared_query = format!("SELECT * FROM {}", full_table);
    let shared_sub = subscribe_as_user(&regular_user, password, &shared_query);
    assert!(
        shared_sub.is_err(),
        "Expected private shared table subscription to fail"
    );

    let public_query = format!("SELECT * FROM {}", full_public);
    let public_sub = subscribe_as_user(&regular_user, password, &public_query);
    assert!(
        public_sub.is_err(),
        "Expected public shared table subscription to fail"
    );

    let _ = execute_sql_as_root_via_client(&format!("DROP USER {}", regular_user));
    let _ = execute_sql_as_root_via_client(&format!("DROP TABLE IF EXISTS {}", full_table));
    let _ = execute_sql_as_root_via_client(&format!("DROP TABLE IF EXISTS {}", full_public));
    let _ = execute_sql_as_root_via_client(&format!("DROP NAMESPACE IF EXISTS {}", namespace));
}

// Query length guard rejects oversized SQL (DoS prevention)
#[ntest::timeout(120000)]
#[test]
fn smoke_security_query_length_limit() {
    if !is_server_running() {
        eprintln!(
            "Skipping smoke_security_query_length_limit: server not running at {}",
            server_url()
        );
        return;
    }

    let user_name = generate_unique_namespace("smoke_len_user");
    let user_pass = "smoke_pass_123";

    let create_user_sql =
        format!("CREATE USER {} WITH PASSWORD '{}' ROLE 'user'", user_name, user_pass);
    execute_sql_as_root_via_client(&create_user_sql).expect("Failed to create user");

    let oversized = "a".repeat(MAX_SQL_QUERY_LENGTH + 1024);
    let sql = format!("SELECT '{}'", oversized);
    let result = execute_sql_via_client_as(&user_name, user_pass, &sql);
    expect_rejected(result, "oversized query length");

    let _ = execute_sql_as_root_via_client(&format!("DROP USER {}", user_name));
}

// Regular users cannot write to system tables (even in batch)
#[ntest::timeout(180000)]
#[test]
fn smoke_security_system_table_write_blocked() {
    if !is_server_running() {
        eprintln!(
            "Skipping smoke_security_system_table_write_blocked: server not running at {}",
            server_url()
        );
        return;
    }

    let user_name = generate_unique_namespace("smoke_sys_write_user");
    let user_pass = "smoke_pass_123";

    let create_user_sql =
        format!("CREATE USER {} WITH PASSWORD '{}' ROLE 'user'", user_name, user_pass);
    execute_sql_as_root_via_client(&create_user_sql).expect("Failed to create user");

    let batch_sql = "INSERT INTO system.users (username) VALUES ('hacker'); UPDATE system.users SET username='x' WHERE username='root'; DELETE FROM system.users WHERE username='root';";
    let result = execute_sql_via_client_as(&user_name, user_pass, batch_sql);
    expect_rejected(result, "system table write batch");

    let _ = execute_sql_as_root_via_client(&format!("DROP USER {}", user_name));
}
