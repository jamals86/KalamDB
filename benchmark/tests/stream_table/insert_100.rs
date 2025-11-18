use kalamdb_benchmark::*;
use std::time::Duration;

#[ignore = "requires running backend server"]
#[test]
fn stream_table_insert_100() -> anyhow::Result<()> {
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));

    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");

    let mut total_cli_ms = 0.0;
    let mut total_server_ms = 0.0;

    for i in 1..=100 {
        let sql = format!(
            "INSERT INTO bench_stream.events (value) VALUES ('benchmark_value_{}')",
            i
        );
        let execution = execute_cli_timed_root(&sql)?;
        total_cli_ms += execution.cli_total_ms;
        total_server_ms += execution.server_time_ms;
    }

    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");

    let mut result = TestResult::new(
        "STR_INS_100",
        TestGroup::StreamTable,
        "insert",
        "Insert 100 rows into stream table",
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
