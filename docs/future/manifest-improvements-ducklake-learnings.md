# Manifest Improvements: DuckLake Architecture Learnings

> **Status**: Future Enhancement Proposal  
> **Created**: January 2026  
> **Reference**: [DuckLake Specification v0.3](https://ducklake.select/docs/stable/specification/introduction)

## Executive Summary

DuckLake is a lakehouse format built by the DuckDB team that uses **22 SQL tables** as its catalog/manifest system. After analyzing their specification, this document outlines key architectural patterns KalamDB should adopt to support robust schema evolution, efficient query pruning, and safe garbage collection.

---

## Table of Contents

1. [DuckLake Architecture Overview](#1-ducklake-architecture-overview)
2. [Stable Column IDs](#2-stable-column-ids)
3. [Dual Default Value System](#3-dual-default-value-system)
4. [Enhanced File Column Statistics](#4-enhanced-file-column-statistics)
5. [Delete File Tracking (Merge-on-Read)](#5-delete-file-tracking-merge-on-read)
6. [Garbage Collection Queue](#6-garbage-collection-queue)
7. [Table-Level Statistics Cache](#7-table-level-statistics-cache)
8. [Snapshot Change Tracking](#8-snapshot-change-tracking)
9. [Column Mapping for External Files](#9-column-mapping-for-external-files)
10. [Implementation Priority](#10-implementation-priority)
11. [Migration Strategy](#11-migration-strategy)

---

## 1. DuckLake Architecture Overview

### Core Metadata Tables

DuckLake separates concerns into specialized tables:

| Table | Purpose | KalamDB Equivalent |
|-------|---------|-------------------|
| `ducklake_snapshot` | Global version tracking | `Manifest.version` |
| `ducklake_table` | Table metadata | `TableDefinition` |
| `ducklake_column` | Column definitions with versioning | `ColumnDefinition` |
| `ducklake_data_file` | Parquet file metadata | `SegmentMetadata` |
| `ducklake_delete_file` | Deletion markers per data file | `_deleted` column (inline) |
| `ducklake_file_column_stats` | Per-file, per-column statistics | `SegmentMetadata.column_stats` |
| `ducklake_table_stats` | Aggregate table statistics | Not implemented |
| `ducklake_files_scheduled_for_deletion` | GC queue | Not implemented |

### Key Design Principles

1. **Temporal Validity**: Every metadata entity has `begin_snapshot`/`end_snapshot` for time travel
2. **Stable Identifiers**: `column_id`, `table_id`, `data_file_id` are permanent, never reused
3. **Separation of Data and Deletes**: Delete files are separate from data files (merge-on-read)
4. **Statistics at Multiple Levels**: Table-level, column-level, and file-column-level stats

---

## 2. Stable Column IDs

### The Problem

KalamDB currently identifies columns by `column_name` in `SegmentMetadata.column_stats`:

```rust
// Current: Column stats keyed by name
pub column_stats: HashMap<String, ColumnStats>,
```

This breaks when:
- Column is renamed: `ALTER TABLE users RENAME email TO email_address`
- Stats from old segments become orphaned under old key
- Read path can't correlate old data with renamed column

### DuckLake's Solution

Every column gets a permanent `column_id` that:
- Is written to Parquet files as the `field_id` metadata field
- Never changes across renames, type promotions, or reordering
- Is used as the key for all column-related lookups

```sql
-- DuckLake's ducklake_column table
CREATE TABLE ducklake_column (
    column_id BIGINT,           -- Permanent identifier
    begin_snapshot BIGINT,      -- Valid from this snapshot
    end_snapshot BIGINT,        -- Valid until this snapshot (NULL = current)
    table_id BIGINT,
    column_order BIGINT,        -- Display order (can change)
    column_name VARCHAR,        -- Name (can change via rename)
    column_type VARCHAR,        -- Type (can change via promotion)
    initial_default VARCHAR,    -- Default for backfilling old files
    default_value VARCHAR,      -- Default for new INSERTs
    nulls_allowed BOOLEAN,
    parent_column BIGINT        -- For nested types (STRUCT fields)
);
```

### Proposed KalamDB Changes

#### ColumnDefinition Enhancement

```rust
// File: backend/crates/kalamdb-commons/src/models/schemas/column_definition.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// Stable column identifier - NEVER changes after creation
    /// 
    /// This ID is written to Parquet files as the `field_id` in the schema.
    /// When reading, we match Parquet field_id to column_id, not column_name.
    /// This enables:
    /// - Column renames without rewriting data
    /// - Type promotions (e.g., INT32 → INT64) with correct mapping
    /// - Detecting which columns exist in old files vs current schema
    pub column_id: u64,
    
    /// Column name (case-insensitive, stored as lowercase)
    /// Can change via ALTER TABLE RENAME COLUMN
    pub column_name: String,
    
    /// Display order in SELECT * (1-indexed)
    /// Can change via ALTER TABLE REORDER COLUMNS
    pub ordinal_position: u32,
    
    /// Data type - can change via type promotion only
    pub data_type: KalamDataType,
    
    /// Default for backfilling old segments that don't have this column
    /// Set when column is added via ALTER TABLE ADD COLUMN
    /// 
    /// Example: ALTER TABLE users ADD COLUMN age INT DEFAULT 0
    /// - initial_default = Some(Value::Number(0))
    /// - When reading segment written before this ALTER, fill with 0
    pub initial_default: Option<serde_json::Value>,
    
    /// Default for new INSERT operations
    /// Can be expression like NOW() or literal
    pub default_value: ColumnDefault,
    
    /// Whether column can contain NULL values
    pub is_nullable: bool,
    
    /// Whether this column is part of the primary key
    pub is_primary_key: bool,
    
    /// Whether this column is part of the partition key
    pub is_partition_key: bool,
    
    /// Optional column comment/description
    pub column_comment: Option<String>,
}
```

#### TableDefinition Enhancement

```rust
// File: backend/crates/kalamdb-commons/src/models/schemas/table_definition.rs

#[derive(Debug, Clone, PartialEq)]
pub struct TableDefinition {
    // ... existing fields ...
    
    /// Next available column_id for new columns
    /// 
    /// Monotonically increasing. When adding a column:
    /// 1. Assign next_column_id to new column
    /// 2. Increment next_column_id
    /// 
    /// This ensures column_ids are never reused, even after DROP COLUMN.
    pub next_column_id: u64,
    
    /// Historical column definitions (for schema evolution)
    /// 
    /// Maps column_id → list of (schema_version, ColumnDefinition)
    /// Allows reconstructing schema at any point in time.
    /// 
    /// Example after: CREATE TABLE → ADD COLUMN → RENAME → DROP
    /// column_id=1: [(v1, "id INT"), (v1, current)]
    /// column_id=2: [(v2, "name VARCHAR"), (v3, "full_name VARCHAR"), (v4, dropped)]
    #[serde(default)]
    pub column_history: HashMap<u64, Vec<(u32, ColumnDefinition)>>,
}
```

#### SegmentMetadata Enhancement

```rust
// File: backend/crates/kalamdb-commons/src/models/types/manifest.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMetadata {
    // ... existing fields ...
    
    /// Column statistics keyed by column_id (not name!)
    /// 
    /// Using column_id as key ensures stats survive renames.
    /// When reading: lookup current column_id → find stats → apply to column
    #[serde(default)]
    pub column_stats: HashMap<u64, FileColumnStats>,
    
    /// List of column_ids present in this Parquet file
    /// 
    /// Enables fast schema compatibility check without reading Parquet metadata.
    /// If current schema has column_id=5 but it's not in columns_present,
    /// we know to use initial_default for that column.
    #[serde(default)]
    pub columns_present: Vec<u64>,
    
    /// Parquet footer byte offset from end of file
    /// 
    /// Parquet footer contains schema + row group metadata.
    /// Caching this avoids:
    /// 1. Full file read to find footer
    /// 2. Seeking to end of file to read footer length
    /// 
    /// Read pattern: seek(file_size - footer_size), read(footer_size)
    pub footer_size: Option<u64>,
}
```

### Read Path Changes

When reading a segment with schema evolution:

```rust
/// Reconstruct table data from segment with schema evolution
fn read_segment_with_evolution(
    segment: &SegmentMetadata,
    current_schema: &TableDefinition,
    table_versions: &TablesStore,
) -> Result<RecordBatch, Error> {
    // 1. Get schema when segment was written
    let segment_schema = table_versions.get_version(
        &current_schema.table_id(),
        segment.schema_version
    )?;
    
    // 2. Build column mapping: current column_id → segment column_id
    let mut column_mapping = HashMap::new();
    for current_col in &current_schema.columns {
        if segment.columns_present.contains(&current_col.column_id) {
            // Column exists in file - read from Parquet using field_id
            column_mapping.insert(current_col.column_id, ColumnSource::FromFile);
        } else {
            // Column doesn't exist in file - use initial_default
            column_mapping.insert(
                current_col.column_id,
                ColumnSource::Default(current_col.initial_default.clone())
            );
        }
    }
    
    // 3. Read Parquet with field_id projection
    let reader = ParquetReader::open(&segment.path)?
        .with_field_id_projection(column_mapping.keys().cloned().collect());
    
    // 4. Apply type promotions if needed
    let batch = reader.read_batch()?;
    let promoted = apply_type_promotions(batch, &segment_schema, current_schema)?;
    
    // 5. Fill missing columns with defaults
    fill_default_columns(promoted, &column_mapping)
}

enum ColumnSource {
    FromFile,
    Default(Option<serde_json::Value>),
}
```

### Write Path Changes

When writing Parquet files, include `field_id`:

```rust
/// Write RecordBatch to Parquet with field_id metadata
fn write_parquet_with_field_ids(
    batch: &RecordBatch,
    table_def: &TableDefinition,
    path: &Path,
) -> Result<SegmentMetadata, Error> {
    // Build Arrow schema with field_id in metadata
    let fields: Vec<Field> = table_def.columns.iter().map(|col| {
        let arrow_type = col.data_type.to_arrow_type();
        Field::new(&col.column_name, arrow_type, col.is_nullable)
            .with_metadata(HashMap::from([
                ("PARQUET:field_id".to_string(), col.column_id.to_string())
            ]))
    }).collect();
    
    let schema = Arc::new(Schema::new(fields));
    
    // Write with schema
    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, schema, None)?;
    writer.write(batch)?;
    let metadata = writer.close()?;
    
    // Capture columns_present
    let columns_present: Vec<u64> = table_def.columns
        .iter()
        .map(|c| c.column_id)
        .collect();
    
    Ok(SegmentMetadata {
        columns_present,
        footer_size: Some(metadata.footer_size()),
        // ... other fields
    })
}
```

### DuckLake Gotcha to Avoid

DuckLake stores `column_id` in the `ducklake_column` table but relies on Parquet's `field_id` being set correctly. If a file is written without `field_id` (e.g., imported from external source), they need a fallback mapping table (`ducklake_column_mapping`).

**KalamDB approach**: Always write `field_id` to Parquet. For imported files, either:
1. Reject if no `field_id` present
2. Create mapping entry based on position or name

---

## 3. Dual Default Value System

### The Problem

KalamDB has a single `default_value` field:

```rust
pub default_value: ColumnDefault,
```

This conflates two distinct use cases:
1. **Backfilling**: What value to use when reading old segments that don't have this column
2. **Insertion**: What value to use when INSERT doesn't specify this column

### DuckLake's Solution

Two separate default values:

| Field | Purpose | When Used |
|-------|---------|-----------|
| `initial_default` | Backfill value for old data | READ path, when column missing from file |
| `default_value` | Operational default | WRITE path, when INSERT omits column |

### Why This Matters

Consider:

```sql
-- Day 1: Create table
CREATE TABLE events (id INT PRIMARY KEY, name TEXT);

-- Day 2: Add column with default
ALTER TABLE events ADD COLUMN priority INT DEFAULT 5;

-- Day 3: Change default for new inserts
ALTER TABLE events ALTER COLUMN priority SET DEFAULT 10;
```

With dual defaults:
- `initial_default = 5` (frozen at ALTER TABLE ADD COLUMN time)
- `default_value = 10` (current INSERT default)

Reading day 1 data returns `priority = 5` (initial_default).
New inserts get `priority = 10` (default_value).

With single default (current KalamDB), you'd have to choose:
- Use 10 everywhere → day 1 data shows wrong value
- Use 5 everywhere → new inserts get outdated default

### Proposed Implementation

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// Default value for backfilling old segments
    /// 
    /// Set when column is added via ALTER TABLE ADD COLUMN.
    /// Frozen at that point - never changes afterwards.
    /// 
    /// Can be:
    /// - None: Column existed since table creation (no backfill needed)
    /// - Some(null): Backfill with NULL
    /// - Some(value): Backfill with this literal value
    /// 
    /// Note: Cannot be an expression - must be a literal value that
    /// can be evaluated once and stored.
    pub initial_default: Option<serde_json::Value>,
    
    /// Default value for INSERT operations
    /// 
    /// Can be:
    /// - None: No default, column required in INSERT
    /// - Literal(value): Use this value
    /// - Expression(expr): Evaluate at insert time (e.g., NOW(), UUID())
    /// - AutoIncrement: Generate next sequence value
    pub default_value: ColumnDefault,
}

impl ColumnDefinition {
    /// Create a new column for ALTER TABLE ADD COLUMN
    pub fn new_added_column(
        column_id: u64,
        name: String,
        data_type: KalamDataType,
        default: Option<serde_json::Value>,
    ) -> Self {
        Self {
            column_id,
            column_name: name,
            initial_default: default.clone(),  // Frozen at add time
            default_value: default
                .map(ColumnDefault::Literal)
                .unwrap_or(ColumnDefault::None),
            // ... other fields
        }
    }
}
```

---

## 4. Enhanced File Column Statistics

### Current State

```rust
pub struct ColumnStats {
    pub min: Option<serde_json::Value>,
    pub max: Option<serde_json::Value>,
    pub null_count: Option<i64>,
}
```

### DuckLake's Richer Statistics

```sql
CREATE TABLE ducklake_file_column_stats (
    data_file_id BIGINT,
    table_id BIGINT,
    column_id BIGINT,           -- Uses stable column_id
    column_size_bytes BIGINT,   -- Size of this column in file
    value_count BIGINT,         -- Count of values (may differ from row_count for arrays)
    null_count BIGINT,
    min_value VARCHAR,          -- String-encoded for storage
    max_value VARCHAR,
    contains_nan BOOLEAN,       -- Critical for float comparisons
    extra_stats VARCHAR         -- Extensible JSON for type-specific stats
);
```

### Proposed Enhancement

```rust
/// Statistics for a single column within a segment file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileColumnStats {
    /// Minimum value in the column (serialized as JSON value)
    /// 
    /// For query pruning: if query has `WHERE col > X` and X > max,
    /// this file can be skipped entirely.
    /// 
    /// Note: Stored as lower bound - doesn't need to be exact min.
    /// Parquet row group stats often give us this for free.
    pub min_value: Option<serde_json::Value>,
    
    /// Maximum value in the column (serialized as JSON value)
    /// 
    /// Upper bound for range pruning.
    pub max_value: Option<serde_json::Value>,
    
    /// Number of NULL values in this column
    pub null_count: i64,
    
    /// Total number of values in this column
    /// 
    /// For scalar columns: equals row_count
    /// For array/list columns: sum of array lengths
    /// For struct columns: equals row_count
    /// 
    /// Useful for cardinality estimation in query planning.
    pub value_count: i64,
    
    /// Size of this column's data in bytes within the file
    /// 
    /// Enables I/O cost estimation for column projection.
    /// If query only needs col1 (1MB) and col2 (100MB), knowing
    /// col1 is small helps query planner.
    pub column_size_bytes: Option<u64>,
    
    /// Whether column contains any NaN values (float types only)
    /// 
    /// Critical for correct float comparisons:
    /// - NaN != NaN (by IEEE 754)
    /// - NaN comparisons need special handling
    /// - If contains_nan = false, can use faster comparison paths
    pub contains_nan: Option<bool>,
    
    /// Distinct value count estimate (optional)
    /// 
    /// Useful for join ordering and aggregation planning.
    /// Can be approximate (HyperLogLog estimate).
    pub distinct_count: Option<u64>,
}

impl FileColumnStats {
    /// Check if a predicate can prune this file
    pub fn can_prune(&self, predicate: &Predicate) -> bool {
        match predicate {
            Predicate::Eq(value) => {
                // If value is outside [min, max], file can be skipped
                self.min_value.as_ref().map_or(false, |min| value < min) ||
                self.max_value.as_ref().map_or(false, |max| value > max)
            }
            Predicate::Gt(value) => {
                // If max < value, no rows can match
                self.max_value.as_ref().map_or(false, |max| max <= value)
            }
            Predicate::Lt(value) => {
                // If min > value, no rows can match
                self.min_value.as_ref().map_or(false, |min| min >= value)
            }
            Predicate::IsNull => {
                // If null_count = 0, no NULLs to find
                self.null_count == 0
            }
            Predicate::IsNotNull => {
                // If all values are NULL, no non-NULLs to find
                self.null_count as u64 == self.value_count as u64
            }
        }
    }
}
```

### Why `contains_nan` Matters

```rust
// Without contains_nan knowledge:
fn compare_floats_slow(file_max: f64, query_value: f64) -> bool {
    // Must handle NaN case even if file has no NaNs
    if file_max.is_nan() || query_value.is_nan() {
        return false;  // Can't prune - NaN comparison is always false
    }
    file_max < query_value
}

// With contains_nan = false:
fn compare_floats_fast(file_max: f64, query_value: f64) -> bool {
    // No NaN check needed - we know file has none
    file_max < query_value
}
```

---

## 5. Delete File Tracking (Merge-on-Read)

### Current Approach

KalamDB uses a `_deleted` system column:
- Each row has `_deleted: bool`
- DELETE sets `_deleted = true`
- Compaction rewrites segment without deleted rows

### DuckLake's Approach

Separate delete files:

```sql
CREATE TABLE ducklake_delete_file (
    delete_file_id BIGINT PRIMARY KEY,
    table_id BIGINT,
    begin_snapshot BIGINT,      -- When delete was created
    end_snapshot BIGINT,        -- NULL = active, non-NULL = compacted away
    data_file_id BIGINT,        -- Which data file these deletes apply to
    path VARCHAR,
    format VARCHAR,             -- 'parquet' (contains row indices)
    delete_count BIGINT,
    file_size_bytes BIGINT,
    footer_size BIGINT
);
```

Delete file format (Parquet):
```
row_index: INT64  -- Index of deleted row in data file
```

### Trade-offs

| Aspect | Inline `_deleted` | Separate Delete Files |
|--------|-------------------|----------------------|
| Write cost | Full row update | Small file write |
| Read cost | Filter on read | Join data + delete files |
| Compaction | Rewrite segment | Merge data + deletes |
| Space overhead | 1 bool per row | 1 file per delete batch |
| Concurrency | Row-level conflict | File-level conflict |

### Recommendation for KalamDB

Keep `_deleted` column for now, but consider hybrid approach:

```rust
/// Segment can have inline deletes OR external delete files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMetadata {
    // ... existing fields ...
    
    /// If true, this segment uses inline _deleted column
    /// If false, check delete_files for external delete markers
    #[serde(default = "default_true")]
    pub uses_inline_deletes: bool,
}

/// External delete file metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteFileMetadata {
    /// Unique identifier
    pub delete_file_id: String,
    
    /// Which data segment this applies to
    pub data_segment_id: String,
    
    /// Path to delete file (Parquet with row indices)
    pub path: String,
    
    /// Number of deleted rows in this file
    pub delete_count: u64,
    
    /// File size in bytes
    pub size_bytes: u64,
    
    /// Creation timestamp
    pub created_at: i64,
    
    /// Schema version when delete was created
    /// (needed to correctly interpret row indices)
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    // ... existing fields ...
    
    /// External delete files (merge-on-read pattern)
    /// 
    /// Only used when segments have uses_inline_deletes = false.
    /// Each delete file contains row indices to skip when reading
    /// the corresponding data segment.
    #[serde(default)]
    pub delete_files: Vec<DeleteFileMetadata>,
}
```

### When to Use External Deletes

- High delete volume: Many small deletes that would cause segment rewrites
- Time-series data: Delete old data in bulk (entire date ranges)
- GDPR compliance: Need to quickly mark rows as deleted, compact later

---

## 6. Garbage Collection Queue

### The Problem

When files are no longer needed (compaction, table drop, snapshot expiry), they shouldn't be deleted immediately:
1. Concurrent readers may still be using them
2. Time travel queries may need old snapshots
3. Crash during delete could leave inconsistent state

### DuckLake's Solution

```sql
CREATE TABLE ducklake_files_scheduled_for_deletion (
    data_file_id BIGINT,
    path VARCHAR,
    path_is_relative BOOLEAN,
    schedule_start TIMESTAMPTZ  -- When file became eligible for deletion
);
```

Deletion process:
1. File becomes unused → add to `files_scheduled_for_deletion`
2. Wait for retention period (e.g., 7 days)
3. Background job deletes files where `schedule_start + retention < now()`

### Proposed Implementation

```rust
/// File scheduled for garbage collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDeletion {
    /// Path to the file
    pub path: String,
    
    /// When file became eligible for deletion
    pub scheduled_at: i64,
    
    /// Why file is being deleted
    pub reason: DeletionReason,
    
    /// Original segment ID (for debugging/audit)
    pub original_segment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeletionReason {
    /// Segment was compacted into larger file
    Compacted { into_segment_id: String },
    
    /// Table was dropped
    TableDropped,
    
    /// Snapshot containing this file was expired
    SnapshotExpired { snapshot_version: u64 },
    
    /// Delete file was merged during compaction
    DeleteFileMerged,
    
    /// File was orphaned (no manifest reference found)
    Orphaned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    // ... existing fields ...
    
    /// Files pending deletion
    /// 
    /// GC job periodically scans this list and deletes files
    /// where scheduled_at + retention_period < now().
    /// 
    /// This provides:
    /// 1. Time travel: old snapshots can still read files
    /// 2. Crash safety: delete is two-phase (schedule, then delete)
    /// 3. Audit trail: know why files were deleted
    #[serde(default)]
    pub pending_deletions: Vec<PendingDeletion>,
}
```

### GC Job Implementation

```rust
/// Garbage collection executor
impl GarbageCollectionExecutor {
    pub async fn run(&self, table_id: &TableId) -> Result<GCStats, Error> {
        let manifest = self.manifest_service.get_manifest(table_id)?;
        let now = chrono::Utc::now().timestamp();
        let retention = self.config.file_retention_seconds; // e.g., 7 days
        
        let mut deleted_count = 0;
        let mut deleted_bytes = 0;
        let mut remaining = Vec::new();
        
        for pending in &manifest.pending_deletions {
            if pending.scheduled_at + retention < now {
                // Safe to delete
                match self.storage.delete(&pending.path).await {
                    Ok(size) => {
                        deleted_count += 1;
                        deleted_bytes += size;
                        log::info!(
                            "Deleted file: {} (reason: {:?})",
                            pending.path, pending.reason
                        );
                    }
                    Err(e) => {
                        log::warn!("Failed to delete {}: {}", pending.path, e);
                        remaining.push(pending.clone());
                    }
                }
            } else {
                // Not yet eligible
                remaining.push(pending.clone());
            }
        }
        
        // Update manifest with remaining pending deletions
        let mut updated_manifest = manifest.clone();
        updated_manifest.pending_deletions = remaining;
        self.manifest_service.save_manifest(&updated_manifest)?;
        
        Ok(GCStats { deleted_count, deleted_bytes })
    }
}
```

---

## 7. Table-Level Statistics Cache

### The Problem

Common queries like `SELECT COUNT(*) FROM table` currently require:
1. Load manifest
2. Sum `row_count` across all segments
3. Subtract deleted rows (requires reading delete markers)

### DuckLake's Solution

```sql
CREATE TABLE ducklake_table_stats (
    table_id BIGINT,
    record_count BIGINT,    -- Approximate total rows
    next_row_id BIGINT,     -- For row lineage tracking
    file_size_bytes BIGINT  -- Total data size
);
```

Stats are updated incrementally:
- INSERT: `record_count += inserted_rows`
- DELETE: Stats not updated (upper bound is fine)
- COMPACT: Recalculate accurate count

### Proposed Implementation

```rust
/// Cached aggregate statistics for a table
/// 
/// Updated incrementally on writes, periodically reconciled.
/// All values are upper bounds - deletes don't decrement.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TableStats {
    /// Total row count across all segments (upper bound)
    /// 
    /// This is incremented on INSERT but not decremented on DELETE.
    /// For exact count, scan segments and subtract deletes.
    /// 
    /// Use case: Query planner cardinality estimation
    pub total_row_count: u64,
    
    /// Next available row ID for row lineage tracking
    /// 
    /// Each row gets a unique, monotonically increasing ID.
    /// Useful for:
    /// - CDC (Change Data Capture): "give me rows after ID X"
    /// - Debugging: correlate log entries with specific rows
    pub next_row_id: u64,
    
    /// Total size of all data files in bytes
    pub total_file_size_bytes: u64,
    
    /// Number of segment files
    pub segment_count: u32,
    
    /// Number of active delete files (if using merge-on-read)
    pub delete_file_count: u32,
    
    /// Estimated deleted row count (upper bound)
    /// 
    /// Incremented on DELETE, decremented on compaction.
    pub estimated_deleted_rows: u64,
    
    /// Last time stats were fully reconciled with actual data
    pub last_reconciled_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    // ... existing fields ...
    
    /// Cached table-level statistics
    /// 
    /// Avoid O(n) segment scan for basic queries.
    #[serde(default)]
    pub table_stats: TableStats,
}

impl Manifest {
    /// Update stats after INSERT
    pub fn record_insert(&mut self, row_count: u64, file_size: u64) {
        self.table_stats.total_row_count += row_count;
        self.table_stats.next_row_id += row_count;
        self.table_stats.total_file_size_bytes += file_size;
        self.table_stats.segment_count += 1;
    }
    
    /// Update stats after DELETE
    pub fn record_delete(&mut self, deleted_count: u64) {
        // Don't decrement total_row_count - it's an upper bound
        self.table_stats.estimated_deleted_rows += deleted_count;
    }
    
    /// Recalculate stats from segments (expensive, do periodically)
    pub fn reconcile_stats(&mut self) {
        let mut total_rows = 0u64;
        let mut total_bytes = 0u64;
        
        for segment in &self.segments {
            if !segment.tombstone {
                total_rows += segment.row_count;
                total_bytes += segment.size_bytes;
            }
        }
        
        self.table_stats.total_row_count = total_rows;
        self.table_stats.total_file_size_bytes = total_bytes;
        self.table_stats.segment_count = self.segments.len() as u32;
        self.table_stats.last_reconciled_at = Some(chrono::Utc::now().timestamp());
    }
}
```

---

## 8. Snapshot Change Tracking

### DuckLake's Approach

```sql
CREATE TABLE ducklake_snapshot_changes (
    snapshot_id BIGINT PRIMARY KEY,
    changes_made VARCHAR,       -- CSV of change types
    author VARCHAR,             -- Who made the change
    commit_message VARCHAR,     -- Git-style message
    commit_extra_info VARCHAR   -- Arbitrary metadata
);
```

Change types:
- `created_table:TABLE_NAME`
- `inserted_into_table:TABLE_ID`
- `deleted_from_table:TABLE_ID`
- `altered_table:TABLE_ID`
- `compacted_table:TABLE_ID`
- `dropped_table:TABLE_ID`

### Benefits

1. **Conflict Detection**: Two concurrent snapshots modifying same table can be detected
2. **Audit Trail**: Know what changed and when
3. **Debugging**: Correlate issues with specific changes
4. **CDC (Change Data Capture)**: Stream changes to downstream systems

### Proposed Implementation

```rust
/// Record of changes made in a manifest version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestChange {
    /// Type of change
    pub change_type: ChangeType,
    
    /// Timestamp when change occurred
    pub timestamp: i64,
    
    /// User who made the change (if authenticated)
    pub author: Option<String>,
    
    /// Optional commit message
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    /// Rows were inserted
    Insert { row_count: u64, segment_id: String },
    
    /// Rows were deleted
    Delete { row_count: u64 },
    
    /// Schema was altered
    SchemaChange { 
        from_version: u32, 
        to_version: u32,
        description: String,  // e.g., "ADD COLUMN age INT"
    },
    
    /// Segments were compacted
    Compaction { 
        merged_segment_ids: Vec<String>,
        result_segment_id: String,
    },
    
    /// Manifest was created (first write to table)
    Created,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    // ... existing fields ...
    
    /// History of changes to this manifest
    /// 
    /// Newest changes first. Can be truncated to limit size.
    /// Full history available via audit logs.
    #[serde(default)]
    pub recent_changes: Vec<ManifestChange>,
}
```

---

## 9. Column Mapping for External Files

### DuckLake's Problem

When ingesting external Parquet files that don't have `field_id` set:

```sql
CREATE TABLE ducklake_column_mapping (
    mapping_id BIGINT,
    table_id BIGINT,
    type VARCHAR  -- 'map_by_name' or 'map_by_position'
);

CREATE TABLE ducklake_name_mapping (
    mapping_id BIGINT,
    column_id BIGINT,
    source_name VARCHAR,      -- Column name in external file
    target_field_id BIGINT,   -- Our stable column_id
    parent_column BIGINT,
    is_partition BOOLEAN
);
```

### KalamDB Approach

For MVP, reject files without `field_id`. For external file import:

```rust
/// Strategy for mapping external Parquet columns to table schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColumnMappingStrategy {
    /// Require field_id in Parquet schema (default, safest)
    RequireFieldId,
    
    /// Map by column name (case-insensitive)
    /// Fails if names don't match
    MapByName,
    
    /// Map by position (fragile, not recommended)
    /// Assumes external file has same column order
    MapByPosition,
    
    /// Explicit mapping from external name to column_id
    Explicit(HashMap<String, u64>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMetadata {
    // ... existing fields ...
    
    /// How to map columns if Parquet lacks field_id
    /// 
    /// None = file has field_id (normal case)
    /// Some(strategy) = apply mapping when reading
    #[serde(default)]
    pub column_mapping: Option<ColumnMappingStrategy>,
}
```

---

## 10. Implementation Priority

### Phase 1: Foundation (High Impact, Low Risk)

| Change | Effort | Impact | Files to Modify |
|--------|--------|--------|-----------------|
| Add `footer_size` to SegmentMetadata | Low | Medium | `manifest.rs` |
| Add `TableStats` to Manifest | Low | High | `manifest.rs` |
| Add `contains_nan` to ColumnStats | Low | Low | `manifest.rs` |
| Add `value_count` to ColumnStats | Low | Medium | `manifest.rs` |

### Phase 2: Schema Evolution (High Impact, Medium Risk)

| Change | Effort | Impact | Files to Modify |
|--------|--------|--------|-----------------|
| Add `column_id` to ColumnDefinition | Medium | Critical | `column_definition.rs`, `table_definition.rs` |
| Add `initial_default` to ColumnDefinition | Medium | High | `column_definition.rs`, DDL handlers |
| Change `column_stats` key to `column_id` | Medium | High | `manifest.rs`, flush service, read path |
| Add `columns_present` to SegmentMetadata | Low | High | `manifest.rs`, write path |
| Write `field_id` to Parquet | Medium | Critical | Parquet writer |

### Phase 3: Operations (Medium Impact, Low Risk)

| Change | Effort | Impact | Files to Modify |
|--------|--------|--------|-----------------|
| Add `PendingDeletion` tracking | Low | Medium | `manifest.rs`, GC job |
| Add `ManifestChange` history | Low | Medium | `manifest.rs`, all write operations |
| Add `column_size_bytes` to stats | Low | Low | `manifest.rs`, Parquet metadata extraction |

### Phase 4: Advanced (Medium Impact, Higher Risk)

| Change | Effort | Impact | Files to Modify |
|--------|--------|--------|-----------------|
| External delete files | High | Medium | New `delete_file.rs`, read path changes |
| Column mapping for imports | Medium | Low | `manifest.rs`, import handler |

---

## 11. Migration Strategy

### Backward Compatibility

New manifest fields should use `#[serde(default)]` to allow reading old manifests:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMetadata {
    // Existing fields work as-is
    pub id: String,
    pub path: String,
    
    // New fields have defaults for old manifests
    #[serde(default)]
    pub footer_size: Option<u64>,  // None for old segments
    
    #[serde(default)]
    pub columns_present: Vec<u64>,  // Empty for old segments
}
```

### Column ID Migration

For existing tables without `column_id`:

```rust
impl TableDefinition {
    /// Assign column_ids to existing columns (migration)
    pub fn migrate_add_column_ids(&mut self) {
        if self.columns.iter().any(|c| c.column_id == 0) {
            // Assign sequential IDs based on ordinal_position
            for col in &mut self.columns {
                if col.column_id == 0 {
                    col.column_id = col.ordinal_position as u64;
                }
            }
            self.next_column_id = self.columns.len() as u64 + 1;
        }
    }
}
```

### Stats Key Migration

For `column_stats` key change (name → column_id):

```rust
impl SegmentMetadata {
    /// Migrate column_stats from name-keyed to id-keyed
    pub fn migrate_stats_to_column_ids(
        &mut self,
        table_def: &TableDefinition,
    ) {
        let name_to_id: HashMap<&str, u64> = table_def.columns
            .iter()
            .map(|c| (c.column_name.as_str(), c.column_id))
            .collect();
        
        let old_stats = std::mem::take(&mut self.column_stats);
        for (name, stats) in old_stats {
            if let Some(&id) = name_to_id.get(name.as_str()) {
                self.column_stats.insert(id, stats);
            }
        }
    }
}
```

---

## References

- [DuckLake Specification v0.3](https://ducklake.select/docs/stable/specification/introduction)
- [DuckLake Schema Evolution](https://ducklake.select/docs/stable/duckdb/usage/schema_evolution)
- [DuckLake Maintenance](https://ducklake.select/docs/stable/duckdb/maintenance/recommended_maintenance)
- [Apache Parquet field_id](https://github.com/apache/parquet-format/blob/master/src/main/thrift/parquet.thrift#L550)
- [Apache Iceberg Schema Evolution](https://iceberg.apache.org/spec/#schema-evolution) (similar approach)
