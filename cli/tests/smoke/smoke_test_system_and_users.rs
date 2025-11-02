// Smoke Test 3: System tables and user lifecycle
// Covers: SELECT from system tables, CREATE USER, verify presence, DROP USER, FLUSH ALL TABLES job

use crate::common::*;

#[test]
fn smoke_system_tables_and_user_lifecycle() {
    if !is_server_running() {
        println!(
            "Skipping smoke_system_tables_and_user_lifecycle: server not running at {}",
            SERVER_URL
        );
        return;
    }

    // 1) SELECT from system tables (allow empty but must succeed)
    let system_queries = [
        "SELECT * FROM system.jobs LIMIT 1",
        "SELECT * FROM system.users LIMIT 1",
        "SELECT * FROM system.live_queries LIMIT 1",
        "SELECT * FROM system.tables LIMIT 1",
        "SELECT * FROM system.namespaces LIMIT 1",
    ];
    for q in system_queries {
        let _ = execute_sql_as_root_via_cli(q).expect("system table query should succeed");
    }

    // 2) CREATE USER and verify present in system.users
    let uname = format!("smoke_user_{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis());
    let pass = "S1mpleP@ss!";
    let create_user = format!("CREATE USER {} WITH PASSWORD '{}' ROLE 'user'", uname, pass);
    execute_sql_as_root_via_cli(&create_user).expect("create user should succeed");

    // Use SELECT username to avoid column truncation in pretty-printed tables
    let users_out = execute_sql_as_root_via_cli(&format!(
        "SELECT username FROM system.users WHERE username='{}'",
        uname
    ))
    .expect("select user should succeed");
    assert!(
        users_out.contains(&uname),
        "expected newly created user to be listed: {}",
        users_out
    );

    // 3) DROP USER and verify removed or soft-deleted
    let drop_user = format!("DROP USER '{}'", uname);
    execute_sql_as_root_via_cli(&drop_user).expect("drop user should succeed");

    let users_out2 = execute_sql_as_root_via_cli(&format!(
        "SELECT * FROM system.users WHERE username='{}'",
        uname
    ))
    .expect("select user after drop should succeed");
    assert!(
        !users_out2.contains(&format!("| {} |", uname)) || users_out2.to_lowercase().contains("deleted"),
        "user should be removed or marked deleted: {}",
        users_out2
    );

    // 4) FLUSH ALL TABLES should enqueue a job
    // Create a test namespace first since FLUSH ALL TABLES requires an existing namespace
    let test_ns = "smoke_test_flush";
    let _ = execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE IF NOT EXISTS {}", test_ns));
    let _ = execute_sql_as_root_via_cli(&format!("FLUSH ALL TABLES IN {}", test_ns)).expect("flush all tables in namespace should succeed");
    let jobs = execute_sql_as_root_via_cli("SELECT * FROM system.jobs LIMIT 1")
        .expect("jobs query should succeed after flush all tables");
    assert!(
        !jobs.trim().is_empty(),
        "jobs table should not be empty after flush all tables"
    );
}
