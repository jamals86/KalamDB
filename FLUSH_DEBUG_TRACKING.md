# Flush Debug Tracking Enhancement

## Problem

The flush mechanism was not providing adequate visibility into:
1. Whether flush jobs are actually executing
2. How many rows are being flushed to Parquet files
3. The start and end of each flush operation
4. Per-user flush progress for user tables
5. The actual execution path (scheduler was just simulating flushes)

## Solution

Added comprehensive debug and info logging throughout the flush pipeline with visual indicators:

### 1. **Scheduler Level** (`kalamdb-core/src/scheduler.rs`)

**Added logging for:**
- 🚀 Flush job start trigger
- 📊 Job execution begin
- ⚠️  Warning that flush logic is not yet wired to scheduler
- ✅ Job completion (currently shows 0 rows as not wired yet)
- ❌ Error logging for trigger state failures

**Key insight:** The scheduler's `check_and_trigger_flushes` method has a TODO where it should execute the actual flush logic. Currently it only:
1. Sleeps for 100ms
2. Resets trigger state
3. Removes from active flushes

**Next step needed:** Wire the scheduler to actually call `UserTableFlushJob::execute()` or `SharedTableFlushJob::execute()`.

### 2. **User Table Flush** (`kalamdb-core/src/flush/user_table_flush.rs`)

**Added logging for:**
- 🚀 Flush job started with job_id and timestamp
- 🔄 Flush execution starting
- 📸 RocksDB snapshot creation
- 🔍 Row scanning progress (every 1000 rows)
- 💾 Per-user flush operations with row counts
- 📝 Parquet file write operations with paths
- ✅ Successful flush completion with metrics
- 🗑️  Row deletion from RocksDB after flush
- ⚠️  Warnings for empty flushes or no rows
- ❌ Error logging for failures at each stage

**Metrics tracked:**
- `total_rows_scanned`: Total rows examined from RocksDB
- `total_rows_flushed`: Actual rows written to Parquet
- `users_count`: Number of distinct users processed
- `parquet_files`: List of files written
- `duration_ms`: Time taken for flush

### 3. **Shared Table Flush** (`kalamdb-core/src/flush/shared_table_flush.rs`)

**Added logging for:**
- 🚀 Shared table flush job started
- 🔄 Flush execution starting
- 📊 Row scan results
- 💾 Flushing row count
- 📝 Parquet file write with path
- ✅ Flush completion with metrics
- 🗑️  Row deletion tracking
- ⚠️  Empty table warnings
- ❌ Error logging with context

**Metrics tracked:**
- `rows_flushed`: Number of rows written
- `parquet_file`: Output file path
- `duration_ms`: Execution time

## Visual Indicators

- 🚀 = Start of operation
- 📊 = Data metrics/status
- 📸 = Snapshot creation
- 🔍 = Scanning/searching
- 💾 = Data persistence operation
- 📝 = File write operation
- ✅ = Success/completion
- 🗑️  = Deletion operation
- ⚠️  = Warning (non-fatal)
- ❌ = Error/failure
- 🔄 = Process starting/cycling

## Log Levels

- **INFO**: Major lifecycle events (job start/complete, flush results)
- **DEBUG**: Detailed step-by-step execution (scanning, writing, deleting)
- **WARN**: Empty flushes, missing configuration, TODO items
- **ERROR**: Failures at any stage with full context

## Example Log Output

```
[INFO ] 🚀 Flush job started: job_id=flush-messages-1729698808456-uuid, table=prod.messages, timestamp=2025-10-23T13:53:28Z
[DEBUG] 🔄 Starting flush execution: table=prod.messages, snapshot creation...
[DEBUG] 📸 RocksDB snapshot created for table=prod.messages
[DEBUG] 🔍 Scanning rows for table=prod.messages...
[DEBUG] 📊 Scanned 1000 rows so far (table=prod.messages)
[DEBUG] 💾 Flushing 150 rows for user_id=user123 (table=prod.messages)
[DEBUG] 📝 Writing Parquet file: path=data/prod/messages/user123/2025-10-23T13-53-28.parquet, rows=150, user_id=user123
[INFO ] ✅ Flushed 150 rows for user_id=user123 to data/prod/messages/user123/2025-10-23T13-53-28.parquet (table=prod.messages)
[DEBUG] 🗑️  Deleting 150 flushed keys from RocksDB (table=prod.messages)
[DEBUG] ✅ Deleted 150 flushed rows from RocksDB (table=prod.messages)
[INFO ] ✅ Flush execution completed: table=prod.messages, total_rows_scanned=1500, total_rows_flushed=1450, users_count=10, parquet_files=10
[INFO ] ✅ Flush job completed: job_id=flush-messages-1729698808456-uuid, table=prod.messages, rows_flushed=1450, users_count=10, parquet_files=10, duration_ms=245
```

## How to Enable Debug Logging

Set the `RUST_LOG` environment variable:

```bash
# Windows PowerShell
$env:RUST_LOG="debug"

# Windows CMD
set RUST_LOG=debug

# Linux/macOS
export RUST_LOG=debug

# Or for specific modules only
$env:RUST_LOG="kalamdb_core::flush=debug,kalamdb_core::scheduler=debug"
```

## Testing the Changes

To verify flush tracking:

```bash
cd backend
cargo test flush
```

To run the server with debug logging:

```bash
cd backend
$env:RUST_LOG="debug"
cargo run --bin kalamdb-server
```

## Known Issues

1. **Scheduler not wired to actual flush execution**: The scheduler triggers jobs but doesn't call the flush job execute methods. See TODO in `scheduler.rs:692`.

2. **Manual flush may not work**: If manual flush commands are not triggering actual flushes, they likely have the same issue - they create job records but don't execute the flush logic.

## Next Steps to Fix Flush Execution

### Required: Wire scheduler to flush execution

The scheduler needs to be modified to:

1. Access the table provider (UserTableProvider or SharedTableProvider)
2. Create the appropriate flush job (UserTableFlushJob or SharedTableFlushJob)
3. Call the job's `execute()` method
4. Handle the FlushJobResult

Example fix needed in `scheduler.rs`:

```rust
// Instead of TODO, need something like:
let flush_job = UserTableFlushJob::new(
    store,
    namespace_id,
    table_name,
    schema,
    storage_location,
)
.with_jobs_provider(jobs_provider.clone());

let result = flush_job.execute()?;
// Handle result...
```

This requires passing additional dependencies to the scheduler:
- Arc<UserTableStore> or Arc<SharedTableStore>
- Storage registry
- Table metadata (namespace, schema)

## Files Modified

1. `backend/crates/kalamdb-core/src/scheduler.rs` - Scheduler logging
2. `backend/crates/kalamdb-core/src/flush/user_table_flush.rs` - User table flush logging
3. `backend/crates/kalamdb-core/src/flush/shared_table_flush.rs` - Shared table flush logging

## Conclusion

With these changes, you can now:
- See exactly when flush jobs start and complete
- Track how many rows are being flushed
- Identify if flushes are failing and why
- Understand the streaming per-user flush process
- Debug why flushes might not be working (currently: scheduler not wired)

The logging will help diagnose the core issue: **the scheduler is not actually executing flush operations**, it's just simulating them.
