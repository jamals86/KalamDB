mod common;
use common::*;
use std::thread;
use std::time::Duration;

/// Test updating all data types in a USER table
#[test]
fn test_update_all_types_user_table() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table_name = generate_unique_table("all_types_user");
    let namespace = "test_update_types";
    let full_table_name = format!("{}.{}", namespace, table_name);

    // Setup namespace
    let _ = execute_sql_via_cli(&format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace));

    // Create table with all supported types
    // Note: Skipping EMBEDDING and BYTES for simplicity in SQL literals for now, 
    // but covering all scalar types.
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id VARCHAR PRIMARY KEY,
            col_bool BOOLEAN,
            col_int INT,
            col_bigint BIGINT,
            col_double DOUBLE,
            col_float FLOAT,
            col_text TEXT,
            col_timestamp TIMESTAMP,
            col_date DATE,
            col_datetime DATETIME,
            col_time TIME,
            col_json JSON,
            col_uuid UUID,
            col_decimal DECIMAL(10, 2),
            col_smallint SMALLINT
        ) WITH (TYPE='USER', FLUSH_POLICY='rows:10')"#,
        full_table_name
    );

    let output = execute_sql_as_root_via_cli(&create_sql).unwrap();
    assert!(output.contains("created") || output.contains("Success"), "Table creation failed: {}", output);

    // Insert initial data
    let insert_sql = format!(
        r#"INSERT INTO {} (
            id, col_bool, col_int, col_bigint, col_double, col_float, col_text, 
            col_timestamp, col_date, col_datetime, col_time, col_json, col_uuid, 
            col_decimal, col_smallint
        ) VALUES (
            'row1', true, 123, 1234567890, 123.45, 12.34, 'initial text',
            '2023-01-01 10:00:00', '2023-01-01', '2023-01-01 10:00:00', '10:00:00',
            '{{"key": "initial"}}', '550e8400-e29b-41d4-a716-446655440000', 100.50, 100
        )"#,
        full_table_name
    );
    
    let output = execute_sql_as_root_via_cli(&insert_sql).unwrap();
    assert!(output.contains("1 row(s) affected") || output.contains("1 rows affected") || output.contains("Inserted 1 row(s)") || output.contains("Success"), "Insert failed: {}", output);

    // Verify initial data
    let query_sql = format!("SELECT * FROM {} WHERE id = 'row1'", full_table_name);
    let output = execute_sql_as_root_via_cli_json(&query_sql).unwrap();
    assert!(output.contains("initial text"), "Initial data not found: {}", output);
    assert!(output.contains("123"), "Initial int not found");

    // Update all columns
    let update_sql = format!(
        r#"UPDATE {} SET 
            col_bool = false,
            col_int = 456,
            col_bigint = 9876543210,
            col_double = 987.65,
            col_float = 56.78,
            col_text = 'updated text',
            col_timestamp = '2023-12-31 23:59:59',
            col_date = '2023-12-31',
            col_datetime = '2023-12-31 23:59:59',
            col_time = '23:59:59',
            col_json = '{{"key": "updated"}}',
            col_uuid = '123e4567-e89b-12d3-a456-426614174000',
            col_decimal = 200.75,
            col_smallint = 200
        WHERE id = 'row1'"#,
        full_table_name
    );

    let output = execute_sql_as_root_via_cli(&update_sql).unwrap();
    assert!(output.contains("1 row(s) affected") || output.contains("1 rows affected") || output.contains("Updated 1 row(s)") || output.contains("Success"), "Update failed: {}", output);

    // Verify updated data (before flush)
    let output = execute_sql_as_root_via_cli_json(&query_sql).unwrap();
    assert!(output.contains("updated text"), "Updated text not found: {}", output);
    assert!(output.contains("456"), "Updated int not found");
    assert!(output.contains("200.75"), "Updated decimal not found");
    // Note: JSON formatting might vary (whitespace), so we might need loose check or just check presence of "updated"
    assert!(output.contains("updated"), "Updated JSON content not found");

    // Flush table
    let flush_sql = format!("FLUSH TABLE {}", full_table_name);
    let output = execute_sql_as_root_via_cli(&flush_sql).unwrap();
    
    // Wait for flush to complete
    if let Ok(job_id) = parse_job_id_from_flush_output(&output) {
        println!("Waiting for flush job {}...", job_id);
        if let Err(e) = verify_job_completed(&job_id, Duration::from_secs(10)) {
            eprintln!("Flush job failed or timed out: {}", e);
            // Continue anyway to see if data is readable
        }
    } else {
        // If we can't parse job ID, just wait a bit
        thread::sleep(Duration::from_secs(2));
    }

    // Verify updated data (after flush)
    let output = execute_sql_as_root_via_cli_json(&query_sql).unwrap();
    assert!(output.contains("updated text"), "Updated text not found after flush: {}", output);
    assert!(output.contains("456"), "Updated int not found after flush");
    assert!(output.contains("200.75"), "Updated decimal not found after flush");

    // Cleanup
    let _ = execute_sql_via_cli(&format!("DROP TABLE IF EXISTS {}", full_table_name));
}

/// Test updating all data types in a SHARED table
#[test]
fn test_update_all_types_shared_table() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table_name = generate_unique_table("all_types_shared");
    let namespace = "test_update_types";
    let full_table_name = format!("{}.{}", namespace, table_name);

    // Setup namespace
    let _ = execute_sql_via_cli(&format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace));

    // Create table with all supported types
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id VARCHAR PRIMARY KEY,
            col_bool BOOLEAN,
            col_int INT,
            col_bigint BIGINT,
            col_double DOUBLE,
            col_float FLOAT,
            col_text TEXT,
            col_timestamp TIMESTAMP,
            col_date DATE,
            col_datetime DATETIME,
            col_time TIME,
            col_json JSON,
            col_uuid UUID,
            col_decimal DECIMAL(10, 2),
            col_smallint SMALLINT
        ) WITH (TYPE='SHARED', FLUSH_POLICY='rows:10')"#,
        full_table_name
    );

    let output = execute_sql_as_root_via_cli(&create_sql).unwrap();
    assert!(output.contains("created") || output.contains("Success"), "Table creation failed: {}", output);

    // Insert initial data
    let insert_sql = format!(
        r#"INSERT INTO {} (
            id, col_bool, col_int, col_bigint, col_double, col_float, col_text, 
            col_timestamp, col_date, col_datetime, col_time, col_json, col_uuid, 
            col_decimal, col_smallint
        ) VALUES (
            'row1', true, 123, 1234567890, 123.45, 12.34, 'initial text',
            '2023-01-01 10:00:00', '2023-01-01', '2023-01-01 10:00:00', '10:00:00',
            '{{"key": "initial"}}', '550e8400-e29b-41d4-a716-446655440000', 100.50, 100
        )"#,
        full_table_name
    );
    
    let output = execute_sql_as_root_via_cli(&insert_sql).unwrap();
    assert!(output.contains("1 row(s) affected") || output.contains("1 rows affected") || output.contains("Inserted 1 row(s)") || output.contains("Success"), "Insert failed: {}", output);

    // Verify initial data
    let query_sql = format!("SELECT * FROM {} WHERE id = 'row1'", full_table_name);
    let output = execute_sql_as_root_via_cli_json(&query_sql).unwrap();
    assert!(output.contains("initial text"), "Initial data not found: {}", output);
    assert!(output.contains("123"), "Initial int not found");

    // Update all columns
    let update_sql = format!(
        r#"UPDATE {} SET 
            col_bool = false,
            col_int = 456,
            col_bigint = 9876543210,
            col_double = 987.65,
            col_float = 56.78,
            col_text = 'updated text',
            col_timestamp = '2023-12-31 23:59:59',
            col_date = '2023-12-31',
            col_datetime = '2023-12-31 23:59:59',
            col_time = '23:59:59',
            col_json = '{{"key": "updated"}}',
            col_uuid = '123e4567-e89b-12d3-a456-426614174000',
            col_decimal = 200.75,
            col_smallint = 200
        WHERE id = 'row1'"#,
        full_table_name
    );

    let output = execute_sql_as_root_via_cli(&update_sql).unwrap();
    assert!(output.contains("1 row(s) affected") || output.contains("1 rows affected") || output.contains("Updated 1 row(s)") || output.contains("Success"), "Update failed: {}", output);

    // Verify updated data (before flush)
    let output = execute_sql_as_root_via_cli_json(&query_sql).unwrap();
    assert!(output.contains("updated text"), "Updated text not found: {}", output);
    assert!(output.contains("456"), "Updated int not found");
    assert!(output.contains("200.75"), "Updated decimal not found");
    assert!(output.contains("updated"), "Updated JSON content not found");

    // Flush table
    let flush_sql = format!("FLUSH TABLE {}", full_table_name);
    let output = execute_sql_as_root_via_cli(&flush_sql).unwrap();
    
    // Wait for flush to complete
    if let Ok(job_id) = parse_job_id_from_flush_output(&output) {
        println!("Waiting for flush job {}...", job_id);
        if let Err(e) = verify_job_completed(&job_id, Duration::from_secs(10)) {
            eprintln!("Flush job failed or timed out: {}", e);
        }
    } else {
        thread::sleep(Duration::from_secs(2));
    }

    // Verify updated data (after flush)
    let output = execute_sql_as_root_via_cli_json(&query_sql).unwrap();
    assert!(output.contains("updated text"), "Updated text not found after flush: {}", output);
    assert!(output.contains("456"), "Updated int not found after flush");
    assert!(output.contains("200.75"), "Updated decimal not found after flush");

    // Cleanup
    let _ = execute_sql_via_cli(&format!("DROP TABLE IF EXISTS {}", full_table_name));
}
