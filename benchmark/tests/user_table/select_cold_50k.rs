use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn user_table_select_cold_50k() -> anyhow::Result<()> {
    // Setup and insert data
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    // Insert 50k rows in batches
    let batch_size = 1000;
    let total_rows = 50000;
    let num_batches = total_rows / batch_size;
    
    for batch in 0..num_batches {
        let mut values = Vec::new();
        for i in 0..batch_size {
            let idx = batch * batch_size + i;
            values.push(format!("('benchmark_value_{}')", idx));
        }
        
        let sql = format!(
            "INSERT INTO bench_user.items (value) VALUES {}",
            values.join(", ")
        );
        execute_cli_timed_root(&sql)?;
    }
    std::thread::sleep(Duration::from_millis(200));
    
    // Flush to cold storage
    let flush_sql = "FLUSH TABLE bench_user.items";
    let flush_result = execute_cli_timed_root(flush_sql)?;
    let job_id = parse_job_id_from_flush(&flush_result.output)?;
    wait_for_flush_completion(&job_id, Duration::from_secs(60))?;
    std::thread::sleep(Duration::from_millis(200));
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute select from cold storage
    let select_sql = "SELECT * FROM bench_user.items LIMIT 50000";
    let execution = execute_cli_timed_root(select_sql)?;
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "USR_SEL_COLD_50K",
        TestGroup::UserTable,
        "select_cold",
        "SELECT 50,000 rows from cold storage (after flush)",
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
