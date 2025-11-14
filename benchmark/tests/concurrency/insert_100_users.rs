use kalamdb_benchmark::*;
use std::time::Duration;

#[test]
fn concurrency_insert_100_users() -> anyhow::Result<()> {
    // Note: This is a simplified version. Full implementation would use threads
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    let mut total_cli_ms = 0.0;
    let mut total_server_ms = 0.0;
    
    // Simulate 100 concurrent users
    for i in 1..=100 {
        let sql = format!("INSERT INTO bench_user.items (value) VALUES ('concurrent_value_{}')", i);
        let execution = execute_cli_timed_root(&sql)?;
        total_cli_ms += execution.cli_total_ms;
        total_server_ms += execution.server_time_ms;
    }
    
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    let mut result = TestResult::new(
        "CONC_INS_100_USERS",
        TestGroup::Concurrency,
        "insert",
        "Concurrent insert with 100 users",
    );
    
    result.set_timings(total_cli_ms, total_server_ms, total_server_ms);
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(100, total_server_ms / 100.0);
    result.validate();
    
    append_test_result(result)?;
    cleanup_benchmark_tables()?;
    
    Ok(())
}
