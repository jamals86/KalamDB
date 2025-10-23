# ADR-006: Flush Execution with Streaming Write and Per-User Isolation

**Status**: Accepted  
**Date**: 2025-10-22  
**Related**: ADR-001 (Table-per-User), ADR-005 (RocksDB Metadata Only), ADR-007 (Storage Registry)

## Context

KalamDB uses a **write-to-RocksDB + periodic-flush-to-Parquet** architecture for user tables. The flush operation must:

1. **Prevent memory spikes** when flushing large tables (millions of rows)
2. **Maintain read consistency** during concurrent writes
3. **Ensure per-user file isolation** for access control and query performance
4. **Handle partial failures** gracefully (don't lose data on Parquet write errors)
5. **Enable immediate deletion** to free buffer space efficiently

This ADR documents the **streaming flush algorithm** that addresses these requirements.

## Decision

### 1. Streaming Write with RocksDB Snapshot

Instead of loading all table data into memory before writing Parquet files, we use a **streaming approach**:

```
┌─────────────────────────────────────────────────────────────┐
│ Flush Execution Flow (UserTableFlushJob::execute_flush)    │
└─────────────────────────────────────────────────────────────┘

1. CREATE SNAPSHOT (RocksDB)
   │
   ├─> Provides point-in-time read consistency
   └─> Prevents missing rows from concurrent inserts

2. SCAN TABLE CF SEQUENTIALLY (via snapshot iterator)
   │
   ├─> Process rows one-by-one (O(1) memory per row)
   └─> Skip soft-deleted rows (_deleted = true)

3. ACCUMULATE ROWS FOR CURRENT USER (in-memory buffer)
   │
   ├─> Buffer size = rows for single user (not entire table)
   └─> Typical user has 100-10K rows (manageable memory)

4. DETECT USER BOUNDARY (current_user_id ≠ previous_user_id)
   │
   ├─> Trigger Parquet write for completed user
   └─> Reset buffer for next user

5. WRITE PARQUET FILE (per user)
   │
   ├─> Filename: YYYY-MM-DDTHH-MM-SS.parquet (ISO 8601)
   ├─> Path: {base_directory}/{namespace}/{tableName}/{userId}/
   └─> Schema: Arrow schema from CREATE TABLE

6. DELETE FLUSHED ROWS (atomic per-user batch)
   │
   ├─> Delete RocksDB keys for successfully flushed user
   └─> On Parquet failure, keep rows in buffer (retry next flush)

7. REPEAT FOR NEXT USER
   └─> Continue until all users processed
```

**Key Benefits**:
- **Memory efficiency**: Only one user's data in memory at a time
- **Read consistency**: Snapshot prevents "phantom reads" from concurrent INSERTs
- **Fault tolerance**: Partial flush success (some users flushed, others retried)

### 2. Per-User File Isolation Principle

Each user's data is written to a **separate Parquet file** during each flush:

```
data/
├── prod/                        # namespace
│   └── events/                  # table
│       ├── user123/             # user_id
│       │   ├── 2025-10-22T10-15-30.parquet  (300 rows)
│       │   ├── 2025-10-22T10-20-45.parquet  (150 rows)
│       │   └── 2025-10-22T10-25-12.parquet  (200 rows)
│       ├── user456/
│       │   ├── 2025-10-22T10-16-05.parquet  (500 rows)
│       │   └── 2025-10-22T10-21-33.parquet  (250 rows)
│       └── user789/
│           └── 2025-10-22T10-17-22.parquet  (120 rows)
```

**Why Per-User Files?**

1. **Row-Level Access Control**: Query engine filters Parquet files by `user_id` directory
   - `SELECT * FROM events WHERE user_id = 'user123'` → Only reads `user123/*.parquet`
   - No need to scan irrelevant users' data

2. **Efficient User Data Deletion**: Dropping user data = delete entire directory
   - `DELETE FROM events WHERE user_id = 'user123'` → Remove `user123/` directory
   - Avoid expensive Parquet rewrites for GDPR compliance

3. **Parallel Query Execution**: Different users' files can be read concurrently
   - Thread pool reads `user123/*.parquet` and `user456/*.parquet` in parallel
   - Scales linearly with CPU cores

4. **Incremental Flush Efficiency**: Each flush appends new file, no merging
   - No need to read existing Parquet files during flush
   - Write-only operation (fast)

**Trade-offs**:
- More files = higher filesystem metadata overhead (mitigated by periodic compaction)
- Small users create small files (< 1MB) → Compaction merges them into larger files

### 3. Immediate Deletion Pattern

After successfully writing Parquet for a user, **immediately delete** their rows from RocksDB:

```rust
// T151e: Write Parquet file
let flush_result = self.flush_user_data(prev_user_id, &current_user_rows, &mut parquet_files);

match flush_result {
    Ok(rows_count) => {
        // T151f: Immediate deletion after successful Parquet write
        let keys: Vec<Vec<u8>> = current_user_rows.iter().map(|(key, _)| key.clone()).collect();
        self.delete_flushed_keys(&keys)?;
        
        log::debug!("Flushed {} rows for user {} (deleted from buffer)", rows_count, prev_user_id);
    }
    Err(e) => {
        // T151g: On failure, keep rows in RocksDB for retry
        log::error!("Failed to flush user {}: {}. Rows kept in buffer.", prev_user_id, e);
    }
}
```

**Why Immediate Deletion?**

1. **Free buffer space quickly**: Prevent RocksDB from growing unbounded
2. **Atomic per-user guarantee**: All rows for user deleted together (no partial state)
3. **Retry safety**: Failed users remain in buffer for next flush attempt
4. **Memory reclamation**: RocksDB compaction can reclaim disk space sooner

**Alternative Rejected**: "Flush all users, then delete all"
- Risk: If deletion fails, data is duplicated (RocksDB + Parquet)
- Problem: Can't distinguish which users were successfully flushed

### 4. Parquet File Naming Convention

Filenames use **ISO 8601 timestamps** with second-level precision:

**Format**: `YYYY-MM-DDTHH-MM-SS.parquet`

**Examples**:
- `2025-10-22T14-30-45.parquet`
- `2025-10-22T14-31-12.parquet`

**Why ISO 8601?**

1. **Lexicographic ordering = chronological ordering**
   - File listing naturally shows time sequence
   - `ls -l` output is already sorted by time

2. **Cross-platform compatibility**
   - No colons (Windows forbids `:` in filenames)
   - Uses hyphens as separators

3. **Collision resistance**
   - Second-level precision + UUID job_id → virtually no collisions
   - Even concurrent flushes for same user create different timestamps

4. **Human-readable**
   - Easy to identify when a flush occurred
   - Useful for debugging and auditing

**Alternative Rejected**: UUID-only filenames (e.g., `3f7b9a12-4c8d-4e5f-9b1a-2c3d4e5f6a7b.parquet`)
- Not chronologically sortable
- Hard to determine flush time without metadata lookup

### 5. Template Path Resolution

Storage paths use **single-pass template substitution** with validation at `CREATE STORAGE` time:

```sql
CREATE STORAGE local
    TYPE filesystem
    BASE_DIRECTORY '/mnt/data'
    USER_TABLES_TEMPLATE '{namespace}/{tableName}/users/{userId}'
    SYSTEM_TABLES_TEMPLATE 'system/{tableName}';
```

**Substitution Variables**:
- `{namespace}` → Namespace ID (e.g., `prod`)
- `{tableName}` → Table name (e.g., `events`)
- `{userId}` → User ID (e.g., `user123`)
- `{shard}` → Shard number (future: from sharding strategy)

**Resolution Example**:
```
Template: "{namespace}/{tableName}/users/{userId}"
Namespace: prod
TableName: events
UserId: user123

Resolved: "prod/events/users/user123"
Full Path: "/mnt/data/prod/events/users/user123/2025-10-22T14-30-45.parquet"
```

**Why Single-Pass Substitution?**

1. **Performance**: O(n) string replacement, no parsing overhead
2. **Simplicity**: No regex or complex evaluation
3. **Safety**: Template validation happens once at CREATE STORAGE (not every flush)
4. **Determinism**: Same inputs → same output, no edge cases

**Template Validation** (at CREATE STORAGE time):
- Check for invalid variables (e.g., `{unknown}`)
- Ensure required variables are present (e.g., `{userId}` for user tables)
- Prevent path traversal attacks (e.g., `../../../etc/passwd`)

## Consequences

### Positive

1. **Memory efficiency**: Streaming flush prevents out-of-memory errors on large tables
2. **Fault tolerance**: Partial flush success with per-user atomicity
3. **Read consistency**: RocksDB snapshot prevents phantom reads during flush
4. **Query performance**: Per-user files enable row-level access control without full table scans
5. **Operational simplicity**: ISO 8601 filenames make debugging easy

### Negative

1. **More filesystem metadata**: Each user creates separate files
   - Mitigated by periodic compaction (merge small files into larger ones)
   
2. **Small file problem**: Users with few rows create tiny Parquet files (< 100KB)
   - Mitigated by compaction strategy (future: User Story 8)

3. **No cross-user transactions**: Each user flushed independently
   - Acceptable: User tables are isolated by design (Table-per-User architecture)

### Neutral

1. **Flush duration depends on user count**: More users = longer flush time
   - Acceptable: Background operation, doesn't block queries
   
2. **Delete-heavy workloads create Parquet churn**: Frequent deletes → many files with `_deleted` rows
   - Mitigated by compaction (rewrite files without deleted rows)

## Implementation Notes

### Streaming Algorithm (T151-T151h)

```rust
fn execute_flush(&self) -> Result<(usize, usize, Vec<String>), KalamDbError> {
    // T151a: Create RocksDB snapshot
    let snapshot = self.store.create_snapshot();
    
    // T151c: Streaming buffer for current user
    let mut current_user_id: Option<String> = None;
    let mut current_user_rows: Vec<(Vec<u8>, JsonValue)> = Vec::new();
    
    // T151b: Scan using snapshot iterator
    let iter = self.store.scan_with_snapshot(&snapshot, ns, table)?;
    
    for item in iter {
        let (key_bytes, value_bytes) = item?;
        let (user_id, _row_id) = self.parse_user_key(&key_str)?;
        
        // T151d: Detect user boundary
        if current_user_id.is_some() && current_user_id != Some(user_id) {
            // T151e: Write Parquet for completed user
            self.flush_user_data(prev_user_id, &current_user_rows, &mut files)?;
            
            // T151f: Delete successfully flushed rows
            self.delete_flushed_keys(&keys)?;
            
            // Reset buffer for next user
            current_user_rows.clear();
        }
        
        // Accumulate row for current user
        current_user_rows.push((key_bytes, row_data));
    }
    
    // Flush final user
    // ...
}
```

### Per-User File Writing (T161a)

```rust
fn flush_user_data(&self, user_id: &str, rows: &[(Vec<u8>, JsonValue)]) -> Result<usize, KalamDbError> {
    // T161c: Resolve storage path using template
    let user_storage_path = self.resolve_storage_path_for_user(&user_id)?;
    
    // T161b: Generate ISO 8601 filename
    let batch_filename = self.generate_batch_filename(); // "2025-10-22T14-30-45.parquet"
    let output_path = PathBuf::from(&user_storage_path).join(&batch_filename);
    
    // Write Parquet file
    let writer = ParquetWriter::new(output_path.to_str().unwrap());
    writer.write(self.schema.clone(), vec![batch])?;
    
    Ok(rows.len())
}
```

### Template Resolution (T161c)

```rust
fn resolve_storage_path_for_user(&self, user_id: &UserId) -> Result<String, KalamDbError> {
    if let Some(ref registry) = self.storage_registry {
        let storage = registry.get_storage("local")?;
        let template = &storage.user_tables_template;
        
        // Single-pass substitution
        let path = template
            .replace("{namespace}", self.namespace_id.as_str())
            .replace("{tableName}", self.table_name.as_str())
            .replace("{userId}", user_id.as_str())
            .replace("{shard}", ""); // Future: sharding strategy
        
        let full_path = format!("{}/{}", storage.base_directory, path);
        Ok(full_path)
    } else {
        // Fallback to legacy path
        Ok(self.substitute_user_id_in_path(user_id))
    }
}
```

## References

- **ADR-001**: Table-per-User Architecture (explains why per-user isolation matters)
- **ADR-005**: RocksDB Metadata Only (explains write-to-buffer + flush-to-Parquet pattern)
- **ADR-007**: Storage Registry (explains template validation and multi-storage support)
- **T151-T151h**: Streaming flush implementation tasks
- **T161a-T161c**: Per-user file isolation and template resolution tasks
- **T162-T162b**: Documentation tasks for this ADR

## Future Enhancements

1. **Sharding Support** (User Story 6): Populate `{shard}` variable using sharding strategy
2. **Compaction** (User Story 8): Merge small per-user files into larger ones periodically
3. **Parallel Flush**: Process multiple users concurrently (requires thread-safe RocksDB iterator)
4. **Incremental Flush**: Track last flush timestamp per user, only flush new rows
