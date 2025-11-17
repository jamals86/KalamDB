use crate::common::*;

// Smoke Test 5: User table per-user isolation (RLS)
// Steps:
// 0) As root: create namespace
// 1) As root: create a user table
// 2) As root: insert several rows
// 3) Create a new regular user
// 4) Login via CLI as the regular user
// 5) As regular user: insert multiple rows, update one, delete one, then SELECT all
// 6) Verify: (a) regular user can insert, (b) login succeeds, (c) SELECT shows only this user's rows (no root rows)
#[test]
fn smoke_user_table_rls_isolation() -> Result<(), Box<dyn std::error::Error>> {
    if !is_server_running() {
        eprintln!(
            "Skipping smoke_user_table_rls_isolation: server not running at {}",
            SERVER_URL
        );
        return Ok(());
    }

    // Unique namespace/table and user credentials per run
    let namespace = generate_unique_namespace("smoke_rls_ns");
    let table = generate_unique_table("smoke_rls_tbl");
    let full_table = format!("{}.{}", namespace, table);

    let user_name = format!(
        "smoke_user_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let user_pass = "smoke_pass_123";

    // 0) As root: create namespace
    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))?;

    // 1) As root: create a user table
    let create_table_sql = format!(
        r#"CREATE USER TABLE {} (
            id BIGINT AUTO_INCREMENT PRIMARY KEY,
            content TEXT NOT NULL,
            updated INT DEFAULT 0,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_table_sql)?;

    // 2) As root: insert several rows
    let root_rows = vec!["root_row_1", "root_row_2", "root_row_3"];
    for val in &root_rows {
        let ins = format!("INSERT INTO {} (content) VALUES ('{}')", full_table, val);
        execute_sql_as_root_via_cli(&ins)?;
    }

    // 3) Create a new regular user
    let create_user_sql = format!(
        "CREATE USER {} WITH PASSWORD '{}' ROLE 'user'",
        user_name, user_pass
    );
    execute_sql_as_root_via_cli(&create_user_sql)?;

    // 4) Login via CLI as the regular user (implicit via next commands)
    // Validate auth by running a trivial command
    let _ = execute_sql_via_cli_as(&user_name, user_pass, "SELECT 1")?;

    // 5) As regular user: insert multiple rows
    let user_rows = vec!["user_row_a", "user_row_b", "user_row_c"];
    for val in &user_rows {
        let ins = format!("INSERT INTO {} (content) VALUES ('{}')", full_table, val);
        execute_sql_via_cli_as(&user_name, user_pass, &ins)?;
    }

    // Fetch ids for the specific user rows so we can perform id-based UPDATE/DELETE
    // (backend currently restricts USER table UPDATE/DELETE to primary key equality predicates)
    let id_query = format!(
        "SELECT id, content FROM {} WHERE content IN ('user_row_b','user_row_c') ORDER BY content",
        full_table
    );
    let id_out = execute_sql_via_cli_as(&user_name, user_pass, &id_query)?;

    // Parse ids from table output, handling negative numbers
    let mut row_b_id: Option<String> = None;
    let mut row_c_id: Option<String> = None;
    for line in id_out.lines() {
        if line.contains("user_row_b") || line.contains("user_row_c") {
            // Split by table cell delimiter '│' and take first column (id)
            let parts: Vec<&str> = line.split('│').collect();
            if parts.len() >= 2 {
                let id_part = parts[1].trim();
                // Validate it's a number (allowing negative)
                if let Ok(_) = id_part.parse::<i64>() {
                    if line.contains("user_row_b") {
                        row_b_id = Some(id_part.to_string());
                    }
                    if line.contains("user_row_c") {
                        row_c_id = Some(id_part.to_string());
                    }
                }
            }
        }
    }
    let row_b_id = row_b_id
        .ok_or_else(|| format!("Failed to parse id for user_row_b from output: {}", id_out))?;
    let row_c_id = row_c_id
        .ok_or_else(|| format!("Failed to parse id for user_row_c from output: {}", id_out))?;

    // Update one of the user's rows (set updated=1) using id predicate
    let upd = format!(
        "UPDATE {} SET updated = 1 WHERE id = {}",
        full_table, row_b_id
    );
    execute_sql_via_cli_as(&user_name, user_pass, &upd)?;

    // Delete one of the user's rows using id predicate
    let del = format!("DELETE FROM {} WHERE id = {}", full_table, row_c_id);
    execute_sql_via_cli_as(&user_name, user_pass, &del)?;

    // 6) SELECT as the regular user and verify visibility
    let select_out = execute_sql_via_cli_as(
        &user_name,
        user_pass,
        &format!(
            "SELECT id, content, updated FROM {} ORDER BY id",
            full_table
        ),
    )?;

    // (a) user could insert (at least one of user's values appears)
    assert!(
        select_out.contains("user_row_a") || select_out.contains("user_row_b"),
        "Expected at least one user row to be present in selection, got: {}",
        select_out
    );

    // (b) CLI login succeeded implicitly through previous commands; also ensured via SELECT 1

    // (c) ensure no root rows are visible
    for r in &root_rows {
        assert!(
            !select_out.contains(r),
            "User selection should not contain root row '{}': {}",
            r,
            select_out
        );
    }

    // Ensure update took effect and delete removed the row
    assert!(
        select_out.contains("user_row_b"),
        "Expected updated row to be present"
    );
    assert!(
        !select_out.contains("user_row_c"),
        "Expected deleted row to be absent"
    );

    // Cleanup (best-effort)
    let _ = execute_sql_as_root_via_cli(&format!("DROP USER {}", user_name));
    let _ = execute_sql_as_root_via_cli(&format!("DROP TABLE IF EXISTS {}", full_table));
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {}", namespace));

    Ok(())
}
