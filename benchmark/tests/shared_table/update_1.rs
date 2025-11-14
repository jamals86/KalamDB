use kalamdb_benchmark::*;
use std::time::Duration;

#[test]
fn shared_table_update_1() -> anyhow::Result<()> {
    // Setup and insert data
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    let sql = "INSERT INTO bench_shared.items (value) VALUES ('benchmark_value_1')";
    execute_cli_timed_root(sql)?;
    std::thread::sleep(Duration::from_millis(100));
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute update
    let update_sql = "UPDATE bench_shared.items SET value = 'updated_value_1' WHERE value = 'benchmark_value_1'";
    let execution = execute_cli_timed_root(update_sql)?;
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "SHR_UPD_1",
        TestGroup::SharedTable,
        "update",
        "Update 1 row in shared table",
    );
    
    result.set_timings(
        execution.cli_total_ms,
        execution.server_time_ms,
        execution.server_time_ms,
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
