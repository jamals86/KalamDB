# Stream Table TTL Eviction and SELECT Support - Implementation Summary

## Issues Fixed

### 1. SELECT FROM stream tables not working
**Problem**: The `scan()` method in `StreamTableProvider` returned `NotImplemented` error.

**Solution**: Implemented full `scan()` method that:
- Scans all events from the in-memory `StreamTableStore`
- Converts JSON events to Arrow RecordBatch
- Applies projection (SELECT specific columns)
- Applies limit (SELECT ... LIMIT N)
- Returns MemoryExec execution plan for DataFusion

**Files Modified**:
- `/backend/crates/kalamdb-core/src/tables/stream_table_provider.rs`
  - Implemented `scan()` method (lines 340-405)
  - Added `json_events_to_arrow_batch()` helper function (lines 675-820)

### 2. TTL eviction not happening automatically
**Problem**: Stream tables had TTL parsing but no automatic background eviction task.

**Solution**: Added background eviction task in server lifecycle that:
- Runs every 60 seconds (configurable via `StreamEvictionConfig`)
- Iterates through all registered DataFusion tables
- Checks if table is a `StreamTableProvider` with TTL configured
- Calculates cutoff timestamp and calls `evict_older_than()`
- Logs eviction results

**Files Modified**:
- `/backend/crates/kalamdb-server/src/lifecycle.rs`
  - Added `StreamEvictionJob` and `JobExecutor` imports
  - Created background eviction task (lines 204-288)
  - Spawned tokio task for periodic eviction

### 3. Test coverage for TTL and SELECT
**Problem**: No integration tests verifying TTL eviction and SELECT functionality.

**Solution**: Added comprehensive test suite:
- `test_stream_table_ttl_eviction_with_select()` - Verifies TTL eviction removes events
- `test_stream_table_select_with_projection()` - Tests SELECT with specific columns
- `test_stream_table_select_with_limit()` - Tests SELECT with LIMIT clause

**Files Created**:
- `/backend/crates/kalamdb-core/src/tables/stream_ttl_test.rs`
- `/backend/tests/test_stream_ttl.sql` (manual test SQL)

## Technical Details

### Stream Table Scan Implementation

The `scan()` method follows this flow:
1. Call `store.scan(namespace_id, table_name)` → `Vec<(i64, String, JsonValue)>`
2. Apply limit if specified
3. Convert JSON tuples to Arrow RecordBatch using schema
4. Apply projection if specified (column selection)
5. Create MemoryExec with appropriate schema
6. Return Arc<dyn ExecutionPlan>

### TTL Eviction Architecture

```
Server Startup
  ↓
Create StreamEvictionJob (60s interval)
  ↓
Spawn tokio background task
  ↓
Every 60 seconds:
  ├─ Iterate all DataFusion schemas/tables
  ├─ Check if table is StreamTableProvider
  ├─ Check if TTL configured (retention_seconds)
  ├─ Calculate cutoff = now_ms - (ttl_seconds * 1000)
  ├─ Call provider.evict_older_than(cutoff_ms)
  └─ Log results
```

### JSON to Arrow Conversion

The `json_events_to_arrow_batch()` function handles:
- `DataType::Utf8` → StringBuilder
- `DataType::Int32` → Int32Builder
- `DataType::Int64` → Int64Builder
- `DataType::Float64` → Float64Builder
- `DataType::Boolean` → BooleanBuilder
- `DataType::Timestamp(_,_)` → TimestampMillisecondBuilder (with ISO 8601 parsing)

## Test Results

```
running 3 tests
test tables::stream_ttl_test::tests::test_stream_table_select_with_projection ... ok
test tables::stream_ttl_test::tests::test_stream_table_select_with_limit ... ok
test tables::stream_ttl_test::tests::test_stream_table_ttl_eviction_with_select ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
```

## Usage Example

```sql
-- Create stream table with 60-second TTL
CREATE STREAM TABLE events (
    event_id TEXT,
    event_type TEXT,
    timestamp BIGINT
) TTL 60;

-- Insert events
INSERT INTO events VALUES ('evt1', 'click', 1000);
INSERT INTO events VALUES ('evt2', 'view', 2000);

-- Query events (now works!)
SELECT * FROM events;

-- Select specific columns
SELECT event_id, event_type FROM events;

-- Limit results
SELECT * FROM events LIMIT 10;

-- After 60+ seconds, events are automatically evicted
-- Background task runs every 60 seconds
```

## Configuration

The eviction interval can be configured via `StreamEvictionConfig`:

```rust
let config = StreamEvictionConfig {
    eviction_interval_secs: 60,  // Run every 60 seconds
    node_id: "node-1".to_string(),
};
```

Default: 60 seconds (defined in `StreamEvictionConfig::default()`)

## Remaining Work (Future Enhancements)

1. **Filter pushdown**: Current scan() doesn't push filters to storage layer
2. **Sorting**: Add ORDER BY support for stream tables
3. **Aggregations**: Enable GROUP BY, COUNT, AVG, etc.
4. **MAX_BUFFER eviction**: Already exists in StreamEvictionJob but not wired to server
5. **Eviction metrics**: Expose eviction counts to system.jobs or monitoring

## Files Summary

**Modified**:
- `backend/crates/kalamdb-core/src/tables/stream_table_provider.rs` (+~150 lines)
- `backend/crates/kalamdb-server/src/lifecycle.rs` (+~90 lines)
- `backend/crates/kalamdb-core/src/tables/mod.rs` (+3 lines)

**Created**:
- `backend/crates/kalamdb-core/src/tables/stream_ttl_test.rs` (+242 lines)
- `backend/tests/test_stream_ttl.sql` (+17 lines)

**Total**: ~500 lines of code added (implementation + tests)
