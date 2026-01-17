use kalamdb_benchmark::*;

#[ignore = "requires running backend server"]
#[test]
fn system_tables_select_schemas() -> anyhow::Result<()> {
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");

    let sql = "SELECT * FROM system.schemas";
    let execution = execute_cli_timed_root(sql)?;

    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");

    let mut result = TestResult::new(
        "SYS_SCHEMAS_SEL",
        TestGroup::SystemTables,
        "select",
        "SELECT * FROM system.schemas",
    );

    result.set_timings(execution.cli_total_ms, execution.server_time_ms, execution.server_time_ms);
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(1, execution.server_time_ms);
    result.validate();

    append_test_result(result)?;

    Ok(())
}
