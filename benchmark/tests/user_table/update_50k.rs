use kalamdb_benchmark::*;
use std::time::Duration;

#[test]
fn user_table_update_50k() -> anyhow::Result<()> {
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
    
    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // Execute batch updates
    let mut total_cli_ms = 0.0;
    let mut total_server_ms = 0.0;
    
    for batch in 0..num_batches {
        for i in 0..batch_size {
            let idx = batch * batch_size + i;
            let update_sql = format!(
                "UPDATE bench_user.items SET value = 'updated_value_{}' WHERE value = 'benchmark_value_{}'",
                idx, idx
            );
            let execution = execute_cli_timed_root(&update_sql)?;
            total_cli_ms += execution.cli_total_ms;
            total_server_ms += execution.server_time_ms;
        }
        
        // Small delay between batches
        std::thread::sleep(Duration::from_millis(50));
    }
    
    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // Create test result
    let mut result = TestResult::new(
        "USR_UPD_50K",
        TestGroup::UserTable,
        "update",
        "Update 50,000 rows in user table (batched)",
    );
    
    result.set_timings(
        total_cli_ms,
        total_server_ms,
        total_server_ms,
    );
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(total_rows as u64, total_server_ms / total_rows as f64);
    result.validate();
    
    // Write result
    let path = append_test_result(result)?;
    println!("✅ Benchmark result written to: {}", path.display());
    
    // Cleanup
    cleanup_benchmark_tables()?;
    
    Ok(())
}
