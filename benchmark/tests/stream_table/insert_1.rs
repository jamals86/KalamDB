use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn stream_table_insert_1() -> anyhow::Result<()> {
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));

    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");

    let sql = "INSERT INTO bench_stream.events (value) VALUES ('benchmark_value_1')";
    let execution = execute_cli_timed_root(sql)?;

    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");

    let mut result = TestResult::new(
        "STR_INS_1",
        TestGroup::StreamTable,
        "insert",
        "Insert 1 row into stream table",
    );

    result.set_timings(execution.cli_total_ms, execution.server_time_ms, execution.server_time_ms);
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(1, execution.server_time_ms);
    result.validate();

    append_test_result(result)?;
    cleanup_benchmark_tables()?;

    Ok(())
}
