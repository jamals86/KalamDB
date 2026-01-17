use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn shared_table_insert_50k() -> anyhow::Result<()> {
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));

    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");

    let batch_size = 1000;
    let total_rows = 50000;
    let num_batches = total_rows / batch_size;

    let mut total_cli_ms = 0.0;
    let mut total_server_ms = 0.0;

    for batch in 0..num_batches {
        let mut values = Vec::new();
        for i in 0..batch_size {
            let idx = batch * batch_size + i;
            values.push(format!("('benchmark_value_{}')", idx));
        }

        let sql = format!("INSERT INTO bench_shared.items (value) VALUES {}", values.join(", "));
        let execution = execute_cli_timed_root(&sql)?;
        total_cli_ms += execution.cli_total_ms;
        total_server_ms += execution.server_time_ms;
        std::thread::sleep(Duration::from_millis(50));
    }

    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");

    let mut result = TestResult::new(
        "SHR_INS_50K",
        TestGroup::SharedTable,
        "insert",
        "Insert 50,000 rows into shared table (batched)",
    );

    result.set_timings(total_cli_ms, total_server_ms, total_server_ms);
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(num_batches as u64, total_server_ms / num_batches as f64);
    result.validate();

    append_test_result(result)?;
    cleanup_benchmark_tables()?;

    Ok(())
}
