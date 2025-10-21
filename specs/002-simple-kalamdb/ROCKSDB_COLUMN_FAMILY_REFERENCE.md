# RocksDB Column Family Quick Reference

**Feature**: 002-simple-kalamdb  
**Purpose**: Quick reference for RocksDB column family architecture implementation

## Column Family Naming Convention

| Table Type | Column Family Name Pattern | Example |
|-----------|---------------------------|---------|
| User Table | `user_table:{namespace}:{table_name}` | `user_table:production:messages` |
| Shared Table | `shared_table:{namespace}:{table_name}` | `shared_table:production:conversations` |
| Stream Table | `stream_table:{namespace}:{table_name}` | `stream_table:production:ai_signals` |
| System Table | `system_table:{table_name}` | `system_table:users` |

## Key Format by Table Type

| Table Type | Key Format | Example | Purpose |
|-----------|-----------|---------|---------|
| User Table | `{user_id}:{row_id}` | `user123:1234567890123456789` | Enables grouping by user_id prefix during flush |
| Shared Table | `{row_id}` | `1234567890123456789` | Simple key, no user isolation |
| Stream Table | `{timestamp_ms}:{row_id}` | `1697385600000:1234567890123456789` | Enables TTL-based eviction (sort by timestamp) |
| System Table | Entity-specific | `user123` (users), `sub_456` (live_queries) | Varies by system table purpose |

## Flush Behavior

| Table Type | Flush Policy | Parquet Output | Delete After Flush |
|-----------|-------------|----------------|-------------------|
| User Table | Row limit OR time interval | **Separate Parquet per user**: `{user_id}/batch-*.parquet` | ✅ Yes (delete flushed rows from RocksDB) |
| Shared Table | Row limit OR time interval | **Single Parquet**: `shared/{table_name}/batch-*.parquet` | ✅ Yes (delete flushed rows from RocksDB) |
| Stream Table | **NEVER flush** | ❌ No Parquet (ephemeral) | ✅ Yes (TTL-based eviction) |
| System Table | **NEVER flush** | ❌ No Parquet (always in RocksDB) | ❌ No (except jobs retention policy) |

## User Table Flush Algorithm

```rust
// Pseudocode for user table flush job
fn flush_user_table(column_family: &str) {
    // 1. Iterate through column family
    let rows = iterate_column_family(column_family);
    
    // 2. Group rows by user_id prefix
    let grouped = rows.group_by(|row| {
        let key = row.key(); // e.g., "user123:1234567890123456789"
        key.split(':').next() // Extract "user123"
    });
    
    // 3. For each user_id, write separate Parquet file
    for (user_id, user_rows) in grouped {
        let parquet_path = format!("{}/batch-{}.parquet", user_id, timestamp);
        write_parquet(parquet_path, user_rows);
    }
    
    // 4. Delete flushed rows from RocksDB
    delete_from_column_family(column_family, rows.keys());
}
```

## Shared Table Flush Algorithm

```rust
// Pseudocode for shared table flush job
fn flush_shared_table(column_family: &str, table_name: &str) {
    // 1. Read all buffered rows from column family
    let rows = iterate_column_family(column_family);
    
    // 2. Write to SINGLE Parquet file
    let parquet_path = format!("shared/{}/batch-{}.parquet", table_name, timestamp);
    write_parquet(parquet_path, rows);
    
    // 3. Delete flushed rows from RocksDB
    delete_from_column_family(column_family, rows.keys());
}
```

## Stream Table Eviction Algorithm

```rust
// Pseudocode for stream table TTL eviction
fn evict_expired_events(column_family: &str, retention_ms: u64) {
    let cutoff_timestamp = now_ms() - retention_ms;
    
    // 1. Iterate through column family (keys are timestamp-prefixed)
    let mut expired_keys = Vec::new();
    
    for (key, _value) in iterate_column_family(column_family) {
        let timestamp = key.split(':').next().parse::<u64>(); // Extract timestamp
        if timestamp < cutoff_timestamp {
            expired_keys.push(key);
        } else {
            break; // Keys are sorted by timestamp, rest are newer
        }
    }
    
    // 2. Delete expired entries
    delete_from_column_family(column_family, &expired_keys);
}

// Pseudocode for max buffer eviction
fn evict_oldest_events(column_family: &str, max_buffer: usize) {
    let current_count = count_column_family_entries(column_family);
    
    if current_count > max_buffer {
        let to_evict = current_count - max_buffer;
        
        // 1. Get oldest entries (lowest timestamp prefix)
        let oldest_keys = iterate_column_family(column_family)
            .take(to_evict)
            .map(|(key, _)| key)
            .collect();
        
        // 2. Delete oldest entries
        delete_from_column_family(column_family, &oldest_keys);
    }
}
```

## System Columns by Table Type

| Table Type | System Columns | Purpose |
|-----------|---------------|---------|
| User Table | `_updated` (TIMESTAMP), `_deleted` (BOOLEAN) | Change tracking, soft deletes, bloom filter optimization |
| Shared Table | `_updated` (TIMESTAMP), `_deleted` (BOOLEAN) | Change tracking, soft deletes, bloom filter optimization |
| Stream Table | ❌ **NO system columns** | Ephemeral events don't need change tracking |
| System Table | Varies by table | Metadata-specific columns |

## Column Family Lifecycle

### Creation (on CREATE TABLE)
1. Parse CREATE TABLE command
2. Validate table schema
3. **Create column family** with naming convention: `{table_type}:{namespace}:{table_name}`
4. Register table metadata in catalog
5. Create schema version file (schema_v1.json)
6. Update manifest.json

### Usage (INSERT/UPDATE/DELETE)
1. Resolve table to column family name
2. Generate key using appropriate format
3. Write/update/delete in column family
4. Monitor flush policy (row count or time interval)
5. Trigger flush job when threshold reached

### Deletion (on DROP TABLE)
1. Check for active subscriptions (prevent drop if exists)
2. **Delete entire column family**
3. Delete all Parquet files from storage
4. Remove from manifest.json
5. Delete schema directory
6. Update storage location usage count

## RocksDB Configuration

| Setting | User Table CF | Shared Table CF | Stream Table CF | System Table CF |
|---------|--------------|-----------------|-----------------|-----------------|
| **Memtable Size** | Configurable (default 64MB) | Configurable (default 64MB) | Smaller (default 16MB) | Small (default 8MB) |
| **Write Buffer** | Configurable (default 128MB) | Configurable (default 128MB) | Smaller (default 32MB) | Small (default 16MB) |
| **WAL** | Enabled | Enabled | Enabled | Enabled |
| **Compaction** | Level-based | Level-based | Size-based (frequent) | Level-based |
| **TTL** | ❌ No (use flush) | ❌ No (use flush) | ✅ Yes (retention period) | ⚠️ Jobs table only |

## Implementation Files

| Component | File Path |
|-----------|-----------|
| Column Family Manager | `backend/crates/kalamdb-core/src/storage/column_family_manager.rs` |
| RocksDB Config | `backend/crates/kalamdb-core/src/storage/rocksdb_config.rs` |
| RocksDB Init | `backend/crates/kalamdb-core/src/storage/rocksdb_init.rs` |
| User Table Flush | `backend/crates/kalamdb-core/src/flush/user_table_flush.rs` |
| Shared Table Flush | `backend/crates/kalamdb-core/src/flush/shared_table_flush.rs` |
| Stream Table Eviction | `backend/crates/kalamdb-core/src/tables/stream_table_eviction.rs` |
| Catalog Store | `backend/crates/kalamdb-core/src/catalog/catalog_store.rs` |

## Key Implementation Tasks

| Task ID | Description | Phase |
|---------|-------------|-------|
| T027 | Column family manager | Foundation |
| T027a | Column family naming utilities | Foundation |
| T027b | RocksDB configuration | Foundation |
| T027c | RocksDB initialization | Foundation |
| T113 | Create column family for user table | User Story 3 |
| T121 | User table flush with grouping | User Story 3 |
| T148 | Create column family for stream table | User Story 4a |
| T149 | Stream table INSERT with timestamp key | User Story 4a |
| T160 | Create column family for shared table | User Story 5 |
| T164 | Shared table flush (single Parquet) | User Story 5 |

## Testing Checklist

- [ ] Column family creation on CREATE TABLE
- [ ] Correct key format for each table type
- [ ] User table flush groups by user_id prefix
- [ ] Shared table flush produces single Parquet
- [ ] Stream table eviction respects TTL
- [ ] Stream table max buffer enforcement
- [ ] Column family deletion on DROP TABLE
- [ ] System table reads/writes work correctly
- [ ] Per-column-family configuration applied
- [ ] Flush job records in system.jobs table

## Performance Considerations

1. **User tables**: Keys naturally distributed, efficient compaction even with millions of users
2. **Shared tables**: Monitor size, may need partitioning if very large
3. **Stream tables**: Frequent eviction prevents unbounded growth, timestamp prefix enables efficient scanning
4. **System tables**: Small size, always in memory for fast lookups
5. **Column family isolation**: Each table's operations don't affect others (independent memtables, compaction)

## Common Pitfalls to Avoid

❌ **Don't** add _updated/_deleted to stream tables  
✅ **Do** validate table type before system column injection

❌ **Don't** flush stream tables to Parquet  
✅ **Do** check table type in flush trigger logic

❌ **Don't** use single Parquet for user tables  
✅ **Do** group by user_id prefix and write separate files

❌ **Don't** delete column families directly  
✅ **Do** use column_family_manager for all operations

❌ **Don't** store namespaces/storage locations in RocksDB  
✅ **Do** keep metadata in memory (loaded from JSON)
