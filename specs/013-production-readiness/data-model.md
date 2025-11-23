# Data Model: Production Readiness

**Feature**: `013-production-readiness`
**Date**: 2025-11-23

## 1. Manifest Schema

**File**: `backend/crates/kalamdb-commons/src/models/manifest.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub table_id: TableId,
    pub version: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub segments: Vec<SegmentMetadata>,
    pub last_sequence_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMetadata {
    pub id: String, // UUID
    pub path: String, // Relative to table root
    pub min_key: Option<Vec<u8>>,
    pub max_key: Option<Vec<u8>>,
    pub row_count: u64,
    pub size_bytes: u64,
    pub created_at: i64,
    pub tombstone: bool, // If true, this segment is marked for deletion
}
```

## 2. Table Definition Updates

**File**: `backend/crates/kalamdb-commons/src/models/schemas/table.rs`

```rust
// Existing struct, ensuring options are flexible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    // ... existing fields
    pub options: HashMap<String, String>, // For arbitrary metadata
}
```

## 3. System Table Schemas

**File**: `backend/crates/kalamdb-core/src/tables/system/tables.rs` (Virtual Schema)

```rust
// Schema for system.tables
// - table_name: Utf8
// - namespace: Utf8
// - type: Utf8
// - created_at: Timestamp
// - options: Utf8 (JSON string)
```

**File**: `backend/crates/kalamdb-core/src/tables/system/live_queries.rs` (Virtual Schema)

```rust
// Schema for system.live_queries
// - query_id: Utf8
// - session_id: Utf8
// - sql: Utf8
// - start_time: Timestamp
// - duration_ms: UInt64
// - state: Utf8 (Running, Queued)
```

## 4. Storage Keys

**RocksDB Column Families**:
- `default`: Data
- `manifests`: `TableId` -> `Manifest` (Serialized Bincode/JSON)

**Filesystem Layout**:
```
/data/storage/
  {namespace_id}/
    {table_name}/
      manifest.json
      data/
        segment_001.parquet
        segment_002.parquet
```
