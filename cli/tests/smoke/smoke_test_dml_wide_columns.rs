// Smoke Test: DML across wide columns for USER and SHARED tables
// Covers: insert -> check -> update -> check -> multi-column update -> check -> delete -> check
// Also verifies subscription delivers UPDATE and DELETE events

use crate::common::*;
use std::time::Duration;

fn create_namespace(ns: &str) {
    let _ = execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE IF NOT EXISTS {}", ns));
}

fn extract_first_id(output: &str) -> Option<i64> {
    // Parses the first numeric cell after a header line from a table-style output
    for line in output.lines() {
        if line.starts_with('│') && line.contains('│') && !line.to_lowercase().contains("user_id")
        {
            let parts: Vec<&str> = line.split('│').collect();
            if parts.len() >= 2 {
                let candidate = parts[1].trim();
                if let Ok(id) = candidate.parse::<i64>() {
                    return Some(id);
                }
            }
        }
    }
    None
}

// reserved: count extractor kept for potential future stricter checks
#[allow(dead_code)]
fn extract_single_count(output: &str) -> Option<i64> {
    for line in output.lines() {
        if line.starts_with('│') && line.contains('│') {
            for token in line.split('│').map(|s| s.trim()) {
                if let Ok(v) = token.parse::<i64>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn run_dml_sequence(full: &str, _is_shared: bool) {
    // insert row 1
    let ins1 = format!(
        "INSERT INTO {} (name, age, active, score, balance, note) VALUES ('alpha', 25, true, 12.5, 1000, 'note-a')",
        full
    );
    let out1 = execute_sql_as_root_via_cli(&ins1).expect("insert 1 should succeed");
    assert!(
        out1.contains("1 rows affected"),
        "expected 1 row affected: {}",
        out1
    );

    // insert row 2
    let ins2 = format!(
        "INSERT INTO {} (name, age, active, score, balance, note) VALUES ('beta', 30, false, 99.9, 2000, 'note-b')",
        full
    );
    let out2 = execute_sql_as_root_via_cli(&ins2).expect("insert 2 should succeed");
    assert!(out2.contains("1 rows affected"));

    // check rows exist
    let sel_all = format!(
        "SELECT id, name, age, active, score, balance, created_at, note FROM {} ORDER BY id",
        full
    );
    let out_sel = execute_sql_as_root_via_cli(&sel_all).expect("select should succeed");
    assert!(
        out_sel.contains("alpha") && out_sel.contains("beta"),
        "expected both rows present: {}",
        out_sel
    );

    // parse alpha id via focused query to avoid brittle table parsing
    let sel_alpha_id = format!("SELECT id FROM {} WHERE name = 'alpha'", full);
    let out_alpha = execute_sql_as_root_via_cli(&sel_alpha_id).expect("select alpha id");
    let id_alpha = extract_first_id(&out_alpha).expect("should parse alpha id");

    // update single column on alpha
    let upd1 = format!("UPDATE {} SET age = 26 WHERE id = {}", full, id_alpha);
    let out_upd1 = execute_sql_as_root_via_cli(&upd1).expect("update 1 should succeed");
    assert!(out_upd1.contains("1 rows affected"));

    // check
    let out_chk1 = execute_sql_as_root_via_cli(&sel_all).expect("post update select");
    assert!(
        out_chk1.contains("26"),
        "expected updated age 26: {}",
        out_chk1
    );

    // multi-column update on alpha
    let upd2 = format!(
        "UPDATE {} SET name='alpha2', age=42, active=false, score=88.75, balance=4321, note='note-upd' WHERE id = {}",
        full, id_alpha
    );
    let out_upd2 = execute_sql_as_root_via_cli(&upd2).expect("update 2 should succeed");
    assert!(out_upd2.contains("1 rows affected"));

    // check
    let out_chk2 = execute_sql_as_root_via_cli(&sel_all).expect("post multi update select");
    assert!(
        out_chk2.contains("alpha2")
            && out_chk2.contains("42")
            && out_chk2.to_lowercase().contains("false"),
        "expected multi-column updates reflected: {}",
        out_chk2
    );

    // delete beta row by filtering on name to get id, then delete by id to respect pk equality
    let sel_beta = format!("SELECT id, name FROM {} WHERE name = 'beta'", full);
    let out_beta = execute_sql_as_root_via_cli(&sel_beta).expect("select beta id");
    let beta_id = extract_first_id(&out_beta).expect("should parse beta id");

    let del = format!("DELETE FROM {} WHERE id = {}", full, beta_id);
    let out_del = execute_sql_as_root_via_cli(&del).expect("delete should succeed");
    assert!(out_del.contains("1 rows affected"));

    // best-effort final check: ensure updated row still present
    let out_after = execute_sql_as_root_via_cli(&sel_all).expect("final select after delete");
    assert!(
        out_after.contains("alpha2"),
        "expected updated row present: {}",
        out_after
    );

    // Note: subscription validations are covered in dedicated test below
}

#[test]
fn smoke_user_table_dml_wide_columns() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("smoke_ns");
    let table = generate_unique_table("user_dml_wide");
    let full = format!("{}.{}", namespace, table);

    create_namespace(&namespace);

    // create USER table with 8+ columns of various types
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR NOT NULL,
            age INT,
            active BOOLEAN,
            score DOUBLE,
            balance BIGINT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            note VARCHAR
        ) WITH (
            TYPE = 'USER',
            FLUSH_POLICY = 'rows:10'
        )"#,
        full
    );
    execute_sql_as_root_via_cli(&create_sql).expect("create user table should succeed");

    run_dml_sequence(&full, false);
}

#[test]
fn smoke_shared_table_dml_wide_columns() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("smoke_ns");
    let table = generate_unique_table("shared_dml_wide");
    let full = format!("{}.{}", namespace, table);

    create_namespace(&namespace);

    // create SHARED table with the same schema
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR NOT NULL,
            age INT,
            active BOOLEAN,
            score DOUBLE,
            balance BIGINT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            note VARCHAR
        ) WITH (
            TYPE = 'SHARED',
            FLUSH_POLICY = 'rows:10'
        )"#,
        full
    );
    execute_sql_as_root_via_cli(&create_sql).expect("create shared table should succeed");

    run_dml_sequence(&full, true);
}

// Subscription coverage for UPDATE and DELETE notifications on a user table with
// _updated/_deleted columns. Marked ignored due to flakiness in CI environments.
#[test]
fn smoke_subscription_update_delete_notifications() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("smoke_ns");
    let table = generate_unique_table("user_sub_dml");
    let full = format!("{}.{}", namespace, table);

    create_namespace(&namespace);

    let create_sql = format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, name VARCHAR, updated_at TIMESTAMP, is_deleted BOOLEAN, note VARCHAR) WITH (TYPE = 'USER')",
        full
    );
    execute_sql_as_root_via_cli(&create_sql).expect("create user table should succeed");

    // Insert initial row BEFORE subscribing
    let _ = execute_sql_as_root_via_cli(&format!(
        "INSERT INTO {} (id, name, updated_at, is_deleted, note) VALUES (1, 'one', 1730497770045, false, 'n1')",
        full
    ));

    // Start subscription
    let query = format!("SELECT * FROM {}", full);
    let mut listener = SubscriptionListener::start(&query).expect("subscription should start");

    // Expect BATCH with at least 1 row
    let snapshot_line = listener
        .wait_for_event("BATCH", Duration::from_secs(5))
        .expect("expected BATCH line");
    assert!(snapshot_line.contains("1 rows") || snapshot_line.contains("1 row"));

    // UPDATE
    let _ = execute_sql_as_root_via_cli(&format!("UPDATE {} SET name='one-upd' WHERE id=1", full));
    let update_line = listener
        .wait_for_event("UPDATE", Duration::from_secs(5))
        .expect("expected UPDATE event");
    assert!(update_line.contains("one-upd"));

    // DELETE
    let _ = execute_sql_as_root_via_cli(&format!("DELETE FROM {} WHERE id=1", full));
    let delete_line = listener
        .wait_for_event("DELETE", Duration::from_secs(5))
        .expect("expected DELETE event");
    assert!(delete_line.contains("\"id\":1") || delete_line.contains("one-upd"));

    listener.stop().ok();
}
