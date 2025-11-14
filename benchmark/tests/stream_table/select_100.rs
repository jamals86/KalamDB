use kalamdb_benchmark::*;
use std::time::Duration;

#[test]
fn stream_table_select_100() -> anyhow::Result<()> {
    // Setup and insert data
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    for i in 1..=100 {
        let sql = format!(
            "INSERT INTO bench_stream.events (event_type, payload) VALUES ('type_{0}', 'payload_{0}')",
            i
        );
        execute_cli_timed_root(&sql)?;
    }
    std::thread::sleep(Duration::from_millis(100));
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute select
    let select_sql = "SELECT * FROM bench_stream.events LIMIT 100";
    let execution = execute_cli_timed_root(select_sql)?;
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "STR_SEL_100",
        TestGroup::StreamTable,
        "select",
        "Select 100 rows from stream table",
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
