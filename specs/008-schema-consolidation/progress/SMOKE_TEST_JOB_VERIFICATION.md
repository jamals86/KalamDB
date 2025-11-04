# Smoke Test Job Verification Enhancement

## Summary

Enhanced smoke tests to verify that FLUSH operations not only start jobs but also complete successfully. Previously, tests only verified that the jobs table was not empty after a FLUSH command. Now tests parse specific job IDs and poll system.jobs until completion or timeout.

## Changes Made

### 1. New Helper Functions (`cli/tests/common/mod.rs`)

#### `parse_job_id_from_flush_output(output: &str) -> Result<String, ...>`
Parses a single job ID from `FLUSH TABLE` output.

**Expected input format:**
```
Flush started for table 'namespace.table'. Job ID: flush-table-123-uuid
```

**Returns:** The job ID string (e.g., `"flush-table-123-uuid"`)

#### `parse_job_ids_from_flush_all_output(output: &str) -> Result<Vec<String>, ...>`
Parses multiple job IDs from `FLUSH ALL TABLES` output.

**Expected input format:**
```
Flush started for 3 table(s) in namespace 'ns'. Job IDs: [flush-t1-123-uuid, flush-t2-456-uuid, flush-t3-789-uuid]
```

**Returns:** Vector of job ID strings

#### `verify_job_completed(job_id: &str, timeout: Duration) -> Result<(), ...>`
Polls `system.jobs` until the specified job reaches 'completed' status.

**Behavior:**
- Polls every 100ms
- Checks job status via: `SELECT job_id, status, error_message FROM system.jobs WHERE job_id = '<id>'`
- Returns `Ok(())` if job completes successfully
- Returns `Err(...)` if job fails or timeout occurs

**Default timeout:** 10 seconds

#### `verify_jobs_completed(job_ids: &[String], timeout: Duration) -> Result<(), ...>`
Convenience wrapper to verify multiple jobs sequentially.

### 2. Updated Smoke Tests

#### `smoke_test_user_table_subscription.rs`
**Before:**
```rust
let flush_sql = format!("FLUSH TABLE {}", full);
execute_sql_as_root_via_cli(&flush_sql).expect("flush should succeed");
let jobs = execute_sql_as_root_via_cli("SELECT * FROM system.jobs LIMIT 1");
assert!(!jobs.trim().is_empty(), "jobs table should not be empty");
```

**After:**
```rust
let flush_sql = format!("FLUSH TABLE {}", full);
let flush_output = execute_sql_as_root_via_cli(&flush_sql)
    .expect("flush should succeed for user table");

println!("[FLUSH] Output: {}", flush_output);

// Parse job ID from flush output
let job_id = parse_job_id_from_flush_output(&flush_output)
    .expect("should parse job_id from FLUSH output");

println!("[FLUSH] Job ID: {}", job_id);

// Verify the job completes successfully (10 second timeout)
verify_job_completed(&job_id, std::time::Duration::from_secs(10))
    .expect("flush job should complete successfully");

println!("[FLUSH] Job {} completed successfully", job_id);
```

#### `smoke_test_system_and_users.rs`
**Before:**
```rust
let _ = execute_sql_as_root_via_cli(&format!("FLUSH ALL TABLES IN {}", test_ns))
    .expect("flush all tables in namespace should succeed");
let jobs = execute_sql_as_root_via_cli("SELECT * FROM system.jobs LIMIT 1");
assert!(!jobs.trim().is_empty(), "jobs table should not be empty");
```

**After:**
```rust
// Create a user table to flush
let test_table = format!("{}.test_flush_table", test_ns);
let create_table_sql = format!(
    "CREATE USER TABLE {} (id INT, value VARCHAR) FLUSH ROWS 100",
    test_table
);
execute_sql_as_root_via_cli(&create_table_sql)
    .expect("create test table should succeed");

// Insert some data
let insert_sql = format!("INSERT INTO {} (id, value) VALUES (1, 'test')", test_table);
execute_sql_as_root_via_cli(&insert_sql)
    .expect("insert should succeed");

// Now flush all tables in the namespace
let flush_output = execute_sql_as_root_via_cli(&format!("FLUSH ALL TABLES IN {}", test_ns))
    .expect("flush all tables in namespace should succeed");

println!("[FLUSH ALL] Output: {}", flush_output);

// Parse job IDs from flush all output
let job_ids = parse_job_ids_from_flush_all_output(&flush_output)
    .expect("should parse job IDs from FLUSH ALL output");

println!("[FLUSH ALL] Job IDs: {:?}", job_ids);
assert!(!job_ids.is_empty(), "should have at least one job ID");

// Verify all jobs complete successfully (10 second timeout each)
verify_jobs_completed(&job_ids, std::time::Duration::from_secs(10))
    .expect("all flush jobs should complete successfully");

println!("[FLUSH ALL] All jobs completed successfully");
```

## Benefits

1. **Stronger Guarantees**: Tests now verify that FLUSH operations actually complete, not just that they start
2. **Job Tracking**: Tests explicitly track individual job IDs, making debugging easier
3. **Error Detection**: Failed jobs are detected immediately with error messages
4. **Timeout Protection**: Tests won't hang indefinitely if a job gets stuck
5. **Better Debugging**: Added debug prints showing job IDs and completion status

## Testing Pattern

The pattern can be applied to any test that uses FLUSH commands:

```rust
// 1. Execute FLUSH and capture output
let flush_output = execute_sql_as_root_via_cli("FLUSH TABLE my.table")?;

// 2. Parse job ID
let job_id = parse_job_id_from_flush_output(&flush_output)?;

// 3. Verify completion
verify_job_completed(&job_id, Duration::from_secs(10))?;
```

For `FLUSH ALL TABLES`:

```rust
// 1. Execute FLUSH ALL and capture output
let flush_output = execute_sql_as_root_via_cli("FLUSH ALL TABLES IN namespace")?;

// 2. Parse job IDs
let job_ids = parse_job_ids_from_flush_all_output(&flush_output)?;

// 3. Verify all jobs complete
verify_jobs_completed(&job_ids, Duration::from_secs(10))?;
```

## Files Modified

- `cli/tests/common/mod.rs` - Added 4 new helper functions (~100 lines)
- `cli/tests/smoke/smoke_test_user_table_subscription.rs` - Enhanced FLUSH verification (~10 lines added)
- `cli/tests/smoke/smoke_test_system_and_users.rs` - Enhanced FLUSH ALL verification (~30 lines added)

## Build Status

✅ CLI tests compile successfully
```
Compiling kalam-cli v0.1.0 (/Users/jamal/git/KalamDB/cli)
Finished `test` profile [unoptimized + debuginfo] target(s) in 1.11s
```

## Next Steps

To run the smoke tests with these enhancements:

```bash
# Start the KalamDB server
cargo run --bin kalamdb-server

# In another terminal, run smoke tests
cd cli
cargo test --test smoke -- --nocapture
```

The `--nocapture` flag will show the debug output including job IDs and completion messages.
