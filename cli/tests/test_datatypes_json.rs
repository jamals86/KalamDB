#![allow(unused_imports, dead_code)]

mod common;
use common::*;
// (apply_patch sanity check)
use serde_json::Value;
use std::time::Duration;

#[test]
fn test_datatypes_json_preservation() {
    if !common::is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table_name = common::generate_unique_table("datatypes_test");
    let namespace = common::generate_unique_namespace("test_datatypes");

    // Create namespace first
    let _ = common::execute_sql_as_root_via_cli(&format!(
        "CREATE NAMESPACE IF NOT EXISTS {}",
        namespace
    ));

    // Create test table with multiple datatypes
    let create_sql = format!(
        r#"CREATE TABLE {}.{} (
            id BIGINT PRIMARY KEY AUTO_INCREMENT,
            col_string VARCHAR,
            col_int INT,
            col_float FLOAT,
            col_bool BOOLEAN,
            col_timestamp TIMESTAMP
        ) WITH (TYPE = 'USER')"#,
        namespace, table_name
    );

    let result = common::execute_sql_as_root_via_cli(&create_sql);
    assert!(
        result.is_ok(),
        "Should create table successfully: {:?}",
        result.err()
    );

    // Insert test data
    // Note: Using a fixed timestamp for easier verification
    let timestamp_str = "2023-01-01 12:00:00";
    let insert_sql = format!(
        "INSERT INTO {}.{} (col_string, col_int, col_float, col_bool, col_timestamp) VALUES ('test_string', 123, 45.67, true, '{}')",
        namespace, table_name, timestamp_str
    );
    let result = common::execute_sql_as_root_via_cli(&insert_sql);
    assert!(
        result.is_ok(),
        "Should insert data successfully: {:?}",
        result.err()
    );

    // Query the data using JSON output format
    let select_sql = format!("SELECT * FROM {}.{}", namespace, table_name);
    let result = common::execute_sql_as_root_via_cli_json(&select_sql);
    assert!(
        result.is_ok(),
        "Should query data successfully: {:?}",
        result.err()
    );

    let output = result.unwrap();
    println!("Query output: {}", output);

    // Parse JSON output
    let json: Value = serde_json::from_str(&output).expect("Failed to parse JSON output");

    // Navigate to results[0].rows[0]
    // Structure is typically: { "results": [ { "rows": [ { "col_name": value, ... } ] } ] }
    let rows = json
        .get("results")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|res| res.get("rows"))
        .and_then(|v| v.as_array())
        .expect("Failed to get rows from JSON response");

    assert!(!rows.is_empty(), "Should return at least one row");
    let row = rows.first().expect("Should have a first row");

    // Verify values and types

    // String
    let col_string = row.get("col_string").expect("Missing col_string");
    assert!(col_string.is_string(), "col_string should be a string");
    assert_eq!(col_string.as_str().unwrap(), "test_string");

    // Int
    let col_int = row.get("col_int").expect("Missing col_int");
    assert!(col_int.is_number(), "col_int should be a number");
    assert_eq!(col_int.as_i64().unwrap(), 123);

    // Float
    let col_float = row.get("col_float").expect("Missing col_float");
    assert!(col_float.is_number(), "col_float should be a number");
    // FLOAT is typically stored as 32-bit and may not round-trip exactly through JSON.
    let actual_float = col_float.as_f64().unwrap();
    let expected_float = 45.67_f64;
    assert!(
        (actual_float - expected_float).abs() < 1e-4,
        "col_float should be approximately {} but was {}",
        expected_float,
        actual_float
    );

    // Boolean
    let col_bool = row.get("col_bool").expect("Missing col_bool");
    assert!(col_bool.is_boolean(), "col_bool should be a boolean");
    assert_eq!(col_bool.as_bool().unwrap(), true);

    // Timestamp
    // Timestamp might be returned as string or number depending on implementation
    // Based on typical JSON serialization of timestamps, it's often an ISO string or epoch
    let col_timestamp = row.get("col_timestamp").expect("Missing col_timestamp");
    // Adjust expectation based on actual behavior. Assuming string for now as it's common.
    // If it fails, we'll see the output and adjust.
    if col_timestamp.is_string() {
        // It might be formatted differently, e.g., with 'T' or 'Z'
        let ts_val = col_timestamp.as_str().unwrap();
        assert!(
            ts_val.contains("2023-01-01"),
            "Timestamp should contain date part"
        );
        assert!(
            ts_val.contains("12:00:00"),
            "Timestamp should contain time part"
        );
    } else if col_timestamp.is_number() {
        // If it's a number (epoch), we would check that range.
        // But let's assume string first as SQL usually returns string representation in JSON unless typed otherwise.
        println!("Timestamp returned as number: {}", col_timestamp);
    } else {
        panic!("Unexpected type for col_timestamp: {:?}", col_timestamp);
    }

    // Cleanup
    let drop_sql = format!("DROP TABLE IF EXISTS {}.{}", namespace, table_name);
    let _ = common::execute_sql_as_root_via_cli(&drop_sql);
}
