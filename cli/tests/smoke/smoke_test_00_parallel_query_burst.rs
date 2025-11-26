// High-concurrency smoke test to ensure SELECT * queries don't starve under load
// Creates a user table with ~1k rows and fires 40 parallel SELECT * queries.

use crate::common::*;
use serde_json::Value;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const PARALLEL_QUERIES: usize = 500;
const ROW_TARGET: usize = 500;
const INSERT_CHUNK_SIZE: usize = 1000;
const MAX_QUERY_DURATION: Duration = Duration::from_secs(30);
const MAX_ROWS_IN_TABLE: usize = 2_000;

#[ntest::timeout(120000)]
#[test]
fn smoke_test_00_parallel_query_burst() {
    if !is_server_running() {
        println!(
            "Skipping smoke_test_00_parallel_query_burst: server not running at {}",
            SERVER_URL
        );
        return;
    }

    println!("\n=== Starting Parallel Query Burst Smoke Test ===\n");

    let namespace = "smoke_parallel";
    let table = "user_table";
    let full_table_name = format!("{}.{}", namespace, table);

    // let _cleanup = NamespaceGuard::new(namespace.to_string());

    // Ensure namespace exists
    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace))
        .expect("CREATE NAMESPACE should succeed");

    // Create the user table with simple schema
    let create_table_sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (id INT PRIMARY KEY, value VARCHAR NOT NULL) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000')",
        full_table_name
    );
    execute_sql_as_root_via_cli(&create_table_sql).expect("CREATE TABLE should succeed");

    // Discover current row count and max(id) so we can cap growth to MAX_ROWS_IN_TABLE
    let count_sql = format!("SELECT COUNT(*) AS total_count FROM {}", full_table_name);
    let count_before_output = execute_sql_as_root_via_cli_json(&count_sql)
        .expect("COUNT(*) should succeed before inserts");
    let current_rows = extract_scalar(&count_before_output, "total_count") as usize;
    println!(
        "{} currently has {} rows (limit {}).",
        full_table_name, current_rows, MAX_ROWS_IN_TABLE
    );

    let max_id_sql = format!("SELECT COALESCE(MAX(id), -1) AS max_id FROM {}", full_table_name);
    let max_id_output = execute_sql_as_root_via_cli_json(&max_id_sql)
        .expect("MAX(id) query should succeed");
    let mut next_id = extract_scalar(&max_id_output, "max_id");
    if next_id < 0 {
        next_id = 0;
    }
    let next_id = next_id as usize;

    if current_rows >= MAX_ROWS_IN_TABLE {
        println!(
            "Table already at/above {} rows; skipping insert phase to respect cap.",
            MAX_ROWS_IN_TABLE
        );
    } else {
        let target_total = (current_rows + ROW_TARGET).min(MAX_ROWS_IN_TABLE);
        let rows_to_insert = target_total - current_rows;
        println!(
            "Inserting {} rows to reach {} total (next id starts at {}).",
            rows_to_insert, target_total, next_id
        );

        let mut inserted = 0;
        while inserted < rows_to_insert {
            let batch_size = usize::min(INSERT_CHUNK_SIZE, rows_to_insert - inserted);
            let values = (0..batch_size)
                .map(|offset| {
                    let id = next_id + inserted + offset;
                    format!("({}, 'payload_{}')", id, id)
                })
                .collect::<Vec<_>>()
                .join(", ");
            let insert_sql = format!("INSERT INTO {} (id, value) VALUES {}", full_table_name, values);
            execute_sql_as_root_via_cli(&insert_sql).expect("INSERT chunk should succeed");
            inserted += batch_size;
        }
    }

    println!("Issuing FLUSH TABLE {} to force persistence before load test", full_table_name);
    let flush_sql = format!("FLUSH TABLE {}", full_table_name);
    match execute_sql_as_root_via_cli(&flush_sql) {
        Ok(output) => println!("FLUSH TABLE acknowledged: {}", output.trim()),
        Err(err) => panic!("FLUSH TABLE {} failed: {}", full_table_name, err),
    }

    // Verify row count via JSON output for precise validation
    let count_output = execute_sql_as_root_via_cli_json(&count_sql)
        .expect("COUNT(*) should succeed for prepared table");
    let total_count = extract_scalar(&count_output, "total_count") as usize;
    println!("Verified row count: {}", total_count);

    println!(
        "Loaded {} rows into {}. Launching {} parallel SELECT * queries...",
        total_count, full_table_name, PARALLEL_QUERIES
    );

    let select_sql = Arc::new(format!("SELECT * FROM {}", full_table_name));
    let overall_start = Instant::now();
    let mut handles = Vec::with_capacity(PARALLEL_QUERIES);

    for idx in 0..PARALLEL_QUERIES {
        let sql_clone = Arc::clone(&select_sql);
        let suite_start = overall_start;

        handles.push(thread::spawn(move || {
            let launch = Instant::now();
            let offset = launch
                .checked_duration_since(suite_start)
                .unwrap_or_default();
            let result = execute_sql_as_root_via_cli(&sql_clone)
                .map_err(|e| format!("{}", e));
            let duration = launch.elapsed();
            (idx, result, offset, duration)
        }));
    }

    let mut durations = Vec::with_capacity(PARALLEL_QUERIES);

    for handle in handles {
        let (idx, result, offset, duration) = handle.join().expect("Query thread panicked");
        let output = result.unwrap_or_else(|e| panic!("Parallel query {} failed: {}", idx, e));
        assert!(
            output.contains("rows") || output.contains("│"),
            "Query {} output did not look like a result set: {}",
            idx,
            output
        );
        println!(
            "Query #{:02} completed in {:?} (started +{:?} from burst start)",
            idx, duration, offset
        );
        if duration > MAX_QUERY_DURATION {
            println!(
                "⚠️  Query #{:02} exceeded {:?} (took {:?})",
                idx, MAX_QUERY_DURATION, duration
            );
        }
        durations.push(duration);
    }

    let max_duration = durations
        .iter()
        .copied()
        .max()
        .unwrap_or_default();
    let avg_duration = if durations.is_empty() {
        Duration::from_secs(0)
    } else {
        let total_ns: u128 = durations
            .iter()
            .map(|d| d.as_nanos())
            .sum();
        Duration::from_nanos((total_ns / durations.len() as u128) as u64)
    };

    println!(
        "Parallel query burst finished in {:?} (max {:?}, avg {:?})",
        overall_start.elapsed(),
        max_duration,
        avg_duration
    );

    assert!(
        max_duration <= MAX_QUERY_DURATION,
        "Expected SELECT * calls to complete within {:?}; slowest took {:?}",
        MAX_QUERY_DURATION,
        max_duration
    );

    println!("\n=== Parallel Query Burst Smoke Test Complete ===\n");
}

fn extract_scalar(json_output: &str, field: &str) -> i64 {
    let value: Value = serde_json::from_str(json_output)
        .unwrap_or_else(|e| panic!("Failed to parse JSON output: {}", e));

    value
        .get("results")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|res| res.get("rows"))
        .and_then(|rows| rows.as_array())
        .and_then(|rows| rows.first())
        .and_then(|row| row.get(field))
        .and_then(|count| count.as_i64())
        .unwrap_or_else(|| panic!("JSON response missing field '{}': {}", field, json_output))
}

// struct NamespaceGuard(String);

// impl NamespaceGuard {
//     fn new(namespace: String) -> Self {
//         Self(namespace)
//     }
// }

// impl Drop for NamespaceGuard {
//     fn drop(&mut self) {
//         let drop_sql = format!("DROP NAMESPACE {} CASCADE", self.0);
//         let _ = execute_sql_as_root_via_cli(&drop_sql);
//     }
// }
