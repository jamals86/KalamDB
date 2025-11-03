// Smoke Test 2: Shared table CRUD
// Covers: namespace creation, shared table creation, insert/select, delete/update, final select, drop table

use crate::common::*;

#[test]
fn smoke_shared_table_crud() {
    if !is_server_running() {
        println!(
            "Skipping smoke_shared_table_crud: server not running at {}",
            SERVER_URL
        );
        return;
    }

    // Unique names per run
    let namespace = generate_unique_namespace("smoke_ns");
    let table = generate_unique_table("shared_crud");
    let full = format!("{}.{}", namespace, table);

    // 0) Create namespace
    let ns_sql = format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace);
    execute_sql_as_root_via_cli(&ns_sql).expect("create namespace should succeed");

    // 1) Create shared table
    let create_sql = format!(
        r#"CREATE SHARED TABLE {} (
            id INT AUTO_INCREMENT,
            name VARCHAR NOT NULL,
            status VARCHAR,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
        full
    );
    execute_sql_as_root_via_cli(&create_sql).expect("create shared table should succeed");

    // 2) Insert rows
    let ins1 = format!("INSERT INTO {} (name, status) VALUES ('alpha', 'new')", full);
    let ins2 = format!("INSERT INTO {} (name, status) VALUES ('beta', 'new')", full);
    execute_sql_as_root_via_cli(&ins1).expect("insert alpha should succeed");
    execute_sql_as_root_via_cli(&ins2).expect("insert beta should succeed");

    // 3) SELECT and verify both rows present
    let sel_all = format!("SELECT * FROM {}", full);
    let out = execute_sql_as_root_via_cli(&sel_all).expect("select should succeed");
    assert!(out.contains("alpha"), "expected 'alpha' in results: {}", out);
    assert!(out.contains("beta"), "expected 'beta' in results: {}", out);

    // 4) DELETE one row
    let del = format!("DELETE FROM {} WHERE name='alpha'", full);
    execute_sql_as_root_via_cli(&del).expect("delete should succeed");

    // 5) UPDATE one row
    let upd = format!("UPDATE {} SET status='done' WHERE name='beta'", full);
    execute_sql_as_root_via_cli(&upd).expect("update should succeed");

    // 6) SELECT non-deleted rows and verify contents reflect changes
    let out2 = execute_sql_as_root_via_cli(&sel_all).expect("second select should succeed");
    assert!(out2.contains("beta"), "expected 'beta' to remain: {}", out2);
    assert!(out2.contains("done"), "expected updated status 'done': {}", out2);

    // 7) DROP TABLE and verify selecting fails
    let drop_sql = format!("DROP TABLE {}", full);
    execute_sql_as_root_via_cli(&drop_sql).expect("drop table should succeed");

    let select_after_drop = execute_sql_via_cli(&sel_all);
    match select_after_drop {
        Ok(s) => panic!("expected failure selecting dropped table, got output: {}", s),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            assert!(
                msg.contains("not found")
                    || msg.contains("does not exist")
                    || msg.contains("unknown table"),
                "unexpected error selecting dropped table: {}",
                msg
            );
        }
    }
}
