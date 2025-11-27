use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn user_table_insert_50k() -> anyhow::Result<()> {
    // // Setup
    // setup_benchmark_tables()?;
    // std::thread::sleep(Duration::from_millis(200));

    // // Measure before
    // let mem_before = measure_memory_mb();
    // let disk_before = measure_disk_mb("backend/data/rocksdb");

    // // Execute 50k inserts in batches
    // let batch_size = 1000;
    // let total_rows = 50000;
    // let num_batches = total_rows / batch_size;

    // let mut total_cli_ms = 0.0;
    // let mut total_server_ms = 0.0;

    // for batch in 0..num_batches {
    //     let mut values = Vec::new();
    //     for i in 0..batch_size {
    //         let idx = batch * batch_size + i;
    //         values.push(format!("('benchmark_value_{}')", idx));
    //     }

    //     let sql = format!(
    //         "INSERT INTO bench_user.items (value) VALUES {}",
    //         values.join(", ")
    //     );

    //     let execution = execute_cli_timed_root(&sql)?;
    //     total_cli_ms += execution.cli_total_ms;
    //     total_server_ms += execution.server_time_ms;

    //     // Small delay between batches
    //     std::thread::sleep(Duration::from_millis(50));
    // }

    // // Measure after
    // let mem_after = measure_memory_mb();
    // let disk_after = measure_disk_mb("backend/data/rocksdb");

    // // Create test result
    // let mut result = TestResult::new(
    //     "USR_INS_50K",
    //     TestGroup::UserTable,
    //     "insert",
    //     "Insert 50,000 rows into user table (batched)",
    // );

    // result.set_timings(total_cli_ms, total_server_ms, total_server_ms);
    // result.set_memory(mem_before, mem_after);
    // result.set_disk(disk_before, disk_after);
    // result.set_requests(num_batches as u64, total_server_ms / num_batches as f64);
    // result.validate();

    // // Write result
    // let path = append_test_result(result)?;
    // println!("✅ Benchmark result written to: {}", path.display());

    // // Cleanup
    // cleanup_benchmark_tables()?;

    Ok(())
}
