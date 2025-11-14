use kalamdb_benchmark::*;
use std::time::Duration;

#[test]
fn user_table_update_100() -> anyhow::Result<()> {
    // Setup and insert data
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    for i in 1..=100 {
        let sql = format!(
            "INSERT INTO bench_user.items (value) VALUES ('benchmark_value_{}')",
            i
        );
        execute_cli_timed_root(&sql)?;
    }
    std::thread::sleep(Duration::from_millis(100));
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute updates
    let mut total_cli_ms = 0.0;
    let mut total_server_ms = 0.0;
    
    for i in 1..=100 {
        let update_sql = format!(
            "UPDATE bench_user.items SET value = 'updated_value_{}' WHERE value = 'benchmark_value_{}'",
            i, i
        );
        let execution = execute_cli_timed_root(&update_sql)?;
        total_cli_ms += execution.cli_total_ms;
        total_server_ms += execution.server_time_ms;
    }
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "USR_UPD_100",
        TestGroup::UserTable,
        "update",
        "Update 100 rows in user table",
    );
    
    result.set_timings(
        total_cli_ms,
        total_server_ms,
        total_server_ms,
    );
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(100, total_server_ms / 100.0);
    result.validate();
    
    // Write result
    let path = append_test_result(result)?;
    println!("✅ Benchmark result written to: {}", path.display());
    
    // Cleanup
    cleanup_benchmark_tables()?;
    
    Ok(())
}
