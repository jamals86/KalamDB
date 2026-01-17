use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn shared_table_update_50k() -> anyhow::Result<()> {
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

        let sql = format!("INSERT INTO bench_shared.items (value) VALUES {}", values.join(", "));
        execute_cli_timed_root(&sql)?;
    }
    std::thread::sleep(Duration::from_millis(200));

    // Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");

    // Execute bulk update
    let update_sql = "UPDATE bench_shared.items SET value = CONCAT('updated_', value)";
    let execution = execute_cli_timed_root(update_sql)?;

    // Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");

    // Create test result
    let mut result = TestResult::new(
        "SHR_UPD_50K",
        TestGroup::SharedTable,
        "update",
        "Update 50,000 rows in shared table",
    );

    result.set_timings(execution.cli_total_ms, execution.server_time_ms, execution.server_time_ms);
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
