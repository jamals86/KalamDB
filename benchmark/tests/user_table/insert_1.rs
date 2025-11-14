use kalamdb_benchmark::*;
use std::time::Duration;

#[test]
fn user_table_insert_1() -> anyhow::Result<()> {
    // Setup
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute insert
    let sql = "INSERT INTO bench_user.items (value) VALUES ('benchmark_value_1')";
    let execution = execute_cli_timed_root(sql)?;
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "USR_INS_1",
        TestGroup::UserTable,
        "insert",
        "Insert 1 row into user table",
    );
    
    result.set_timings(
        execution.cli_total_ms,
        execution.server_time_ms,
        execution.server_time_ms, // SQL execution is same as server time for single query
    );
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(1, execution.server_time_ms);
    result.validate();
    
    // Write result
    let path = append_test_result(result)?;
    println!("✅ Benchmark result written to: {}", path.display());
    
    // Cleanup
    cleanup_benchmark_tables()?;
    
    Ok(())
}
