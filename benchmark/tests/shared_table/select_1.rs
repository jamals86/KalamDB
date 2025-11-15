use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn shared_table_select_1() -> anyhow::Result<()> {
    // Setup and insert data
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    let sql = "INSERT INTO bench_shared.items (value) VALUES ('benchmark_value_1')";
    execute_cli_timed_root(sql)?;
    std::thread::sleep(Duration::from_millis(100));
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute select
    let select_sql = "SELECT * FROM bench_shared.items LIMIT 1";
    let execution = execute_cli_timed_root(select_sql)?;
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "SHR_SEL_1",
        TestGroup::SharedTable,
        "select",
        "Select 1 row from shared table",
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
