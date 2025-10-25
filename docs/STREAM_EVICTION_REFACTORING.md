# Stream Table Eviction Implementation - Complete Refactoring

## Summary of Changes

This refactoring addresses all 5 requirements:

### 1. ✅ Tests moved to integration tests
- Tests are now in `/backend/tests/test_stream_tables.rs` (existing integration test file)
- Removed `stream_ttl_test.rs` from `src/tables/` module

### 2. ✅ Using existing `stream_eviction.rs` instead of custom logic
- **Removed** custom eviction logic from `lifecycle.rs` (that was manually iterating DataFusion tables)
- **Fixed** `StreamEvictionJob::run_eviction()` to properly query system.tables and run TTL eviction
- Now uses existing job infrastructure with proper tracking in system.jobs

### 3. ✅ Stream eviction follows same pattern as flush/cleanup jobs
- Created `StreamEvictionScheduler` (similar to `FlushScheduler`)
- Uses `JobExecutor` for job tracking in system.jobs
- Prevents duplicate jobs (only one eviction job runs at a time)
- Proper lifecycle management (start/stop with graceful shutdown)

### 4. ✅ Eviction interval configurable in config.toml
- Added `[stream] eviction_interval_seconds = 60` to config.toml
- Added `StreamSettings.eviction_interval_seconds` field
- Default: 60 seconds (1 minute)
- Configurable per deployment

### 5. ✅ Optimized data structure for efficient eviction
- Stream tables already use `BTreeMap<String, JsonValue>` with timestamp-prefix keys
- Key format: `"{timestamp_ms}:{row_id}"` - naturally sorted by timestamp
- `evict_older_than(cutoff)` efficiently iterates only old events (no full scan needed)
- BTreeMap provides O(log n) range queries for timestamp-based eviction

## Files Modified

### Configuration
- `backend/config.toml` - Added eviction_interval_seconds
- `backend/config.example.toml` - Added eviction_interval_seconds with documentation
- `backend/crates/kalamdb-server/src/config.rs` - Added StreamSettings.eviction_interval_seconds field

### Core Implementation
- `backend/crates/kalamdb-core/src/jobs/stream_eviction.rs` - Fixed run_eviction() to query tables and run TTL cleanup
- `backend/crates/kalamdb-core/src/jobs/stream_eviction_scheduler.rs` - **NEW** scheduler similar to FlushScheduler
- `backend/crates/kalamdb-core/src/jobs/mod.rs` - Exported StreamEvictionScheduler

### Server Lifecycle
- `backend/crates/kalamdb-server/src/lifecycle.rs`:
  - **Removed** 100+ lines of custom eviction logic
  - **Added** StreamEvictionScheduler initialization and startup
  - **Added** Graceful shutdown for eviction scheduler
  - **Added** StreamEvictionScheduler to ApplicationComponents

### Tests
- `backend/crates/kalamdb-core/src/tables/mod.rs` - Removed stream_ttl_test module
- Tests moved to integration test suite (test_stream_tables.rs)

## Architecture

### Before (Custom Logic)
```
Server Startup
  ↓
spawn tokio::spawn(async move {
    loop {
        // Iterate DataFusion catalogs/schemas/tables
        // Downcast to StreamTableProvider
        // Manually call evict_older_than()
    }
})
```
**Problems:**
- No job tracking in system.jobs
- No duplicate prevention
- Hard to maintain
- Different pattern from other jobs

### After (Scheduler Pattern)
```
Server Startup
  ↓
StreamEvictionScheduler::new(eviction_job, interval)
  ↓
scheduler.start()
  ↓
Background Task (similar to FlushScheduler):
  ├─ Runs every N seconds (configurable)
  ├─ Calls StreamEvictionJob::run_eviction()
  ├─ JobExecutor registers job in system.jobs
  ├─ Prevents duplicate eviction jobs
  └─ Logs eviction results

Shutdown:
  ↓
scheduler.stop()
  ↓
Graceful cleanup
```

**Benefits:**
- Consistent with FlushScheduler pattern
- Job tracking in system.jobs
- Duplicate prevention built-in
- Graceful shutdown support
- Config-driven interval

## Stream Eviction Flow

```rust
1. Timer tick (every eviction_interval_seconds)
   ↓
2. StreamEvictionJob::run_eviction()
   ↓
3. kalam_sql.scan_all_tables() // Query system.tables
   ↓
4. Filter for table_type == "stream"
   ↓
5. For each stream table:
   ├─ Get table definition (for ttl_seconds)
   ├─ Skip if no TTL configured
   ├─ Calculate cutoff = now_ms - (ttl * 1000)
   ├─ JobExecutor::execute_job() // Registers in system.jobs
   │   └─ stream_store.evict_older_than(cutoff)
   └─ Log eviction count
   ↓
6. Return total evicted events
```

## Data Structure Efficiency

### Stream Table Storage (In-Memory)
```rust
DashMap<String, BTreeMap<String, JsonValue>>
  ↑              ↑            ↑       ↑
  |              |            |       └─ Event data (JSON)
  |              |            └───────── Key: "{timestamp_ms}:{row_id}"
  |              └────────────────────── Events sorted by timestamp
  └───────────────────────────────────── Table key: "{namespace}:{table_name}"
```

### Eviction Performance
- **BTreeMap** maintains sorted order by key
- **Timestamp prefix** enables efficient range iteration
- **evict_older_than(cutoff)**:
  1. Iterate BTreeMap in order (already sorted by timestamp)
  2. Break early when timestamp >= cutoff (no need to scan all events)
  3. O(k + log n) where k = events to delete, n = total events
  4. Minimal CPU usage even with large buffers

Example:
```
Events: [1000:evt1, 1500:evt2, 2000:evt3, 2500:evt4, 3000:evt5]
Cutoff: 2000ms
Eviction scans: [1000:evt1, 1500:evt2] → stops at 2000:evt3
Result: 2 events deleted, 3 remaining
```

## Configuration Example

```toml
[stream]
default_ttl_seconds = 10
default_max_buffer = 10000
eviction_interval_seconds = 60  # Check every minute
```

Production tuning:
- High-frequency events: `eviction_interval_seconds = 30` (check every 30s)
- Low-frequency events: `eviction_interval_seconds = 300` (check every 5 minutes)
- Large buffers: Increase interval to reduce CPU usage
- Small buffers: Decrease interval for timely cleanup

## Testing

Integration tests verify:
1. SELECT FROM stream tables works (scan implementation)
2. SELECT with projection (column selection)
3. SELECT with LIMIT
4. TTL eviction removes old events
5. Events inserted before TTL are retained
6. Events inserted after TTL are evicted

All tests use the proper StreamTableProvider and StreamTableStore infrastructure.

## Migration Notes

**No breaking changes:**
- Existing stream tables continue to work
- Default config values maintain backward compatibility
- New eviction_interval_seconds defaults to 60 seconds (1 minute)

**Performance Impact:**
- Eviction now runs on configurable schedule (60s default) instead of per-table logic
- Single eviction job for all stream tables (more efficient)
- Job tracking adds minimal overhead (system.jobs writes)

## Future Enhancements

1. **MAX_BUFFER eviction** - Already implemented in StreamEvictionJob.evict_max_buffer()
2. **Per-table eviction intervals** - Configure different intervals for different tables
3. **Eviction metrics** - Track eviction counts, times, and performance in system.jobs
4. **Adaptive eviction** - Adjust interval based on event ingestion rate
5. **Eviction policies** - Support different eviction strategies (LRU, FIFO, priority-based)
