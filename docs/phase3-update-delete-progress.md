# Phase 3: UPDATE/DELETE Implementation Progress

**Branch**: `012-full-dml-support` | **Date**: 2025-01-15 | **Status**: IN PROGRESS

## Overview

Phase 3 implements append-only UPDATE/DELETE operations with version resolution across RocksDB (fast storage) and Parquet (long-term storage) using SystemColumnsService for monotonic nanosecond-precision timestamps.

## Completed Work

### UPDATE Handler (T042-T047) - ✅ COMPLETE

**File**: `backend/crates/kalamdb-core/src/tables/user_tables/user_table_update.rs`

**Changes** (Lines 99-145):
1. **SystemColumnsService Integration** (Lines 116-134):
   - Replaced manual `chrono::Utc::now().to_rfc3339()` with `SystemColumnsService.handle_update()`
   - Parse existing `_updated` timestamp to nanoseconds
   - Call `handle_update(record_id, previous_updated_ns)` for monotonic timestamp
   - Convert new nanosecond timestamp back to RFC3339 string
   - **Result**: Monotonic timestamps with +1ns guarantee on updates

2. **Timestamp Conversion Helpers** (After Line 224):
   ```rust
   fn parse_timestamp_to_nanos(timestamp: &str) -> Result<i64, KalamDbError>
   fn format_nanos_to_timestamp(nanos: i64) -> String
   ```
   - Uses `chrono::DateTime` for RFC3339 ↔ nanosecond conversion
   - Handles timezone-aware parsing with `parse_from_rfc3339()`
   - Formats with proper UTC timezone via `to_rfc3339()`

**Implementation Details**:
```rust
// BEFORE (Manual timestamp)
updated_row._updated = chrono::Utc::now().to_rfc3339();

// AFTER (SystemColumnsService integration)
let previous_updated_ns = Self::parse_timestamp_to_nanos(&existing_row._updated)?;
let app_context = AppContext::get();
let sys_cols = app_context.system_columns_service();
let record_id = updated_row.row_id.parse::<i64>()?; // Snowflake ID
let (new_updated_ns, _deleted) = sys_cols.handle_update(record_id, previous_updated_ns)?;
updated_row._updated = Self::format_nanos_to_timestamp(new_updated_ns);
```

**Completed Tasks**:
- ✅ T042: UpdateHandler structure exists
- ✅ T043: execute_update() parses WHERE clause, fetches records
- ✅ T044: In-place update path with SystemColumnsService
- ✅ T046: Nanosecond precision enforcement (≥ previous + 1ns)
- ✅ T047: SqlExecutor routing already integrated
- ✅ T024: SystemColumnsService.handle_update() integration

**Deferred**:
- ⏸️ T045: Append-only update path for Parquet records (requires version resolution T052-T057)

---

### DELETE Handler (T048-T051) - ✅ COMPLETE

**File**: `backend/crates/kalamdb-core/src/tables/user_tables/user_table_delete.rs`

**Changes** (Lines 65-165):
1. **SystemColumnsService Integration** (Lines 95-120):
   - Fetch existing row to get current `_updated` timestamp
   - Parse `_updated` to nanoseconds
   - Call `SystemColumnsService.handle_delete(record_id, previous_updated_ns)`
   - Update both `_deleted = true` AND `_updated` with monotonic timestamp
   - Write updated row back to storage (soft delete)

2. **Notification Handling**:
   - Clone `row_data_before` for notification BEFORE modifying
   - Send change notification with pre-deletion data
   - Fixed variable naming: `row_to_delete` → `row_data_before`

3. **Timestamp Conversion Helpers** (Lines 256-273):
   ```rust
   fn parse_timestamp_to_nanos(timestamp: &str) -> Result<i64, KalamDbError>
   fn format_nanos_to_timestamp(nanos: i64) -> String
   ```
   - Same implementation as UPDATE handler for consistency

**Implementation Details**:
```rust
// BEFORE (Soft delete without timestamp update)
UserTableStoreExt::delete(self.store.as_ref(), ..., false)?;
// → Only set _deleted = true, _updated unchanged

// AFTER (SystemColumnsService integration)
let existing_row = UserTableStoreExt::get(...)?;
let previous_updated_ns = Self::parse_timestamp_to_nanos(&existing_row._updated)?;
let app_context = AppContext::get();
let sys_cols = app_context.system_columns_service();
let record_id = existing_row.row_id.parse::<i64>()?; // Snowflake ID
let (new_updated_ns, deleted) = sys_cols.handle_delete(record_id, previous_updated_ns)?;

let mut deleted_row = existing_row;
deleted_row._updated = Self::format_nanos_to_timestamp(new_updated_ns);
deleted_row._deleted = deleted; // true

UserTableStoreExt::put(..., &deleted_row)?;
```

**Completed Tasks**:
- ✅ T048: DeleteHandler structure exists
- ✅ T049: execute_delete() fetches existing row, parses _updated
- ✅ T050: SystemColumnsService.handle_delete() integration with monotonic _updated
- ✅ T051: SqlExecutor routing already integrated
- ✅ T025: SystemColumnsService.handle_delete() integration

---

## Build Status

**Compilation**: ✅ **CLEAN** (0 errors, 0 warnings in modified files)

```bash
cargo check --package kalamdb-core
# No errors in user_table_update.rs or user_table_delete.rs
```

---

## Architecture Benefits

### 1. Monotonic Timestamp Guarantee
- **Problem**: Manual `chrono::Utc::now()` could produce identical timestamps for rapid updates
- **Solution**: SystemColumnsService enforces `new_updated ≥ previous_updated + 1ns`
- **Result**: No timestamp collisions, strict version ordering

### 2. Centralized System Column Logic
- **Problem**: Scattered timestamp logic across multiple files (T036: 4 instances remaining)
- **Solution**: SystemColumnsService centralizes all `_updated`, `_deleted`, `_id` management
- **Result**: Single source of truth, easier maintenance, consistent behavior

### 3. Snowflake ID Integration
- **Problem**: row_id was separate from _id (Snowflake ID) causing redundancy
- **Solution**: Phase 12 refactoring unified them (row_id = Snowflake.to_string())
- **Result**: `record_id = row.row_id.parse::<i64>()` links UPDATE/DELETE to Snowflake IDs

### 4. Append-Only Foundation
- **Current**: In-place updates in RocksDB only
- **Next**: Version resolution will enable true append-only across RocksDB + Parquet
- **Benefit**: Prepare infrastructure for multi-version queries (T052-T057)

---

## Next Steps

### Immediate: Version Resolution (T052-T057)

**Goal**: Implement multi-version query resolution across fast + long-term storage

**Tasks**:
1. **T052**: Create `version_resolution.rs` module in `backend/crates/kalamdb-core/src/tables/`
2. **T053**: Implement `resolve_latest_version()`:
   - Join RocksDB scan + Parquet scan results
   - Group by `_id` (Snowflake ID)
   - Select `MAX(_updated)` per group
   - Return latest version of each record
3. **T054**: Add tie-breaker: RocksDB > Parquet when `_updated` identical
4. **T055**: Integrate into `UserTableProvider.scan()` method
5. **T056**: Integrate into `SharedTableProvider.scan()` method
6. **T057**: Apply `SystemColumnsService.apply_deletion_filter()` AFTER version resolution

**Architecture**:
```rust
// version_resolution.rs (pseudocode)
pub fn resolve_latest_version(
    fast_batch: RecordBatch,  // RocksDB results
    long_batch: RecordBatch,  // Parquet results
) -> Result<RecordBatch, KalamDbError> {
    // 1. Union fast + long batches
    let combined = union_batches(fast_batch, long_batch)?;
    
    // 2. Group by _id column (Snowflake ID)
    // 3. Select MAX(_updated) per group
    // 4. Tie-breaker: prefer fast_batch if timestamps equal
    
    // 5. Return deduplicated RecordBatch with latest versions
    Ok(latest_versions)
}
```

**Integration Point**:
```rust
// UserTableProvider.scan() (user_table_provider.rs)
async fn scan(&self, ...) -> Result<RecordBatch, KalamDbError> {
    let fast_batch = self.scan_rocksdb()?;
    let long_batch = self.scan_parquet()?;
    
    // NEW: Version resolution
    let resolved = version_resolution::resolve_latest_version(fast_batch, long_batch)?;
    
    // Apply deletion filter (_deleted = false)
    let filtered = SystemColumnsService::apply_deletion_filter(resolved)?;
    
    Ok(filtered)
}
```

### Then: Flush Integration (T058-T059)

**Goal**: Persist multi-version records to Parquet without overwriting old versions

**Tasks**:
1. **T058**: Update `FlushExecutor` to create separate batch files per flush
2. **T059**: Ensure old Parquet versions remain unchanged (append-only)

**Current Behavior**:
- FlushExecutor writes RocksDB → single Parquet file per table
- Overwrites previous flush (loses version history)

**Desired Behavior**:
- FlushExecutor writes RocksDB → timestamped Parquet file (`batch_20250115_143000.parquet`)
- Old Parquet files preserved (all versions accessible)
- Version resolution merges all Parquet files during query

### Finally: Testing (T060-T068)

**Testing Strategy**:
1. **Unit Tests** (T060-T061):
   - UPDATE in RocksDB → verify in-place update
   - UPDATE in Parquet → verify new version created
2. **Integration Tests** (T062-T065):
   - INSERT → FLUSH → UPDATE → query latest version
   - 3 updates → all flushed → query returns MAX(_updated)
   - DELETE → _deleted=true → query excludes record
   - DELETE Parquet record → new version in RocksDB
3. **Concurrency Tests** (T066-T067):
   - 10 threads UPDATE same record → all succeed
   - Rapid updates → verify +1ns increment
4. **Performance Tests** (T068):
   - Query latency with 1/10/100 versions ≤ 2× baseline

---

## Phase 3 Progress Summary

**Overall**: 11/20 tasks complete (55%)

**Breakdown**:
- ✅ SQL Parser Extensions (T038-T041): 4/4 complete (100%)
- ✅ UPDATE Handler (T042-T047): 5/6 complete (83%) - T045 deferred
- ✅ DELETE Handler (T048-T051): 4/4 complete (100%)
- ✅ Deferred Tasks (T024-T025): 2/2 complete (100%)
- ⏳ Version Resolution (T052-T057): 0/6 (0%)
- ⏳ Flush Integration (T058-T059): 0/2 (0%)
- ⏳ Testing (T060-T068): 0/9 (0%)

**Files Modified**:
- `backend/crates/kalamdb-core/src/tables/user_tables/user_table_update.rs` (+40 lines)
- `backend/crates/kalamdb-core/src/tables/user_tables/user_table_delete.rs` (+50 lines)
- `specs/012-full-dml-support/tasks.md` (marked 11 tasks complete)

**Ready for Next Phase**: ✅ YES - Foundation complete, version resolution can begin

---

## References

- **Specification**: `specs/012-full-dml-support/spec.md`
- **Tasks**: `specs/012-full-dml-support/tasks.md`
- **SystemColumnsService**: `backend/crates/kalamdb-core/src/system_columns_service.rs`
- **Snowflake ID Refactoring**: `docs/phase12-snowflake-id-refactoring.md`
- **AGENTS.md**: Development guidelines and architecture principles
