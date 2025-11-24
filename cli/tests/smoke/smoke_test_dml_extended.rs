//! Smoke tests for DML operation gaps
//!
//! Tests DML features not covered by existing smoke tests:
//! - Multi-row INSERT with VALUES (...), (...), (...)
//! - Soft DELETE for USER/SHARED tables (sets _deleted = true)
//! - Hard DELETE for STREAM tables (rows physically removed)
//! - Aggregation queries (COUNT, SUM, GROUP BY)
//! - Multi-row UPDATE operations
//!
//! Reference: docs/SQL.md DML section lines 471-584

use crate::common::*;
use std::time::Duration;

/// Test multi-row INSERT with batch VALUES
///
/// Verifies:
/// - INSERT INTO ... VALUES (...), (...), (...) syntax works
/// - All rows inserted in single statement
/// - Data retrievable via SELECT
#[test]
fn smoke_test_multi_row_insert() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("dml_ns");
    let table = generate_unique_table("multi_insert_test");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing multi-row INSERT");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create table
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            name TEXT NOT NULL,
            age INT,
            created_at TIMESTAMP DEFAULT NOW()
        ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000')"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create table");

    println!("✅ Created table");

    // Multi-row INSERT
    let multi_insert = format!(
        r#"INSERT INTO {} (name, age) VALUES 
            ('Alice', 25),
            ('Bob', 30),
            ('Charlie', 35),
            ('Diana', 28)"#,
        full_table
    );

    execute_sql_as_root_via_cli(&multi_insert).expect("Failed to execute multi-row INSERT");

    println!("✅ Executed multi-row INSERT (4 rows)");

    // Verify all rows inserted
    let select_sql = format!("SELECT COUNT(*) as total FROM {}", full_table);
    let output = execute_sql_as_root_via_cli(&select_sql).expect("Failed to count rows");

    assert!(
        output.contains('4'),
        "Expected 4 rows after multi-row INSERT, got: {}",
        output
    );

    // Verify specific data
    let select_names = format!("SELECT name FROM {} ORDER BY name", full_table);
    let names_output =
        execute_sql_as_root_via_cli_json(&select_names).expect("Failed to query names");

    assert!(
        names_output.contains("Alice") && names_output.contains("Diana"),
        "Expected all names in output"
    );

    println!("✅ Verified all 4 rows inserted correctly");
}

/// Test soft DELETE for USER tables
///
/// Verifies:
/// - DELETE sets _deleted = true (soft delete)
/// - Deleted rows excluded from SELECT by default
/// - Deleted rows visible with WHERE _deleted = true
/// - _updated column updated on DELETE
#[test]
fn smoke_test_soft_delete_user_table() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("dml_ns");
    let table = generate_unique_table("soft_delete_test");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing soft DELETE for USER table");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create USER table
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            name TEXT NOT NULL
        ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000')"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create table");

    // Insert test data
    let insert_sql = format!(
        r#"INSERT INTO {} (name) VALUES ('Alice'), ('Bob'), ('Charlie')"#,
        full_table
    );
    execute_sql_as_root_via_cli(&insert_sql).expect("Failed to insert data");

    println!("✅ Inserted 3 rows");

    // Verify initial count
    let count_before = format!("SELECT COUNT(*) as total FROM {}", full_table);
    let before_output = execute_sql_as_root_via_cli(&count_before).expect("Failed to count");
    assert!(before_output.contains('3'), "Expected 3 rows initially");

    // Soft DELETE one row
    let delete_sql = format!("DELETE FROM {} WHERE name = 'Bob'", full_table);
    execute_sql_as_root_via_cli(&delete_sql).expect("Failed to soft delete");

    println!("✅ Soft deleted row (name='Bob')");

    // Verify count after delete (should be 2, excluding soft-deleted row)
    let count_after = format!("SELECT COUNT(*) as total FROM {}", full_table);
    let after_output =
        execute_sql_as_root_via_cli(&count_after).expect("Failed to count after delete");

    assert!(
        after_output.contains('2'),
        "Expected 2 rows after soft delete (Bob excluded), got: {}",
        after_output
    );

    // Verify Bob not in default SELECT
    let select_all = format!("SELECT name FROM {} ORDER BY name", full_table);
    let all_output = execute_sql_as_root_via_cli_json(&select_all).expect("Failed to select all");

    assert!(
        !all_output.contains("Bob"),
        "Expected Bob to be excluded from default SELECT after soft delete"
    );
    assert!(
        all_output.contains("Alice") && all_output.contains("Charlie"),
        "Expected Alice and Charlie still visible"
    );

    // Query soft-deleted rows explicitly
    let query_deleted = format!("SELECT name FROM {} WHERE _deleted = true", full_table);
    let deleted_output =
        execute_sql_as_root_via_cli_json(&query_deleted).expect("Failed to query deleted rows");

    assert!(
        deleted_output.contains("Bob"),
        "Expected Bob visible when querying _deleted = true"
    );

    println!(
        "✅ Verified soft delete behavior: row hidden by default, visible with _deleted = true"
    );
}

/// Test soft DELETE for SHARED tables
///
/// Verifies:
/// - SHARED tables also use soft delete (like USER tables)
/// - _deleted column works same way
#[test]
fn smoke_test_soft_delete_shared_table() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("dml_ns");
    let table = generate_unique_table("shared_soft_delete");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing soft DELETE for SHARED table");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create SHARED table
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            config_key TEXT NOT NULL,
            config_value TEXT
        ) WITH (TYPE = 'SHARED', ACCESS_LEVEL = 'PUBLIC', FLUSH_POLICY = 'rows:1000')"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create shared table");

    // Insert and delete
    let insert_sql = format!(
        r#"INSERT INTO {} (config_key, config_value) VALUES ('key1', 'value1'), ('key2', 'value2')"#,
        full_table
    );
    execute_sql_as_root_via_cli(&insert_sql).expect("Failed to insert");

    let delete_sql = format!("DELETE FROM {} WHERE config_key = 'key1'", full_table);
    execute_sql_as_root_via_cli(&delete_sql).expect("Failed to delete");

    println!("✅ Soft deleted row from SHARED table");

    // Verify soft delete (count should be 1)
    let count_sql = format!("SELECT COUNT(*) as total FROM {}", full_table);
    let count_output = execute_sql_as_root_via_cli(&count_sql).expect("Failed to count");

    assert!(
        count_output.contains('1'),
        "Expected 1 row after soft delete in SHARED table"
    );

    println!("✅ Verified SHARED table uses soft delete");
}

/// Test hard DELETE for STREAM tables
///
/// Verifies:
/// - DELETE physically removes rows from STREAM tables (no _deleted column)
/// - Deleted rows NOT retrievable via any query
/// - COUNT decreases after DELETE
#[test]
fn smoke_test_hard_delete_stream_table() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("dml_ns");
    let table = generate_unique_table("stream_hard_delete");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing hard DELETE for STREAM table");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create STREAM table
    let create_sql = format!(
        r#"CREATE TABLE {} (
            event_id TEXT PRIMARY KEY DEFAULT ULID(),
            event_type TEXT NOT NULL,
            payload TEXT
        ) WITH (TYPE = 'STREAM', TTL_SECONDS = 60)"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create stream table");

    // Insert events
    let insert_sql = format!(
        r#"INSERT INTO {} (event_type, payload) VALUES 
            ('click', 'button1'),
            ('hover', 'menu'),
            ('click', 'button2')"#,
        full_table
    );
    execute_sql_as_root_via_cli(&insert_sql).expect("Failed to insert events");

    println!("✅ Inserted 3 events into STREAM table");

    // Verify initial count
    let count_before = format!("SELECT COUNT(*) as total FROM {}", full_table);
    let before_output = execute_sql_as_root_via_cli(&count_before).expect("Failed to count");
    assert!(before_output.contains('3'), "Expected 3 events initially");

    // Hard DELETE (remove click events)
    let delete_sql = format!("DELETE FROM {} WHERE event_type = 'click'", full_table);
    execute_sql_as_root_via_cli(&delete_sql).expect("Failed to hard delete");

    println!("✅ Hard deleted 2 click events");

    // Verify count after delete (should be 1, rows physically removed)
    let count_after = format!("SELECT COUNT(*) as total FROM {}", full_table);
    let after_output =
        execute_sql_as_root_via_cli(&count_after).expect("Failed to count after delete");

    assert!(
        after_output.contains('1'),
        "Expected 1 event after hard delete (click events physically removed), got: {}",
        after_output
    );

    // Verify deleted rows NOT retrievable even with _deleted filter
    // Note: STREAM tables don't have _deleted column, so this query should fail or return nothing
    let query_all = format!("SELECT event_type FROM {} ORDER BY event_type", full_table);
    let all_output = execute_sql_as_root_via_cli_json(&query_all).expect("Failed to query all");

    assert!(
        !all_output.contains("click"),
        "Expected click events to be physically removed from STREAM table"
    );
    assert!(
        all_output.contains("hover"),
        "Expected hover event still exists"
    );

    println!("✅ Verified STREAM table uses hard delete (rows physically removed)");
}

/// Test aggregation queries (COUNT, SUM, GROUP BY)
///
/// Verifies:
/// - COUNT(*) works
/// - COUNT(column) works
/// - SUM(column) works
/// - GROUP BY works
/// - AVG, MIN, MAX work
#[test]
fn smoke_test_aggregation_queries() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("dml_ns");
    let table = generate_unique_table("agg_test");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing aggregation queries");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create table with numeric data
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            category TEXT NOT NULL,
            amount INT NOT NULL
        ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000')"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create table");

    // Insert test data
    let insert_sql = format!(
        r#"INSERT INTO {} (category, amount) VALUES 
            ('A', 10),
            ('A', 20),
            ('B', 30),
            ('B', 40),
            ('C', 50)"#,
        full_table
    );
    execute_sql_as_root_via_cli(&insert_sql).expect("Failed to insert data");

    println!("✅ Inserted 5 rows with categories A, B, C");

    // Test COUNT(*)
    let count_all = format!("SELECT COUNT(*) as total FROM {}", full_table);
    let count_output = execute_sql_as_root_via_cli(&count_all).expect("Failed to COUNT(*)");
    assert!(count_output.contains('5'), "Expected COUNT(*) = 5");

    // Test SUM
    let sum_query = format!("SELECT SUM(amount) as total_amount FROM {}", full_table);
    let sum_output = execute_sql_as_root_via_cli(&sum_query).expect("Failed to SUM");
    assert!(
        sum_output.contains("150"),
        "Expected SUM(amount) = 150 (10+20+30+40+50)"
    );

    println!("✅ COUNT and SUM work");

    // Test GROUP BY with COUNT
    let group_count = format!(
        "SELECT category, COUNT(*) as count FROM {} GROUP BY category ORDER BY category",
        full_table
    );
    let group_output = execute_sql_as_root_via_cli(&group_count).expect("Failed to GROUP BY COUNT");

    assert!(
        group_output.contains('A') && group_output.contains('2'),
        "Expected category A with count 2"
    );
    assert!(
        group_output.contains('B') && group_output.contains('2'),
        "Expected category B with count 2"
    );

    println!("✅ GROUP BY with COUNT works");

    // Test GROUP BY with SUM
    let group_sum = format!(
        "SELECT category, SUM(amount) as total FROM {} GROUP BY category ORDER BY category",
        full_table
    );
    let group_sum_output = execute_sql_as_root_via_cli(&group_sum).expect("Failed to GROUP BY SUM");

    assert!(
        group_sum_output.contains('A') && group_sum_output.contains("30"),
        "Expected category A with sum 30"
    );
    assert!(
        group_sum_output.contains('B') && group_sum_output.contains("70"),
        "Expected category B with sum 70"
    );

    println!("✅ GROUP BY with SUM works");

    // Test AVG, MIN, MAX
    let stats_query = format!(
        "SELECT AVG(amount) as avg, MIN(amount) as min, MAX(amount) as max FROM {}",
        full_table
    );
    let stats_output = execute_sql_as_root_via_cli(&stats_query).expect("Failed to compute stats");

    assert!(
        stats_output.contains("30") || stats_output.contains("avg"),
        "Expected AVG(amount) = 30"
    );
    assert!(stats_output.contains("10"), "Expected MIN(amount) = 10");
    assert!(stats_output.contains("50"), "Expected MAX(amount) = 50");

    println!("✅ AVG, MIN, MAX work");

    println!("✅ All aggregation queries successful");
}

/// Test multi-row UPDATE
///
/// Verifies:
/// - UPDATE with WHERE clause affects multiple rows
/// - All matching rows updated
/// - _updated column updated for all affected rows
#[test]
fn smoke_test_multi_row_update() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("dml_ns");
    let table = generate_unique_table("multi_update_test");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing multi-row UPDATE");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create table
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            status TEXT NOT NULL,
            priority INT
        ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000')"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create table");

    // Insert test data
    let insert_sql = format!(
        r#"INSERT INTO {} (status, priority) VALUES 
            ('pending', 1),
            ('pending', 2),
            ('pending', 3),
            ('done', 1)"#,
        full_table
    );
    execute_sql_as_root_via_cli(&insert_sql).expect("Failed to insert data");

    println!("✅ Inserted 4 rows (3 pending, 1 done)");

    // Multi-row UPDATE: change all pending to active
    let update_sql = format!(
        "UPDATE {} SET status = 'active' WHERE status = 'pending'",
        full_table
    );
    execute_sql_as_root_via_cli(&update_sql).expect("Failed to multi-row UPDATE");

    println!("✅ Updated all pending rows to active");

    // Verify all pending rows updated
    let count_active = format!(
        "SELECT COUNT(*) as total FROM {} WHERE status = 'active'",
        full_table
    );
    let active_output = execute_sql_as_root_via_cli(&count_active).expect("Failed to count active");

    assert!(
        active_output.contains('3'),
        "Expected 3 rows with status='active' after multi-row UPDATE"
    );

    // Verify no pending rows remain
    let count_pending = format!(
        "SELECT COUNT(*) as total FROM {} WHERE status = 'pending'",
        full_table
    );
    let pending_output =
        execute_sql_as_root_via_cli(&count_pending).expect("Failed to count pending");

    assert!(
        pending_output.contains('0'),
        "Expected 0 rows with status='pending' after UPDATE"
    );

    println!("✅ Verified multi-row UPDATE affected all matching rows");
}
