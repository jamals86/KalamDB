# Benchmark Test Templates

This directory contains placeholder files for the remaining benchmark tests.
Each test follows the same pattern as the implemented user_table tests.

## Implementation Status

### ✅ Completed (User Table - 12 tests)
- insert_1, insert_100, insert_50k
- select_hot_1, select_hot_100, select_hot_50k  
- select_cold_1, select_cold_100, select_cold_50k
- update_1, update_100, update_50k
- delete_1, delete_100, delete_50k

### ✅ Partially Completed (Shared Table - 3 tests)
- insert_1, insert_100, insert_50k

### 📝 TODO: Remaining Tests

Copy the pattern from user_table tests and adapt for each group:

**Shared Table** (9 remaining):
- select_1, select_100, select_50k
- update_1, update_100, update_50k
- delete_1, delete_100, delete_50k

**Stream Table** (6 tests):
- insert_1, insert_100, insert_50k
- select_1, select_100, select_50k

**System Tables** (4 tests):
- select_tables
- select_jobs
- select_users
- select_schemas

**Concurrency** (3 tests):
- insert_1_user
- insert_100_users
- insert_50k_users

## Template Pattern

```rust
use kalamdb_benchmark::*;
use std::time::Duration;

#[test]
fn test_name() -> anyhow::Result<()> {
    // 1. Setup
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    // 2. Measure before
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    // 3. Execute SQL
    let sql = "YOUR SQL HERE";
    let execution = execute_cli_timed_root(sql)?;
    
    // 4. Measure after
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    // 5. Create result
    let mut result = TestResult::new(
        "TEST_ID",
        TestGroup::YourGroup,
        "subcategory",
        "Description",
    );
    
    result.set_timings(execution.cli_total_ms, execution.server_time_ms, execution.server_time_ms);
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(1, execution.server_time_ms);
    result.validate();
    
    // 6. Write result
    append_test_result("0.2.0", "012-full-dml-support", result)?;
    
    // 7. Cleanup
    cleanup_benchmark_tables()?;
    
    Ok(())
}
```

## Quick Implementation Guide

1. Copy a similar test file from user_table/
2. Change table name (bench_user.items → bench_shared.items or bench_stream.events)
3. Update test ID and description
4. Adjust SQL queries as needed
5. Add test entry in Cargo.toml if not already present
