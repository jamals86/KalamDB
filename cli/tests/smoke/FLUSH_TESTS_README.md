# Flush Operations Smoke Tests

This test suite validates flush operations for both USER and SHARED tables, ensuring that:
- Flush policies work correctly
- Manual flush commands execute successfully
- Flush jobs complete without errors
- Data is correctly retrieved from both RocksDB (unflushed) and Parquet (flushed) sources

## Test Files

- `smoke/smoke_test_flush_operations.rs` - Comprehensive flush smoke tests

## Test Coverage

### 1. `smoke_test_user_table_flush()`
Tests USER table flush with the following flow:
1. Creates table with `FLUSH ROWS 50` policy
2. Inserts 200 rows in batches
3. Manually triggers `FLUSH TABLE`
4. Verifies flush job completes successfully (using `verify_job_completed()`)
5. Queries all data and verifies 200 rows are returned
6. Validates data integrity by checking sequence numbers

**Expected Behavior:**
- All 200 rows should be retrievable (combining flushed Parquet + unflushed RocksDB)
- Flush job should complete within 30 seconds
- Data integrity maintained (no missing or duplicate rows)

### 2. `smoke_test_shared_table_flush()`
Tests SHARED table flush with identical flow to USER table test:
1. Creates SHARED table with `FLUSH ROWS 50` policy
2. Inserts 200 rows in batches
3. Manually triggers `FLUSH TABLE`
4. Verifies flush job completes successfully
5. Queries all data and verifies 200 rows are returned
6. Validates data integrity

**Expected Behavior:**
- Same as USER table test
- Validates SHARED tables flush to correct storage path

### 3. `smoke_test_mixed_source_query()`
Tests query engine's ability to merge data from multiple sources:
1. Creates table with `FLUSH ROWS 30` policy
2. Inserts 50 rows (triggers flush) → Parquet
3. Manually flushes to ensure Parquet write
4. Inserts 20 more rows → RocksDB
5. Queries all data and verifies 70 total rows
6. Queries range spanning both sources (rows 45-55)

**Expected Behavior:**
- Total count: 70 rows (50 Parquet + 20 RocksDB)
- Range queries correctly merge from both sources
- No duplicate or missing data at boundaries

## Running the Tests

### Prerequisites
1. Start the KalamDB server:
   ```bash
   cd backend
   cargo run --bin kalamdb-server
   ```

2. In a separate terminal, run the tests:
   ```bash
   cd cli
   cargo test --test smoke test_flush_operations -- --nocapture
   ```

### Run Individual Tests
```bash
# Run only USER table flush test
cargo test --test smoke smoke_test_user_table_flush -- --nocapture

# Run only SHARED table flush test
cargo test --test smoke smoke_test_shared_table_flush -- --nocapture

# Run only mixed source query test
cargo test --test smoke smoke_test_mixed_source_query -- --nocapture
```

### Run All Smoke Tests
```bash
cargo test --test smoke -- --nocapture
```

## Helper Functions Used

### Job Verification
- `parse_job_id_from_flush_output(output)` - Extracts job ID from FLUSH TABLE response
- `verify_job_completed(job_id, timeout)` - Polls `system.jobs` until job completes or fails
- `verify_jobs_completed(job_ids, timeout)` - Bulk verification for multiple jobs

### Test Setup
- `generate_unique_namespace(base)` - Creates unique namespace name with timestamp
- `generate_unique_table(base)` - Creates unique table name with timestamp
- `execute_sql_as_root_via_cli(sql)` - Executes SQL as root user (localhost bypass)

## Configuration

Test constants (in `test_flush_operations.rs`):
```rust
const JOB_TIMEOUT: Duration = Duration::from_secs(30);  // Max wait for flush job
const FLUSH_POLICY_ROWS: usize = 50;                    // Rows before auto-flush
const INSERT_ROWS: usize = 200;                         // Total rows to insert
```

## Expected Storage Layout

After successful flush, parquet files should be in:
```
./data/storage/{namespace}/{table_name}/{user_id}/{timestamp}.parquet
```

Example:
```
./data/storage/smoke_flush_1731234567890/user_flush_1731234567891/sys_root/2025-11-08T20-30-45.parquet
```

## Troubleshooting

### Test Failures

**"Server not running"**
- Start the backend server first: `cd backend && cargo run --bin kalamdb-server`

**"Timeout waiting for job to complete"**
- Check server logs for flush job errors
- Verify storage path is writable: `ls -la backend/data/storage`
- Increase `JOB_TIMEOUT` if system is slow

**"Expected 200 rows but got X"**
- Check if flush job actually completed (query `system.jobs`)
- Verify parquet files were created: `find backend/data/storage -name "*.parquet"`
- Check server logs for read errors

### Debugging Tips

1. **Enable verbose logging:**
   ```bash
   cargo test --test smoke test_flush_operations -- --nocapture --test-threads=1
   ```

2. **Check flush job status manually:**
   ```bash
   kalam -u http://localhost:8080 --username root --password "" \
     --command "SELECT * FROM system.jobs WHERE job_id = 'FL-xxxxx' ORDER BY created_at DESC LIMIT 1"
   ```

3. **Verify parquet files:**
   ```bash
   find backend/data/storage -name "*.parquet" -mmin -5  # Files created in last 5 minutes
   ```

4. **Check table data distribution:**
   ```bash
   kalam -u http://localhost:8080 --username root --password "" \
     --command "SELECT COUNT(*) FROM namespace.table"
   ```

## Success Criteria

All three tests should pass with:
- ✅ Flush jobs complete successfully (status = 'Completed')
- ✅ All inserted rows are retrievable via SELECT
- ✅ Data integrity preserved (no duplicates, no missing rows)
- ✅ Parquet files created in correct storage path
- ✅ Query engine correctly merges RocksDB + Parquet sources

## Related Documentation

- Backend Configuration: `../../backend/config.toml`
- Test Helper Functions: `../common/mod.rs`
- Flush Job Executors: `../../backend/crates/kalamdb-core/src/jobs/executors/flush.rs`
