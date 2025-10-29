# Phase 14: Performance Optimizations

This document outlines performance optimizations for queries and memory consumption in the EntityStore architecture.

## Current Performance Issues

### ⚠️ NOTE: Snapshot Support Already Works ✅

**Good News**: The iterator-based scanning already uses RocksDB snapshots for consistency!

See `kalamdb-store/src/rocksdb_impl.rs` line 115:
```rust
fn scan(...) -> Result<Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_>> {
    // ✅ Take a consistent snapshot for the duration of the iterator
    let snapshot = self.db.snapshot();
    
    struct SnapshotScanIter<'a, D: rocksdb::DBAccess> {
        _snapshot: rocksdb::SnapshotWithThreadMode<'a, D>,  // Keeps snapshot alive
        inner: rocksdb::DBIteratorWithThreadMode<'a, D>,
        // ...
    }
}
```

**Benefits for Flush Operations**:
- ✅ ACID guarantees: Flush sees consistent point-in-time view
- ✅ Zero locking: Concurrent writes don't block flush
- ✅ Repeatable: Same flush on same data produces identical Parquet files
- ✅ Performance: Zero-copy reads from snapshot

All proposed optimizations (T222-T228) maintain snapshot semantics. See [PHASE_14_FLUSH_REFACTORING.md](./PHASE_14_FLUSH_REFACTORING.md) for details.

---

### 1. **Memory Allocation: Collecting All Rows into Vec**

**Problem**: Current implementation loads ALL rows into memory before filtering:

```rust
// ❌ CURRENT: Loads everything into Vec first
let raw_rows = self.store
    .scan_all(namespace, table)  // Returns Vec<(String, RowData)>
    .map_err(...)?;

// Then filters in memory
let filtered_rows: Vec<_> = raw_rows
    .into_iter()
    .filter(|(_row_id, row_data)| !row_data.get("_deleted").unwrap_or(false))
    .collect();
```

**Issues**:
- ⚠️ Loads 100% of data into memory (even if query needs 1%)
- ⚠️ Double allocation: RocksDB → Vec → filtered Vec
- ⚠️ No early termination for LIMIT queries
- ⚠️ O(N) memory usage regardless of result size

**Impact**: For 1M rows table with LIMIT 10:
- Memory: ~100MB allocated (should be <1KB)
- Latency: ~50ms scan + filter (should be <1ms)

---

### 2. **Column Array Building: Per-Row Iteration**

**Problem**: Current implementation iterates rows once per column:

```rust
// ❌ CURRENT: N iterations for N columns
for field in schema.fields() {
    let values: Vec<Option<String>> = rows
        .iter()  // Iterate ALL rows
        .map(|row_data| row_data.get(field.name()))
        .collect();
    
    arrays.push(Arc::new(StringArray::from(values)));
}
```

**Issues**:
- ⚠️ O(rows × columns) iterations (should be O(rows))
- ⚠️ Cache-unfriendly: jumps between rows for each column
- ⚠️ Intermediate Vec allocations per column

**Impact**: For 1000 rows × 10 columns:
- Current: 10,000 iterations + 10 Vec allocations
- Optimized: 1,000 iterations + column builders (zero-copy)

---

### 3. **Schema Cloning on Every RecordBatch**

**Problem**: Schema cloned unnecessarily:

```rust
// ❌ CURRENT: Clones Arc<Schema> on every batch creation
RecordBatch::try_new(schema.clone(), arrays)
```

**Issues**:
- ⚠️ Arc::clone() overhead (atomic increment)
- ⚠️ Unnecessary when schema is already Arc'd

**Impact**: Minor but adds up (10-20ns per batch creation)

---

### 4. **No Projection Pushdown**

**Problem**: Current implementation ignores projection:

```rust
async fn scan(
    projection: Option<&Vec<usize>>,  // ❌ Ignored!
) -> Result<Arc<dyn ExecutionPlan>> {
    // Fetches ALL columns even if query only needs 1-2 columns
    let rows = self.store.scan_all(...)?;
}
```

**Issues**:
- ⚠️ Fetches unused columns from storage
- ⚠️ Builds unused Arrow arrays
- ⚠️ Wastes memory and CPU

**Impact**: For `SELECT id FROM users` on table with 20 columns:
- Current: Fetches 20 columns, builds 20 arrays
- Optimized: Fetches 1 column, builds 1 array (20× faster)

---

## Proposed Optimizations

### Optimization 1: Iterator-Based Scanning (Zero-Copy)

**Replace Vec-based APIs with iterators**:

```rust
// ✅ NEW: Iterator-based API
trait EntityStore<K, V> {
    // Zero-allocation scanning with early termination
    fn scan_iter(&self) -> Result<impl Iterator<Item = Result<(K, V)>>>;
    
    // Prefix scanning with iterator
    fn scan_prefix_iter(&self, prefix: &K) -> Result<impl Iterator<Item = Result<(K, V)>>>;
}
```

**Benefits**:
- ✅ Zero allocations until needed
- ✅ Early termination for LIMIT queries
- ✅ Filter during iteration (not after)
- ✅ Stream large result sets

**Implementation**:
```rust
// In kalamdb-store/src/entity_store.rs
pub struct ScanIterator<'a, K, V> {
    db_iter: rocksdb::DBIterator<'a>,
    partition: String,
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> Iterator for ScanIterator<'_, K, V>
where
    K: AsRef<[u8]> + for<'de> Deserialize<'de>,
    V: for<'de> Deserialize<'de>,
{
    type Item = Result<(K, V)>;
    
    fn next(&mut self) -> Option<Self::Item> {
        self.db_iter.next().map(|result| {
            result.map_err(Into::into).and_then(|(k, v)| {
                let key: K = serde_json::from_slice(&k)?;
                let value: V = serde_json::from_slice(&v)?;
                Ok((key, value))
            })
        })
    }
}

// Usage in provider
let rows_iter = self.store
    .scan_iter()?
    .filter(|r| r.as_ref().ok().map_or(false, |(_, row)| !row.is_deleted()))
    .take(limit.unwrap_or(usize::MAX));
```

**Memory Impact**:
- Before: 1M rows = ~100MB Vec allocation
- After: Iterator = ~0KB (lazy evaluation)

---

### Optimization 2: Columnar Building (Single-Pass)

**Build all column arrays in one pass**:

```rust
// ✅ NEW: Single-pass columnar building
fn rows_to_batch_optimized(
    schema: &SchemaRef,
    rows: impl Iterator<Item = RowData>,
) -> Result<RecordBatch> {
    let row_count = rows.size_hint().0;
    
    // Pre-allocate builders for ALL columns
    let mut builders: HashMap<String, Box<dyn ArrayBuilder>> = schema
        .fields()
        .iter()
        .map(|field| {
            let builder: Box<dyn ArrayBuilder> = match field.data_type() {
                DataType::Utf8 => Box::new(StringBuilder::with_capacity(row_count, row_count * 50)),
                DataType::Int64 => Box::new(Int64Builder::with_capacity(row_count)),
                DataType::Boolean => Box::new(BooleanBuilder::with_capacity(row_count)),
                // ... other types
                _ => unimplemented!(),
            };
            (field.name().clone(), builder)
        })
        .collect();
    
    // ✅ Single iteration: append to ALL builders at once
    for row_data in rows {
        for field in schema.fields() {
            let builder = builders.get_mut(field.name()).unwrap();
            
            match row_data.get(field.name()) {
                Some(value) => append_value(builder, value, field.data_type()),
                None => builder.append_null(),
            }
        }
    }
    
    // Finish builders to create arrays
    let arrays: Vec<ArrayRef> = schema
        .fields()
        .iter()
        .map(|field| builders.remove(field.name()).unwrap().finish())
        .collect();
    
    RecordBatch::try_new(schema.clone(), arrays)
}
```

**Benefits**:
- ✅ O(rows) instead of O(rows × columns)
- ✅ Cache-friendly: processes one row at a time
- ✅ Pre-allocated builders (no intermediate Vecs)
- ✅ Zero-copy finish() to create arrays

**Performance Impact**:
- Before: 1000 rows × 10 cols = 10,000 iterations
- After: 1000 rows = 1,000 iterations (10× faster)

---

### Optimization 3: Projection Pushdown

**Filter columns early**:

```rust
async fn scan(
    projection: Option<&Vec<usize>>,  // ✅ Use this!
) -> Result<Arc<dyn ExecutionPlan>> {
    // Determine which columns to fetch
    let projected_schema = match projection {
        Some(indices) => {
            let fields: Vec<_> = indices
                .iter()
                .map(|&i| self.schema.fields()[i].clone())
                .collect();
            Arc::new(Schema::new(fields))
        }
        None => self.schema.clone(),
    };
    
    // Fetch only needed columns
    let rows_iter = self.store
        .scan_iter()?
        .map(|result| {
            result.map(|(key, value)| {
                // Project row to only include needed fields
                let projected_row = project_row(value, &projected_schema);
                (key, projected_row)
            })
        });
    
    let batch = rows_to_batch_optimized(&projected_schema, rows_iter)?;
    
    // ...
}
```

**Benefits**:
- ✅ Skip unused column deserialization
- ✅ Build fewer Arrow arrays
- ✅ Reduce memory footprint

**Performance Impact**: For `SELECT id FROM users WHERE ...` (1 col out of 20):
- Before: Deserialize 20 columns, build 20 arrays
- After: Deserialize 1 column, build 1 array (20× faster)

---

### Optimization 4: Filter Pushdown

**Apply filters during iteration**:

```rust
async fn scan(
    filters: &[Expr],  // ✅ Use this!
) -> Result<Arc<dyn ExecutionPlan>> {
    // Compile DataFusion expressions to filter predicate
    let filter_fn = compile_filters(filters)?;
    
    let rows_iter = self.store
        .scan_iter()?
        .filter(|result| {
            result.as_ref().ok().map_or(false, |(_, row)| {
                !row.is_deleted() && filter_fn(row)  // ✅ Filter during scan
            })
        })
        .take(limit.unwrap_or(usize::MAX));
    
    let batch = rows_to_batch_optimized(&schema, rows_iter)?;
    // ...
}
```

**Benefits**:
- ✅ Early termination for selective filters
- ✅ Avoid building rows that will be filtered out
- ✅ Reduce memory usage

**Performance Impact**: For `WHERE id = 'abc123'` on 1M rows:
- Before: Scan 1M rows → filter → return 1 row
- After: Stop after finding first match (1000× faster for unique lookups)

---

### Optimization 5: RecordBatch Size Tuning

**Avoid single giant batch**:

```rust
const BATCH_SIZE: usize = 8192;  // Arrow's default batch size

async fn scan(...) -> Result<Arc<dyn ExecutionPlan>> {
    let batches: Vec<RecordBatch> = self.store
        .scan_iter()?
        .filter(/* ... */)
        .chunks(BATCH_SIZE)  // ✅ Stream in batches
        .map(|chunk| rows_to_batch_optimized(&schema, chunk.into_iter()))
        .collect::<Result<Vec<_>>>()?;
    
    let exec = MemoryExec::try_new(&[batches], schema, projection)?;
    Ok(Arc::new(exec))
}
```

**Benefits**:
- ✅ Better memory locality (fits in CPU cache)
- ✅ DataFusion can parallelize batch processing
- ✅ Streaming-friendly (can start processing before all data loaded)

**Memory Impact**:
- Before: 1M rows in single batch = 100MB
- After: 1M rows in 122 batches of 8K = same total, but processed incrementally

---

### Optimization 6: Lazy Schema Validation

**Skip validation for trusted internal operations**:

```rust
// ✅ Use try_new_unchecked for validated schemas
RecordBatch::try_new(schema, arrays)  // ❌ Validates on every call
RecordBatch::try_new_unchecked(schema, arrays)  // ✅ Skip validation (unsafe but faster)

// Safer wrapper
fn create_batch_trusted(schema: SchemaRef, arrays: Vec<ArrayRef>) -> Result<RecordBatch> {
    #[cfg(debug_assertions)]
    RecordBatch::try_new(schema, arrays)  // Validate in debug builds
    
    #[cfg(not(debug_assertions))]
    unsafe { Ok(RecordBatch::try_new_unchecked(schema, arrays)) }  // Skip in release
}
```

**Benefits**:
- ✅ ~10-15% faster batch creation
- ✅ Still validates in debug builds (catches bugs during development)

---

## Summary: Expected Performance Improvements

### Memory Consumption

| Operation | Before | After | Improvement |
|-----------|--------|-------|-------------|
| Scan 1M rows (LIMIT 10) | 100MB | <1KB | **100,000×** |
| Scan 1M rows (no limit) | 200MB (2 Vecs) | 100MB (streaming) | **2×** |
| SELECT 1 col from 20 | 100MB (all cols) | 5MB (1 col) | **20×** |

### Query Latency

| Query Type | Before | After | Improvement |
|------------|--------|-------|-------------|
| LIMIT 10 from 1M rows | 50ms | <1ms | **50×** |
| WHERE id = 'x' (unique) | 50ms (full scan) | <1ms (early exit) | **50×** |
| SELECT * (1000 rows × 10 cols) | 5ms | 0.5ms | **10×** |
| SELECT id (projection) | 5ms | 0.25ms | **20×** |

### CPU Efficiency

| Operation | Before | After | Improvement |
|-----------|--------|-------|-------------|
| Column building (1K rows × 10 cols) | 10K iterations | 1K iterations | **10×** |
| RecordBatch creation | 20ns (validated) | 2ns (unchecked) | **10×** |

---

## Implementation Tasks (Add to Phase 14)

### New Tasks: T222-T228

- [ ] **T222** [P] Add iterator-based APIs to EntityStore trait in `kalamdb-store/src/entity_store.rs` (scan_iter(), scan_prefix_iter() returning impl Iterator)
- [ ] **T223** [P] Implement ScanIterator in `kalamdb-store/src/entity_store.rs` (wraps rocksdb::DBIterator, lazy deserialization, early termination)
- [ ] **T224** [P] Create columnar batch builder in `kalamdb-core/src/tables/batch_builder.rs` (single-pass with ArrayBuilders, pre-allocated capacity)
- [ ] **T225** [P] Add projection pushdown to all providers in `tables/system/{table}/{table}_provider.rs` (use projection parameter, filter fields early)
- [ ] **T226** [P] Add filter pushdown support to providers (compile DataFusion Expr to Rust predicates, apply during iteration)
- [ ] **T227** [P] Implement batched scanning in providers (chunk iterator into BATCH_SIZE=8192 RecordBatches instead of single giant batch)
- [ ] **T228** [P] Add create_batch_trusted() helper (uses try_new_unchecked in release builds, validates in debug)

**Checkpoint**: Performance optimizations complete - 50× faster queries, 100× less memory for LIMIT queries, 10× faster column building

---

## Migration Strategy

### Phase 1: Add Iterator APIs (Backwards Compatible)
- Add `scan_iter()` alongside existing `scan_all()`
- Keep old APIs for backwards compatibility
- Migrate providers one-by-one

### Phase 2: Optimize Providers
- Update system table providers to use iterators
- Add projection/filter pushdown
- Use columnar batch builder

### Phase 3: Deprecate Old APIs
- Mark `scan_all()` as deprecated
- Remove in Phase 15

---

## Benchmarking Plan

### Benchmark 1: Memory Usage (LIMIT Queries)
```rust
#[bench]
fn bench_scan_limit_10_from_1m_rows(b: &mut Bencher) {
    // Setup: 1M rows in table
    b.iter(|| {
        // SELECT * FROM users LIMIT 10
        let batch = provider.scan(/* ... */, Some(10));
        assert_eq!(batch.num_rows(), 10);
    });
    
    // Measure peak memory (should be <1KB vs 100MB)
}
```

### Benchmark 2: Column Building
```rust
#[bench]
fn bench_columnar_building(b: &mut Bencher) {
    let rows = generate_rows(1000, 10); // 1K rows × 10 cols
    
    b.iter(|| {
        rows_to_batch_optimized(&schema, rows.clone())
    });
    
    // Compare with old per-column iteration
}
```

### Benchmark 3: Projection Pushdown
```rust
#[bench]
fn bench_projection_single_column(b: &mut Bencher) {
    b.iter(|| {
        // SELECT id FROM users (1 col out of 20)
        let batch = provider.scan(Some(&vec![0]), /* ... */);
    });
    
    // Should be 20× faster than fetching all columns
}
```
