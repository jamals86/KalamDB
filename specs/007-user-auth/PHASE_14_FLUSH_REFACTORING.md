# Phase 14: Flush Architecture Refactoring

This document extends Phase 14 to refactor the flush implementation to align with the new EntityStore architecture.

## Current Issues

### 1. **Flush Modules in Wrong Location**

**Problem**: Flush logic is in `backend/crates/kalamdb-core/src/flush/` instead of co-located with table providers:

```
❌ CURRENT:
backend/crates/kalamdb-core/src/
├── flush/
│   ├── shared_table_flush.rs  # 614 lines
│   ├── user_table_flush.rs    # 981 lines
│   └── mod.rs
├── tables/
│   ├── shared_table_provider.rs
│   └── user_table_provider.rs
```

**Issues**:
- Violates co-location principle (flush logic separate from table logic)
- Hard to find related code (provider in one folder, flush in another)
- Doesn't align with new folder structure (`tables/user/`, `tables/shared/`)

**Solution**: Move flush modules into table folders:

```
✅ NEW:
backend/crates/kalamdb-core/src/tables/
├── user/
│   ├── user_table_provider.rs
│   ├── user_table_store.rs
│   └── user_table_flush.rs     # Moved here ✅
├── shared/
│   ├── shared_table_provider.rs
│   ├── shared_table_store.rs
│   └── shared_table_flush.rs   # Moved here ✅
└── system/
    └── {table}/
        ├── {table}_provider.rs
        ├── {table}_store.rs
        └── (no flush - system tables don't flush to Parquet)
```

---

### 2. **Code Duplication: 400+ Lines**

**Problem**: `shared_table_flush.rs` and `user_table_flush.rs` share 70% of code:

**Shared Code** (duplicated):
- Job creation and tracking
- Job record initialization
- Error handling and logging
- LiveQuery notifications
- Metrics collection (duration, row count)
- Job completion/failure status

**Different Code** (unique):
- `shared`: Single Parquet file for all rows
- `user`: Multiple Parquet files (one per user)
- `shared`: Simple scan_iter()
- `user`: Group by user_id, then scan per user

**Duplication Stats**:
- Shared logic: ~400 lines duplicated
- Unique logic: ~200 lines per implementation
- Total: 1,595 lines (should be ~600 lines)

---

### 3. **Snapshot Support Already Exists** ✅

**Good News**: The iterator-based scanning already uses RocksDB snapshots!

**Current Implementation** (from `kalamdb-store/src/rocksdb_impl.rs`):

```rust
fn scan(...) -> Result<Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_>> {
    // ✅ Take a consistent snapshot for the duration of the iterator
    let snapshot = self.db.snapshot();
    
    // Bind snapshot to ReadOptions
    let mut readopts = rocksdb::ReadOptions::default();
    readopts.set_snapshot(&snapshot);
    
    let inner = self.db.iterator_cf_opt(cf, readopts, iter_mode);
    
    // ✅ Iterator holds snapshot reference - ensures consistency
    struct SnapshotScanIter<'a, D: rocksdb::DBAccess> {
        _snapshot: rocksdb::SnapshotWithThreadMode<'a, D>,  // Keeps snapshot alive
        inner: rocksdb::DBIteratorWithThreadMode<'a, D>,
        // ...
    }
}
```

**Benefits**:
- ✅ **ACID Guarantees**: Flush sees consistent view of data
- ✅ **No Locking**: Writes continue during flush
- ✅ **Performance**: Zero-copy reads from snapshot
- ✅ **Isolation**: Concurrent flushes don't interfere

**Works With**:
- ✅ `scan_iter()` - Already snapshot-backed
- ✅ `scan_prefix_iter()` - Will use same snapshot mechanism
- ✅ Future optimizations (T222-T228) - All compatible

---

## Proposed Architecture

### Base Flush Trait

Create a generic flush trait that shared/user implementations extend:

```rust
// File: backend/crates/kalamdb-core/src/tables/base_flush.rs

use crate::error::KalamDbError;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::system::Job;
use std::sync::Arc;

/// Common flush job result
#[derive(Debug, Clone)]
pub struct FlushJobResult {
    /// Job record for system.jobs table
    pub job_record: Job,
    
    /// Total rows flushed
    pub rows_flushed: usize,
    
    /// Parquet files written
    pub parquet_files: Vec<String>,
    
    /// Additional metadata (users_count, etc.)
    pub metadata: serde_json::Value,
}

/// Base trait for table flush operations
pub trait TableFlush: Send + Sync {
    /// Execute the flush job
    fn execute(&self) -> Result<FlushJobResult, KalamDbError>;
    
    /// Get table identifier (for logging)
    fn table_identifier(&self) -> String;
    
    /// Generate job ID
    fn generate_job_id(&self) -> String {
        format!(
            "flush-{}-{}",
            self.table_identifier(),
            chrono::Utc::now().timestamp_millis()
        )
    }
    
    /// Create base job record (common fields)
    fn create_job_record(&self, job_id: &str) -> Job {
        // Common job initialization logic
        // ...
    }
    
    /// Send LiveQuery notification (if manager configured)
    fn notify_flush(&self, rows_flushed: usize, parquet_files: Vec<String>) {
        // Common notification logic
        // ...
    }
    
    /// Build RecordBatch from rows (uses optimized columnar builder)
    fn build_record_batch(
        &self,
        schema: &SchemaRef,
        rows: Vec<serde_json::Value>,
    ) -> Result<RecordBatch, KalamDbError> {
        // Use optimized batch builder from T224
        crate::tables::batch_builder::rows_to_batch_optimized(schema, rows.into_iter())
    }
}

/// Template method pattern for common flush workflow
pub struct FlushExecutor;

impl FlushExecutor {
    /// Execute flush with common error handling and metrics
    pub fn execute_with_tracking<F: TableFlush>(
        flush_job: &F,
    ) -> Result<FlushJobResult, KalamDbError> {
        let job_id = flush_job.generate_job_id();
        let mut job_record = flush_job.create_job_record(&job_id);
        
        log::info!(
            "🚀 Flush job started: job_id={}, table={}",
            job_id,
            flush_job.table_identifier()
        );
        
        // Execute with metrics
        let start_time = std::time::Instant::now();
        let result = match flush_job.execute() {
            Ok(mut res) => {
                let duration_ms = start_time.elapsed().as_millis();
                log::info!(
                    "✅ Flush completed: job_id={}, rows={}, duration_ms={}",
                    job_id,
                    res.rows_flushed,
                    duration_ms
                );
                
                // Update job record
                res.job_record = res.job_record.complete(Some(
                    serde_json::json!({
                        "rows_flushed": res.rows_flushed,
                        "duration_ms": duration_ms,
                        "parquet_files": res.parquet_files,
                    }).to_string()
                ));
                
                // Send notification
                flush_job.notify_flush(res.rows_flushed, res.parquet_files.clone());
                
                Ok(res)
            }
            Err(e) => {
                log::error!("❌ Flush failed: job_id={}, error={}", job_id, e);
                job_record = job_record.fail(format!("Flush failed: {}", e));
                Err(e)
            }
        };
        
        result
    }
}
```

---

### Shared Table Flush (Simplified)

```rust
// File: backend/crates/kalamdb-core/src/tables/shared/shared_table_flush.rs

use super::super::base_flush::{TableFlush, FlushJobResult, FlushExecutor};
use super::shared_table_store::SharedTableStoreImpl;
use crate::storage::ParquetWriter;
use std::sync::Arc;

pub struct SharedTableFlushJob {
    store: Arc<SharedTableStoreImpl>,
    namespace_id: NamespaceId,
    table_name: TableName,
    schema: SchemaRef,
    storage_location: String,
}

impl TableFlush for SharedTableFlushJob {
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        // ✅ Unique logic: Single Parquet file for all rows
        
        // Stream snapshot-backed scan (already uses snapshot!)
        let rows_iter = self.store
            .scan_iter()?  // ✅ Snapshot-backed!
            .filter(|result| {
                result.as_ref().ok()
                    .map_or(false, |(_, row)| !row._deleted)
            });
        
        // Collect rows (will use iterator in T222-T228 optimization)
        let rows: Vec<_> = rows_iter.collect::<Result<Vec<_>, _>>()?;
        let rows_count = rows.len();
        
        // Build RecordBatch (uses base trait's optimized builder)
        let batch = self.build_record_batch(
            &self.schema,
            rows.into_iter().map(|(_, row)| row.fields).collect()
        )?;
        
        // Write Parquet file
        let filename = format!("batch-{}.parquet", chrono::Utc::now().timestamp_millis());
        let parquet_path = PathBuf::from(&self.storage_location).join(&filename);
        
        ParquetWriter::write_batch(&parquet_path, batch)?;
        
        Ok(FlushJobResult {
            job_record: Job::new(/* ... */),
            rows_flushed: rows_count,
            parquet_files: vec![filename],
            metadata: serde_json::json!({}),
        })
    }
    
    fn table_identifier(&self) -> String {
        format!("{}.{}", self.namespace_id.as_str(), self.table_name.as_str())
    }
}

// Public API
impl SharedTableFlushJob {
    pub fn execute_tracked(&self) -> Result<FlushJobResult, KalamDbError> {
        FlushExecutor::execute_with_tracking(self)
    }
}
```

**Code Reduction**:
- Before: 614 lines
- After: ~150 lines (75% reduction)

---

### User Table Flush (Simplified)

```rust
// File: backend/crates/kalamdb-core/src/tables/user/user_table_flush.rs

use super::super::base_flush::{TableFlush, FlushJobResult, FlushExecutor};
use super::user_table_store::UserTableStoreImpl;
use std::collections::HashMap;

pub struct UserTableFlushJob {
    store: Arc<UserTableStoreImpl>,
    namespace_id: NamespaceId,
    table_name: TableName,
    schema: SchemaRef,
    storage_registry: Arc<StorageRegistry>,
}

impl TableFlush for UserTableFlushJob {
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        // ✅ Unique logic: Group by user, multiple Parquet files
        
        // Stream snapshot-backed scan (already uses snapshot!)
        let rows_iter = self.store
            .scan_iter()?  // ✅ Snapshot-backed!
            .filter(|result| {
                result.as_ref().ok()
                    .map_or(false, |(_, row)| !row._deleted)
            });
        
        // Group by user_id
        let mut rows_by_user: HashMap<UserId, Vec<_>> = HashMap::new();
        for result in rows_iter {
            let (key, row) = result?;
            let user_id = key.user_id();  // Extract from UserRowId composite key
            rows_by_user.entry(user_id).or_default().push(row);
        }
        
        // Write one Parquet file per user
        let mut parquet_files = Vec::new();
        let mut total_rows = 0;
        
        for (user_id, rows) in rows_by_user {
            let row_count = rows.len();
            total_rows += row_count;
            
            // Build RecordBatch (uses base trait's optimized builder)
            let batch = self.build_record_batch(
                &self.schema,
                rows.into_iter().map(|r| r.fields).collect()
            )?;
            
            // Resolve storage path for user
            let storage_path = self.storage_registry
                .resolve_user_table_path(&self.namespace_id, &self.table_name, &user_id)?;
            
            let filename = format!("batch-{}.parquet", chrono::Utc::now().timestamp_millis());
            let parquet_path = storage_path.join(&filename);
            
            ParquetWriter::write_batch(&parquet_path, batch)?;
            parquet_files.push(filename);
        }
        
        Ok(FlushJobResult {
            job_record: Job::new(/* ... */),
            rows_flushed: total_rows,
            parquet_files,
            metadata: serde_json::json!({ "users_count": rows_by_user.len() }),
        })
    }
    
    fn table_identifier(&self) -> String {
        format!("{}.{}", self.namespace_id.as_str(), self.table_name.as_str())
    }
}

// Public API
impl UserTableFlushJob {
    pub fn execute_tracked(&self) -> Result<FlushJobResult, KalamDbError> {
        FlushExecutor::execute_with_tracking(self)
    }
}
```

**Code Reduction**:
- Before: 981 lines
- After: ~200 lines (80% reduction)

---

## Snapshot Guarantees for Flush

### How Snapshots Ensure Consistency

```rust
// RocksDB snapshot lifecycle during flush

1. Flush job starts
   ↓
2. store.scan_iter() called
   ↓
3. RocksDB creates snapshot
   let snapshot = self.db.snapshot();  // ← Point-in-time view
   ↓
4. Iterator holds snapshot reference
   struct SnapshotScanIter {
       _snapshot: SnapshotWithThreadMode<'a>,  // ← Keeps snapshot alive
       inner: DBIteratorWithThreadMode<'a>,
   }
   ↓
5. Flush reads from snapshot (consistent view)
   - Concurrent writes go to different version
   - Flush sees data as of step 3
   ↓
6. Iterator dropped → snapshot released
   - Snapshot automatically cleaned up
   - Disk space reclaimed
```

### Concurrent Write Handling

**Scenario**: Flush running while new writes arrive

```rust
Timeline:
T0: Flush starts, snapshot created (sees rows 1-1000)
T1: New row 1001 written (not in snapshot)
T2: Row 500 updated (snapshot still sees old version)
T3: Row 200 deleted (snapshot still sees row 200)
T4: Flush completes, snapshot released
T5: Next flush sees rows 1-199, 201-499, 501-1000, 1001 (new state)
```

**Benefits**:
- ✅ Flush never sees partial writes
- ✅ No "torn reads" (row half-updated)
- ✅ Deterministic Parquet files (repeatable flushes)
- ✅ Zero locking overhead

---

## New Tasks for Phase 14

### Step 11: Flush Architecture Refactoring (T229-T234)

- [ ] **T229** [P] Create base_flush.rs in `backend/crates/kalamdb-core/src/tables/` (TableFlush trait, FlushJobResult struct, FlushExecutor helper with template method pattern for common job tracking/metrics/notifications)

- [ ] **T230** [P] Move shared_table_flush.rs to `backend/crates/kalamdb-core/src/tables/shared/shared_table_flush.rs` (implement TableFlush trait, remove duplicated job tracking code, use FlushExecutor::execute_with_tracking())

- [ ] **T231** [P] Move user_table_flush.rs to `backend/crates/kalamdb-core/src/tables/user/user_table_flush.rs` (implement TableFlush trait, remove duplicated job tracking code, use FlushExecutor::execute_with_tracking())

- [ ] **T232** [P] Update flush service in `backend/crates/kalamdb-core/src/services/flush_service.rs` (change imports from `crate::flush::*` to `crate::tables::user::user_table_flush`, `crate::tables::shared::shared_table_flush`)

- [ ] **T233** [P] Delete old flush folder `backend/crates/kalamdb-core/src/flush/` (remove shared_table_flush.rs, user_table_flush.rs, mod.rs - logic moved to tables/ subfolders)

- [ ] **T234** [P] Update flush tests to use new paths (change imports in integration tests, verify snapshot consistency during concurrent writes)

**Code Reduction**:
- Before: 1,595 lines (614 + 981)
- After: ~550 lines (150 base + 200 user + 200 shared)
- Eliminated: ~1,045 lines (65% reduction)

---

## Benefits Summary

### 1. **Co-location** ✅
- Flush logic lives with table providers
- Easier to find and maintain related code
- Aligns with Phase 14 folder structure

### 2. **DRY Principle** ✅
- 1,045 lines of duplicated code eliminated
- Job tracking, metrics, notifications in base trait
- Only unique logic (grouping, file naming) in implementations

### 3. **Snapshot Consistency** ✅
- Already works! `scan_iter()` uses snapshots
- ACID guarantees during flush
- Zero additional overhead

### 4. **Future-Proof** ✅
- Compatible with T222-T228 iterator optimizations
- Base trait makes adding new table types easy
- Template method pattern for consistent behavior

### 5. **Performance** ✅
- Zero-copy snapshot reads
- No locking during flush
- Optimized batch building (when T224 implemented)
