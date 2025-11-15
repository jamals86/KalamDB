use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn stream_table_select_1() -> anyhow::Result<()> {
    // Setup: Create stream table with 1000 rows
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    let setup_sql = "INSERT INTO bench_stream.events (id, value) SELECT generate_series(1, 1000), 'event_' || generate_series(1, 1000)";
    execute_cli_timed_root(setup_sql)?;

    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");

    // Execute SELECT for 1 row
    let sql = "SELECT * FROM bench_stream.events WHERE id = 1";
    let execution = execute_cli_timed_root(sql)?;

    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");

    // Create result
    let mut result = TestResult::new(
        "STR_SEL_1",
        TestGroup::StreamTable,
        "select",
        "Select 1 row from stream table",
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
