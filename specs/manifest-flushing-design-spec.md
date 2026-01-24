# Manifest and Flushing Design Specification

## Overview

This document provides a comprehensive analysis of the current manifest and flushing implementation in KalamDB, compares it against the desired spec, and outlines the gaps and tasks to align them.

---

## 1. Current Implementation Summary

### 1.1 Data Flow Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           WRITE PATH                                     │
├─────────────────────────────────────────────────────────────────────────┤
│  User DML (INSERT/UPDATE/DELETE)                                         │
│         ↓                                                                │
│  InsertHandler / UpdateHandler / DeleteHandler                          │
│         ↓ (via Raft)                                                    │
│  UserTableProvider / SharedTableProvider                                 │
│         ↓                                                                │
│  ┌────────────────────────┐                                             │
│  │  RocksDB (Hot Storage) │ ← Rows stored with _seq, _deleted           │
│  │  - Per-table partition │                                             │
│  │  - PK Index (secondary)│                                             │
│  └────────────────────────┘                                             │
│         ↓ (After each batch)                                            │
│  ManifestService.mark_pending_write() ✅ IMPLEMENTED                    │
│         ↓                                                                │
│  ManifestCacheEntry.sync_state = PendingWrite                           │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                           FLUSH PATH                                     │
├─────────────────────────────────────────────────────────────────────────┤
│  FLUSH TABLE command OR Job Scheduler                                    │
│         ↓                                                                │
│  FlushTableHandler → JobsManager.create_job_typed(JobType::Flush)       │
│         ↓                                                                │
│  FlushExecutor.do_flush()                                               │
│         ↓                                                                │
│  UserTableFlushJob / SharedTableFlushJob                                │
│         ↓                                                                │
│  1. Scan RocksDB (hot storage)                                          │
│  2. Version resolution (MAX(_seq) per PK)                               │
│  3. Filter tombstones (_deleted=true)                                   │
│  4. Write Parquet (temp → rename atomic)                                │
│  5. Update Manifest (add SegmentMetadata)                               │
│  6. Delete flushed rows from RocksDB                                    │
│         ↓                                                                │
│  ManifestCacheEntry.sync_state = InSync                                 │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                           READ PATH                                      │
├─────────────────────────────────────────────────────────────────────────┤
│  SELECT query                                                            │
│         ↓                                                                │
│  DataFusion query planner                                               │
│         ↓                                                                │
│  UserTableProvider.scan() / SharedTableProvider.scan()                  │
│         ↓                                                                │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  Read from BOTH:                                                  │   │
│  │  1. Hot Storage (RocksDB) - current unflushed data              │   │
│  │  2. Cold Storage (Parquet via Manifest) - flushed segments      │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│         ↓                                                                │
│  Merge & Deduplicate (MAX(_seq) per PK, exclude _deleted=true)          │
│         ↓                                                                │
│  Return results                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Key Components

| Component | Location | Purpose |
|-----------|----------|---------|
| `ManifestService` | `kalamdb-core/src/manifest/service.rs` | Hot cache (RocksDB) + Cold storage (manifest.json) coordination |
| `ManifestCacheEntry` | `kalamdb-commons/src/models/types/manifest.rs` | Cache entry with Manifest + SyncState |
| `SyncState` | `kalamdb-commons/src/models/types/manifest.rs` | InSync, PendingWrite, Syncing, Stale, Error |
| `Manifest` | `kalamdb-commons/src/models/types/manifest.rs` | Table manifest with segments list |
| `SegmentMetadata` | `kalamdb-commons/src/models/types/manifest.rs` | Parquet file metadata (min/max _seq, column stats) |
| `FlushManifestHelper` | `kalamdb-core/src/manifest/flush_helper.rs` | Helpers for updating manifest during flush |
| `UserTableFlushJob` | `kalamdb-core/src/manifest/flush/users.rs` | Flush implementation for user tables |
| `SharedTableFlushJob` | `kalamdb-core/src/manifest/flush/shared.rs` | Flush implementation for shared tables |
| `PkExistenceChecker` | `kalamdb-tables/src/utils/pk/existence_checker.rs` | PK uniqueness validation with manifest pruning |
| `UserTablePkIndex` | `kalamdb-tables/src/user_tables/pk_index.rs` | Secondary index for PK lookups in hot storage |

---

## 2. Desired Spec vs Current Implementation

### 2.1 INSERT Flow

| Step | Desired Spec | Current Implementation | Status |
|------|--------------|------------------------|--------|
| 1 | User performs insert | `InsertHandler` → `execute_insert_via_raft()` | ✅ IMPLEMENTED |
| 2a | Check if PK specified | Auto-detect from table schema | ✅ IMPLEMENTED |
| 2b | If PK specified, check existence | PK index check in `insert_batch()` | ✅ IMPLEMENTED (Hot only) |
| 2c | If exists → error | `AlreadyExists` error returned | ✅ IMPLEMENTED |
| 2d | If not specified → auto-generate | `AUTO_INCREMENT`, `SNOWFLAKE_ID` defaults | ✅ IMPLEMENTED |
| 3 | Insert into RocksDB | `store.insert_batch()` with _seq | ✅ IMPLEMENTED |
| 4 | Update manifest SyncState | `mark_pending_write()` called after insert | ✅ IMPLEMENTED |

**GAP IDENTIFIED**: PK existence check only validates against **hot storage** (RocksDB PK index). Cold storage (Parquet) is NOT checked during INSERT, only during UPDATE/DELETE when using `find_by_pk()`.

### 2.2 UPDATE Flow

| Step | Desired Spec | Current Implementation | Status |
|------|--------------|------------------------|--------|
| 1 | User performs update | `UpdateHandler` → route based on table type | ✅ IMPLEMENTED |
| 2a | Check if PK specified | Extracted from WHERE clause | ✅ IMPLEMENTED |
| 2b | If PK not specified | Error: "UPDATE requires PK filter" | ✅ IMPLEMENTED |
| 2c | Check PK exists (hot+cold) | `find_by_pk()` checks hot, then `base::find_row_by_pk()` for cold | ✅ IMPLEMENTED |
| 2d | If not exists → error | `NotFound` error returned | ✅ IMPLEMENTED |
| 3 | Fetch existing row (for merge) | `find_by_pk()` returns full row | ✅ IMPLEMENTED |
| 4 | Insert new version in RocksDB | New row with higher _seq | ✅ IMPLEMENTED |
| 5 | Update manifest SyncState | `mark_pending_write()` called after update | ✅ IMPLEMENTED |

**STATUS**: Fully aligned with desired spec.

### 2.3 DELETE Flow

| Step | Desired Spec | Current Implementation | Status |
|------|--------------|------------------------|--------|
| 1 | User performs delete | `DeleteHandler` → route based on table type | ✅ IMPLEMENTED |
| 2 | Check PK specified | Extracted from WHERE clause | ✅ IMPLEMENTED |
| 3 | Check PK exists (hot+cold) | Uses same flow as UPDATE | ✅ IMPLEMENTED |
| 4 | Mark as deleted in RocksDB | New row with `_deleted=true` and higher _seq | ✅ IMPLEMENTED |
| 5 | Update manifest SyncState | `mark_pending_write()` called after delete | ✅ IMPLEMENTED |

**STATUS**: Fully aligned with desired spec.

### 2.4 getByPk Flow

| Step | Desired Spec | Current Implementation | Status |
|------|--------------|------------------------|--------|
| 1 | Go to manifest service | Via `find_by_pk()` method | ✅ IMPLEMENTED |
| 2 | Ensure latest manifest in cache | `ManifestService.get_or_load()` | ✅ IMPLEMENTED |
| 3 | Verify PK in segment range | `ManifestAccessPlanner.plan_by_pk_value()` | ✅ IMPLEMENTED |
| 4 | Check location (hot or cold) | Hot first via PK index, then cold via Parquet | ✅ IMPLEMENTED |
| 5 | Read from storage | RocksDB or Parquet with Bloom filter | ✅ IMPLEMENTED |
| 6 | Return exists/not exists | `PkCheckResult` enum | ✅ IMPLEMENTED |

**STATUS**: Fully aligned with desired spec.

### 2.5 markManifest Flow

| Step | Desired Spec | Current Implementation | Status |
|------|--------------|------------------------|--------|
| 1 | Fetch manifest from cache | `ManifestService.get_or_load()` | ✅ IMPLEMENTED |
| 2 | Check for in_progress segments | Not implemented - no segment-level status | ⚠️ PARTIAL |
| 3 | Update with max seq number | `Manifest.last_sequence_number` tracked | ✅ IMPLEMENTED |
| 4 | Set SyncState = PendingWrite | `mark_pending_write()` | ✅ IMPLEMENTED |
| 5 | Add secondary index for PendingWrite lookup | **NOT IMPLEMENTED** | ❌ MISSING |

**GAP IDENTIFIED**: 
- No segment-level `in_progress` status for retry safety
- No secondary index for efficient PendingWrite lookup (currently scans all manifests)

### 2.6 Flushing Flow

| Step | Desired Spec | Current Implementation | Status |
|------|--------------|------------------------|--------|
| 1 | Flush command OR scheduled job | `FlushTableHandler` + `JobsManager` | ✅ IMPLEMENTED |
| 2 | Look for PendingWrite manifests | **NOT IMPLEMENTED** - scans RocksDB directly | ❌ MISSING |
| 3 | Process per user/shared | `UserTableFlushJob` / `SharedTableFlushJob` | ✅ IMPLEMENTED |
| 4 | Write manifest to external storage | `flush_manifest()` writes manifest.json | ✅ IMPLEMENTED |
| 5 | Update SyncState = InSync | `update_after_flush()` | ✅ IMPLEMENTED |
| 6 | Remove PendingWrite secondary index | N/A - secondary index not implemented | ❌ MISSING |
| 7 | Remove flushed rows from RocksDB | `delete_flushed_keys()` | ✅ IMPLEMENTED |

**GAP IDENTIFIED**: Flush job scans all hot storage rows instead of using manifest SyncState to identify what needs flushing.

### 2.7 SELECT Flow

| Step | Desired Spec | Current Implementation | Status |
|------|--------------|------------------------|--------|
| 1 | User performs select | DataFusion SQL execution | ✅ IMPLEMENTED |
| 2 | Read from hot and cold | `HybridParquetStream` combines both | ✅ IMPLEMENTED |
| 3 | Read hot from RocksDB | Via table provider scan | ✅ IMPLEMENTED |
| 4 | Read cold from Parquet | Via manifest-based file list | ✅ IMPLEMENTED |
| 5 | Merge results | Combine batches | ✅ IMPLEMENTED |
| 6 | Version resolution | MAX(_seq) per PK, exclude _deleted | ✅ IMPLEMENTED |
| 7 | Return final result | DataFusion stream | ✅ IMPLEMENTED |

**STATUS**: Fully aligned with desired spec.

### 2.8 Batch Operations

| Feature | Desired Spec | Current Implementation | Status |
|---------|--------------|------------------------|--------|
| Batch INSERT | Single `mark_pending_write()` call | Called once after all rows inserted | ✅ IMPLEMENTED |
| Batch UPDATE | Batched manifest update | Each update individually calls mark | ⚠️ PARTIAL |
| Batch DELETE | Batched manifest update | Each delete individually calls mark | ⚠️ PARTIAL |

**GAP IDENTIFIED**: UPDATE/DELETE operations call `mark_pending_write()` per row instead of once per batch.

---

## 3. Gap Analysis Summary

### 3.1 Critical Gaps (HIGH Priority)

| Gap | Description | Impact | Effort |
|-----|-------------|--------|--------|
| **G1** | PK uniqueness check on INSERT doesn't check cold storage | Duplicate PKs possible if data was flushed | HIGH |
| **G2** | No secondary index for PendingWrite manifests | Flush job scans all manifests - O(N) instead of O(1) | MEDIUM |
| **G3** | No segment-level in_progress status | Flush retry may create duplicate segments | HIGH |

### 3.2 Medium Gaps

| Gap | Description | Impact | Effort |
|-----|-------------|--------|--------|
| **G4** | UPDATE/DELETE mark_pending_write per row | Extra writes to manifest cache | LOW |
| **G5** | Flush doesn't use PendingWrite to find tables | Scans all rows instead of targeted flush | MEDIUM |

### 3.3 Low Priority / Future

| Gap | Description | Impact | Effort |
|-----|-------------|--------|--------|
| **G6** | Manifest eviction scans all entries | Scalability concern at large scale | LOW |

---

## 4. Implementation Tasks

### 4.5 Consistency & Transactional Guarantees (NEW)

#### 4.5.1 Hot Storage + Manifest Atomicity

All DML batches must update hot rows and manifest state **atomically**. If either side fails, rollback both. This prevents mismatches such as:

- Hot rows exist but manifest not marked PendingWrite
- Manifest marked PendingWrite without corresponding hot rows

**Required behavior**:

- For INSERT/UPDATE/DELETE batch paths, use a single transactional write unit that includes:
    - Hot row inserts/updates/deletes
    - PendingWrite marker update for the manifest (cache entry)

**Preferred approach**:

1. Use RocksDB WriteBatch (or TransactionDB) that includes:
     - Row CF writes
     - Manifest cache CF writes (PendingWrite transition)
2. If any write fails, **rollback** the entire batch.

**Notes**:

- If the manifest cache is in a different store or abstraction, expose a method to write to the same RocksDB backend to enable cross-CF transactions.
- Avoid partial writes when using `mark_pending_write()` by making it part of the same WriteBatch.

#### 4.5.2 Consistent Reads During Flush

Reads must remain consistent under concurrent flush:

- SELECT reads hot + cold, resolves by MAX(_seq)
- If flush finishes and hot rows are removed, cold still contains same data
- If flush fails, hot remains; manifest stays unchanged or flagged Error/Syncing

This is already correct via MVCC but needs tests to guard against regressions (see 6.4, 6.6).

#### 4.5.3 Avoid Duplicated Code

Consolidate any repeated logic in:

- `FlushManifestHelper` (segment updates, SyncState transitions)
- `PkExistenceChecker` (hot + cold checks)
- `TableFlush` helpers (version resolution, tombstone filtering, delete flushed rows)

Before adding new logic, search for duplicated implementations in:

- User vs Shared table providers
- User vs Shared flush jobs
- Manifest update paths (stage, syncing, commit)

Add unit tests around each shared helper to guarantee identical behavior across table types.

### Task 1: PK Uniqueness Check on INSERT (G1)

**Problem**: When a row is flushed to Parquet and deleted from RocksDB, a subsequent INSERT with the same PK succeeds (but shouldn't).

**Current Flow**:
```rust
// user_table_provider.rs insert_batch()
// Only checks hot storage via PK index
if self.find_row_key_by_id_field(user_id, pk_str)?.is_some() {
    return Err(AlreadyExists(...));
}
```

**Required Flow**:
```rust
// 1. Check hot storage (fast path)
if self.find_row_key_by_id_field(user_id, pk_str)?.is_some() {
    return Err(AlreadyExists(...));
}

// 2. Check cold storage (only if table has been flushed)
let manifest = manifest_service.get_or_load(table_id, Some(user_id))?;
if manifest.is_some() && !manifest.segments.is_empty() {
    // Use PkExistenceChecker for manifest-based pruning
    let result = pk_checker.check_pk_exists(..., pk_str)?;
    if result.exists() {
        return Err(AlreadyExists(...));
    }
}
```

**Files to Modify**:
- `kalamdb-tables/src/user_tables/user_table_provider.rs` - `insert_batch()`
- `kalamdb-tables/src/shared_tables/shared_table_provider.rs` - `insert_batch()`

**Estimated Effort**: 4-6 hours

---

### Task 2: Secondary Index for PendingWrite Manifests (G2)

**Problem**: Finding manifests that need flushing requires scanning all manifest entries.

**Solution**: Add a RocksDB column family for secondary index:
```
Key: {table_id}:pending_write:{user_id?}
Value: timestamp
```

**Implementation**:

1. Create new column family: `manifest_pending_idx`

2. Update `mark_pending_write()`:
```rust
pub fn mark_pending_write(&self, table_id: &TableId, user_id: Option<&UserId>) {
    // ... existing code ...
    
    // Add to secondary index
    let idx_key = format!("{}:pending_write:{}", 
        table_id, 
        user_id.map(|u| u.as_str()).unwrap_or("_shared")
    );
    self.pending_index_cf.put(idx_key, timestamp)?;
}
```

3. Update `update_after_flush()`:
```rust
pub fn update_after_flush(&self, ...) {
    // ... existing code ...
    
    // Remove from secondary index
    let idx_key = format!("{}:pending_write:{}", ...);
    self.pending_index_cf.delete(idx_key)?;
}
```

4. Add `get_pending_manifests()`:
```rust
pub fn get_pending_manifests(&self) -> Result<Vec<(TableId, Option<UserId>)>, StorageError> {
    self.pending_index_cf.scan_all()
}
```

**Files to Modify**:
- `kalamdb-core/src/manifest/service.rs`
- `kalamdb-system/src/providers/manifest/manifest_store.rs`

**Estimated Effort**: 6-8 hours

---

### Task 3: Segment-Level in_progress Status (G3)

**Problem**: If flush fails after writing Parquet but before updating manifest, retry creates duplicate segments.

**Solution**: Add `SegmentStatus` to `SegmentMetadata`:

```rust
// manifest.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SegmentStatus {
    /// Segment write in progress (temp file exists)
    InProgress,
    /// Segment successfully written (final file exists)
    Committed,
    /// Segment marked for deletion (compaction)
    Tombstone,
}

pub struct SegmentMetadata {
    // ... existing fields ...
    
    /// Status of this segment
    pub status: SegmentStatus,
}
```

**Flush Flow Changes**:

1. **Before writing Parquet**: Add segment with `InProgress` status
2. **After atomic rename**: Update segment to `Committed` status
3. **On server restart**: Scan for `InProgress` segments → cleanup temp files

```rust
// flush_helper.rs
pub fn create_in_progress_segment(&self, ...) -> Result<(), KalamDbError> {
    let segment = SegmentMetadata::in_progress(
        batch_filename.clone(),
        expected_seq_range,
    );
    self.manifest_service.add_segment_in_progress(table_id, user_id, segment)?;
    Ok(())
}

pub fn commit_segment(&self, ...) -> Result<(), KalamDbError> {
    self.manifest_service.update_segment_status(
        table_id, user_id, segment_id, SegmentStatus::Committed
    )?;
    Ok(())
}
```

**Files to Modify**:
- `kalamdb-commons/src/models/types/manifest.rs` - Add `SegmentStatus`
- `kalamdb-core/src/manifest/service.rs` - Segment lifecycle methods
- `kalamdb-core/src/manifest/flush_helper.rs` - Two-phase segment creation
- `kalamdb-core/src/manifest/flush/users.rs` - Update flush flow
- `kalamdb-core/src/manifest/flush/shared.rs` - Update flush flow

**Estimated Effort**: 8-12 hours

---

### Task 4: Batch mark_pending_write for UPDATE/DELETE (G4)

**Problem**: Each UPDATE/DELETE row calls `mark_pending_write()` individually.

**Solution**: Make `mark_pending_write()` idempotent and batch-aware:

```rust
// Already idempotent - just ensure single call per batch
pub fn update_batch(&self, user_id: &UserId, updates: Vec<(RowId, Row)>) -> Result<...> {
    // ... perform all updates ...
    
    // Single manifest update at end
    if !updates.is_empty() {
        manifest_service.mark_pending_write(table_id, Some(user_id))?;
    }
}
```

**Files to Modify**:
- `kalamdb-tables/src/user_tables/user_table_provider.rs` - Batch methods
- `kalamdb-tables/src/shared_tables/shared_table_provider.rs` - Batch methods

**Estimated Effort**: 2-3 hours

---

### Task 5: Manifest-Driven Flush Discovery (G5)

**Problem**: Flush job scans all hot storage rows to find data to flush.

**Solution**: Use `get_pending_manifests()` (from Task 2) to identify what needs flushing:

```rust
// FlushExecutor
async fn do_flush(&self, ctx: &JobContext<FlushParams>) -> Result<JobDecision, KalamDbError> {
    // Option A: Specific table flush (current behavior)
    if ctx.params().table_id.is_specified() {
        return self.flush_table(&ctx.params().table_id).await;
    }
    
    // Option B: Auto-discover tables with pending writes
    let pending = self.manifest_service.get_pending_manifests()?;
    for (table_id, user_id) in pending {
        self.flush_scope(&table_id, user_id.as_ref()).await?;
    }
}
```

**Files to Modify**:
- `kalamdb-core/src/jobs/executors/flush.rs`
- `kalamdb-core/src/manifest/flush/users.rs` - Optional: Skip scan if no pending writes

**Estimated Effort**: 4-6 hours

---

## 5. Implementation Order

Recommended order based on dependencies and risk:

```
┌─────────────────────────────────────────────────────────────────┐
│ Phase 1: Critical Data Integrity (Week 1)                        │
├─────────────────────────────────────────────────────────────────┤
│ Task 1: PK Uniqueness Check on INSERT (G1)                      │
│   - Prevents duplicate PKs across hot/cold storage              │
│   - Required for data correctness                               │
│                                                                 │
│ Task 3: Segment in_progress Status (G3)                         │
│   - Prevents duplicate segments on flush retry                  │
│   - Required for crash safety                                   │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ Phase 2: Performance Optimization (Week 2)                       │
├─────────────────────────────────────────────────────────────────┤
│ Task 2: Secondary Index for PendingWrite (G2)                   │
│   - Enables O(1) lookup of tables needing flush                 │
│   - Foundation for Task 5                                       │
│                                                                 │
│ Task 5: Manifest-Driven Flush Discovery (G5)                    │
│   - Uses secondary index from Task 2                            │
│   - Reduces flush overhead                                      │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ Phase 3: Minor Optimizations (Week 3)                            │
├─────────────────────────────────────────────────────────────────┤
│ Task 4: Batch mark_pending_write (G4)                           │
│   - Minor performance improvement                               │
│   - Low risk change                                             │
└─────────────────────────────────────────────────────────────────┘
```

---

## 6. Testing Requirements

### 6.0 New Consistency & Failure Coverage

Add focused tests to ensure correctness under flush failures, partial writes, and high concurrency. The goals:

- **Manifest ↔ Hot storage consistency**: Any DML batch that updates hot rows must also update manifest state atomically.
- **Flush correctness**: If flush fails mid-way, the system must not produce duplicate segments or lose data.
- **Read stability while flushing**: SELECT results remain consistent and version-resolved during concurrent flush.

#### Required test categories

1) **Unit tests** (fast, isolated):
    - Segment lifecycle: `InProgress` → `Committed` or `Error`
    - PendingWrite index add/remove
    - Hot+manifest transactional write behavior (rollback on error)

2) **Integration tests** (real storage, local filestore):
    - Crash/failure injection during flush
    - Concurrent flush + read
    - Duplicate PK detection after cold flush

3) **E2E tests** (cluster + load):
    - Many user_ids inserting concurrently
    - Flush under load with consistent results
    - Jobs/flush recovery behavior

### 6.1 Task 1 Tests (PK Uniqueness)

```rust
#[test]
async fn test_insert_pk_uniqueness_after_flush() {
    // 1. Create table with PK column
    // 2. Insert row with pk=1
    // 3. Flush table
    // 4. Verify row is in Parquet (cold), not in RocksDB (hot)
    // 5. Attempt INSERT with pk=1 → should fail with AlreadyExists
}

#[test]
async fn test_insert_pk_uniqueness_hot_and_cold() {
    // 1. Insert row with pk=1 (stays in hot)
    // 2. Insert row with pk=2, flush (goes to cold)
    // 3. Attempt INSERT pk=1 → should fail (hot check)
    // 4. Attempt INSERT pk=2 → should fail (cold check)
    // 5. INSERT pk=3 → should succeed
}
```

### 6.2 Task 3 Tests (Segment Status)

```rust
#[test]
async fn test_segment_in_progress_cleanup() {
    // 1. Start flush, simulate crash after Parquet write but before manifest update
    // 2. Restart server
    // 3. Verify InProgress segment is cleaned up
    // 4. Verify temp file is deleted
    // 5. Re-flush succeeds
}

#[test]
async fn test_segment_status_transitions() {
    // Verify: InProgress → Committed → Tombstone lifecycle
}
```

### 6.3 Task 2 Tests (Secondary Index)
### 6.4 Flush Failure Injection Tests (NEW)

```rust
#[test]
async fn test_flush_failure_before_manifest_commit() {
    // 1. Insert rows (hot)
    // 2. Inject failure after Parquet temp write, before manifest update
    // 3. Verify manifest has no committed segment
    // 4. Verify hot rows still exist
    // 5. Verify subsequent flush succeeds without duplicate segments
}

#[test]
async fn test_flush_failure_after_manifest_commit_before_delete_hot() {
    // 1. Insert rows (hot)
    // 2. Inject failure after manifest commit, before hot deletion
    // 3. Verify manifest includes segment
    // 4. Verify hot rows still exist (dup in hot + cold)
    // 5. SELECT should dedup by MAX(_seq)
}

#[test]
async fn test_read_consistency_during_flush() {
    // 1. Start flush in background
    // 2. Concurrent SELECTs must never return partial or inconsistent data
    // 3. Results should always respect MVCC (MAX(_seq), _deleted)
}
```

### 6.5 Hot+Manifest Consistency Tests (NEW)

```rust
#[test]
async fn test_hot_manifest_batch_atomicity_insert() {
    // 1. Inject failure after hot write but before manifest pending_write update
    // 2. Ensure transaction rollback removes hot rows
    // 3. Ensure manifest not marked PendingWrite
}

#[test]
async fn test_hot_manifest_batch_atomicity_update_delete() {
    // 1. Inject failure during update/delete batch
    // 2. Verify no partial hot rows
    // 3. Verify manifest unchanged
}
```

### 6.6 High-load + Multi-User E2E Tests (NEW)

```rust
#[test]
async fn test_high_load_multi_user_insert_flush_select() {
    // 1. N users concurrently insert with random PKs
    // 2. Periodically flush
    // 3. Concurrent SELECT (both hot and cold) must be correct
    // 4. Validate row counts and PK uniqueness
}

#[test]
async fn test_concurrent_updates_across_users_during_flush() {
    // 1. Multiple users updating same table
    // 2. Start flush while updates are ongoing
    // 3. Verify all updates visible with correct ordering
    // 4. No missing or duplicated rows
}
```

```rust
#[test]
async fn test_pending_write_index_lifecycle() {
    // 1. Insert data → verify manifest in pending index
    // 2. Flush → verify manifest removed from pending index
    // 3. Insert more → verify back in pending index
}
```

---

## 7. Backward Compatibility

### 7.1 Manifest Schema Changes (Task 3)

The `SegmentStatus` field is new. Existing manifests won't have it.

**Migration Strategy**:
```rust
// Use serde default
#[serde(default = "default_segment_status")]
pub status: SegmentStatus,

fn default_segment_status() -> SegmentStatus {
    // Existing segments are already committed
    SegmentStatus::Committed
}
```

### 7.2 Secondary Index (Task 2)

New column family - no migration needed. On first write, index will be populated.

---

## 8. Metrics & Observability

Add metrics for monitoring:

```rust
// Counters
pk_check_hot_hits_total
pk_check_cold_hits_total
pk_check_manifest_pruned_total
flush_pending_manifests_total
segment_in_progress_cleanups_total

// Gauges
pending_write_manifests_count
in_progress_segments_count
```

---

## 9. Appendix: SyncState State Machine

```
                    ┌──────────┐
                    │  InSync  │ ←────────────────────┐
                    └────┬─────┘                      │
                         │                            │
                   INSERT/UPDATE/DELETE               │
                         ↓                            │
                ┌────────────────┐                    │
                │  PendingWrite  │                    │
                └────────┬───────┘                    │
                         │                            │
                   Flush Started                      │
                         ↓                            │
                    ┌─────────┐                       │
                    │ Syncing │                       │
                    └────┬────┘                       │
                         │                            │
              ┌──────────┴──────────┐                │
              │                     │                │
         Success                 Failure              │
              ↓                     ↓                │
         InSync ←───────       ┌───────┐            │
              ↑                │ Error │            │
              │                └───┬───┘            │
              │                    │                │
              │              Retry/Recovery         │
              └────────────────────┘                │
                                                    │
                                          TTL Expires
                                                    │
                              ┌───────────────────────┘
                              ↓
                         ┌───────┐
                         │ Stale │
                         └───────┘
```

---

## 10. References

- [manifest.md](docs/architecture/manifest.md) - Current manifest architecture docs
- [flush.rs](backend/crates/kalamdb-core/src/jobs/executors/flush.rs) - Flush executor implementation
- [existence_checker.rs](backend/crates/kalamdb-tables/src/utils/pk/existence_checker.rs) - PK existence checker
- [user_table_provider.rs](backend/crates/kalamdb-tables/src/user_tables/user_table_provider.rs) - User table operations
