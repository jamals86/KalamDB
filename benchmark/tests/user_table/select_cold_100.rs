use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn user_table_select_cold_100() -> anyhow::Result<()> {
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
    
    // Flush to cold storage
    let flush_sql = "FLUSH TABLE bench_user.items";
    let flush_result = execute_cli_timed_root(flush_sql)?;
    let job_id = parse_job_id_from_flush(&flush_result.output)?;
    wait_for_flush_completion(&job_id, Duration::from_secs(30))?;
    std::thread::sleep(Duration::from_millis(200));
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute select from cold storage
    let select_sql = "SELECT * FROM bench_user.items LIMIT 100";
    let execution = execute_cli_timed_root(select_sql)?;
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "USR_SEL_COLD_100",
        TestGroup::UserTable,
        "select_cold",
        "SELECT 100 rows from cold storage (after flush)",
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
