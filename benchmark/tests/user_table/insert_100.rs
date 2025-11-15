use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn user_table_insert_100() -> anyhow::Result<()> {
    // Setup
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute 100 inserts
    let mut total_cli_ms = 0.0;
    let mut total_server_ms = 0.0;
    
    for i in 1..=100 {
        let sql = format!(
            "INSERT INTO bench_user.items (value) VALUES ('benchmark_value_{}')",
            i
        );
        let execution = execute_cli_timed_root(&sql)?;
        total_cli_ms += execution.cli_total_ms;
        total_server_ms += execution.server_time_ms;
    }
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "USR_INS_100",
        TestGroup::UserTable,
        "insert",
        "Insert 100 rows into user table",
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
