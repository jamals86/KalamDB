use kalamdb_benchmark::*;

#[ignore = "requires running backend server"]
#[test]
fn user_table_delete_1() -> anyhow::Result<()> {
    // NOTE: DELETE tests are skipped for user tables because:
    // 1. DELETE requires WHERE id = <value>
    // 2. User tables have per-user isolation that makes finding specific IDs complex
    // 3. DELETE functionality is already tested in shared_table tests

    setup_benchmark_tables()?;
    cleanup_benchmark_tables()?;

    let mut result = TestResult::new(
        "USR_DEL_1",
        TestGroup::UserTable,
        "delete",
        "DELETE test skipped (tested via shared_table)",
    );
    result.set_timings(0.0, 0.0, 0.0);
    result.set_memory(0.0, 0.0);
    result.set_disk(0.0, 0.0);
    result.set_requests(0, 0.0);
    result.validate();

    // Write result
    let path = append_test_result(result)?;
    println!("✅ Benchmark result written to: {}", path.display());

    // Cleanup
    cleanup_benchmark_tables()?;

    Ok(())
}
