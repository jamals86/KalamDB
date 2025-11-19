//! End-to-end subscription test: verifies initial snapshot + change events

mod common;
use common::*;
use std::time::Duration;

#[test]
fn test_cli_subscription_initial_and_changes() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Prepare unique namespace.table
    let namespace = generate_unique_namespace("batch_test");
    let table_full = format!("{}.items", namespace);

    // Ensure namespace exists
    let _ = execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace));
    std::thread::sleep(Duration::from_millis(150));

    // Create user table
    let _ = execute_sql_as_root_via_cli(&format!(
        "CREATE USER TABLE {} (id INT PRIMARY KEY, name VARCHAR)",
        table_full
    ));
    std::thread::sleep(Duration::from_millis(150));

    // Insert initial row BEFORE subscribing
    let _ = execute_sql_as_root_via_cli(&format!(
        "INSERT INTO {} (id, name) VALUES (1, 'Item One')",
        table_full
    ));
    std::thread::sleep(Duration::from_millis(150));

    // Start subscription via CLI
    let query = format!("SELECT * FROM {}", table_full);
    let mut listener = match SubscriptionListener::start(&query) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("⚠️  Failed to start subscription: {}. Skipping test.", e);
            let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE {} CASCADE", namespace));
            return;
        }
    };

    // Expect a BATCH line with 1 row
    let snapshot_line = listener
        .wait_for_event("BATCH", Duration::from_secs(5))
        .expect("expected BATCH line");
    assert!(
        snapshot_line.contains(" 1 rows "),
        "Expected initial snapshot with 1 row, got: {}",
        snapshot_line
    );

    // Perform INSERT change and wait for INSERT event
    let _ = execute_sql_as_root_via_cli(&format!(
        "INSERT INTO {} (id, name) VALUES (2, 'Second')",
        table_full
    ));
    let insert_line = listener
        .wait_for_event("INSERT", Duration::from_secs(5))
        .expect("expected INSERT event");
    assert!(
        insert_line.contains("\"id\":2") || insert_line.contains("Second"),
        "INSERT payload should include id=2 or name='Second': {}",
        insert_line
    );

    // Perform UPDATE and wait for UPDATE event
    let _ = execute_sql_as_root_via_cli(&format!(
        "UPDATE {} SET name = 'Updated Second' WHERE id = 2",
        table_full
    ));
    let update_line = listener
        .wait_for_event("UPDATE", Duration::from_secs(5))
        .expect("expected UPDATE event");
    assert!(
        update_line.contains("Updated Second"),
        "UPDATE payload should include updated value: {}",
        update_line
    );

    // Perform DELETE and wait for DELETE event
    let _ = execute_sql_as_root_via_cli(&format!("DELETE FROM {} WHERE id = 2", table_full));
    let delete_line = listener
        .wait_for_event("DELETE", Duration::from_secs(5))
        .expect("expected DELETE event");
    assert!(
        delete_line.contains("\"id\":2") || delete_line.contains("Updated Second"),
        "DELETE payload should include id=2 or last value: {}",
        delete_line
    );

    // Cleanup
    listener.stop().ok();
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE {} CASCADE", namespace));
}
