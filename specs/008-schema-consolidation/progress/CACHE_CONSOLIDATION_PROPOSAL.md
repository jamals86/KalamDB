# Cache Consolidation Proposal - SIMPLIFIED DESIGN

## Problem
Currently we have **two separate caches** for table data:

1. **TableCache** (`catalog/table_cache.rs`):
   - Data: `TableMetadata` + storage path templates
   - Purpose: Storage path resolution for flush operations
   - Key: `(NamespaceId, TableName)`

2. **SchemaCache** (`tables/system/schemas/schema_cache.rs`):
   - Data: `TableDefinition` (full schema with columns)
   - Purpose: DESCRIBE TABLE, schema validation
   - Key: `TableId`

**Issues:**
- Double memory usage for essentially the same data (~50% waste)
- Must update/invalidate both caches on CREATE/ALTER/DROP TABLE
- Risk of inconsistency between caches
- More complex code maintenance

## Solution: Unified SchemaCache

### Design Simplification

**Key Insight**: TableId already contains NamespaceId and TableName internally, so we don't need a separate reverse index!

```rust
/// Unified cache for all table-related data (replaces both TableCache and SchemaCache)
/// Location: backend/crates/kalamdb-core/src/catalog/schema_cache.rs
pub struct SchemaCache {
    /// Main cache: TableId → CachedTableData
    /// TableId already contains (namespace_id, table_name) so no reverse index needed!
    tables: DashMap<TableId, Arc<CachedTableData>>,
    
    /// Storage registry for path resolution
    storage_registry: Option<Arc<StorageRegistry>>,
    
    // LRU + metrics (from old SchemaCache)
    access_times: DashMap<TableId, u64>,
    access_counter: AtomicUsize,
    max_size: usize,
    hit_count: AtomicUsize,
    miss_count: AtomicUsize,
    eviction_count: AtomicUsize,
}

/// All cached data for a single table (replaces TableMetadata)
#[derive(Debug, Clone)]
struct CachedTableData {
    /// Table identifier (contains namespace_id + table_name)
    table_id: TableId,
    
    /// Type of table (User, Shared, System, Stream)
    table_type: TableType,
    
    /// When the table was created
    created_at: DateTime<Utc>,
    
    /// Reference to storage configuration
    storage_id: Option<StorageId>,
    
    /// When to flush buffered data to Parquet
    flush_policy: FlushPolicy,
    
    /// Partially-resolved storage path template
    /// Static placeholders ({namespace}, {tableName}) already substituted
    /// Dynamic placeholders ({userId}, {shard}) remain for per-request resolution
    storage_path_template: String,
    
    /// Current schema version number
    schema_version: u32,
    
    /// How long to keep deleted rows (in hours)
    deleted_retention_hours: Option<u32>,
    
    /// Full schema definition with columns
    schema: Arc<TableDefinition>,
}
```

### Key Features

1. **Single Source of Truth**: One cache stores all table data
2. **Simplified Lookups**: 
   - By TableId: Direct O(1) lookup: `cache.get(&table_id)`
   - By (namespace, table_name): Create TableId first, then O(1) lookup: `cache.get(&TableId::new(ns, name))`
3. **Integrated Storage Paths**: Template cached inside CachedTableData (no separate map needed)
4. **Lock-Free**: DashMap for concurrent access
5. **LRU Eviction**: Memory bounded with least-recently-used eviction

### Migration Path

**Phase 1: Create New SchemaCache**
- [ ] T300: Create `catalog/schema_cache.rs` with simplified design
- [ ] T301: Implement CachedTableData struct with all fields
- [ ] T302: Implement SchemaCache::new(max_size) constructor
- [ ] T303: Implement get(&table_id) → Option<Arc<CachedTableData>>
- [ ] T304: Implement get_by_name(namespace, table_name) → Option<Arc<CachedTableData>>
- [ ] T305: Implement insert(table_id, data) with LRU eviction
- [ ] T306: Implement invalidate(&table_id)
- [ ] T307: Implement get_storage_path(table_id, user_id, shard) for dynamic resolution
- [ ] T308: Add comprehensive unit tests (15+ tests)

**Phase 2: Update SqlExecutor**
- [ ] T309: Replace `table_cache` and `schema_cache` fields with single `schema_cache: SchemaCache`
- [ ] T310: Update CREATE TABLE to insert CachedTableData into schema_cache
- [ ] T311: Update ALTER TABLE to invalidate schema_cache entry
- [ ] T312: Update DROP TABLE to remove from schema_cache
- [ ] T313: Update DESCRIBE TABLE to use schema_cache.get()

**Phase 3: Update Consumers**
- [ ] T314: Update UserTableFlushJob to use schema_cache.get_storage_path()
- [ ] T315: Update SharedTableFlushJob to use schema_cache.get_storage_path()
- [ ] T316: Update system table providers to use schema_cache.get()
- [ ] T317: Update all DESCRIBE TABLE paths to use schema_cache

**Phase 4: Remove Old Caches**
- [ ] T318: Delete `catalog/table_cache.rs` (516 lines removed)
- [ ] T319: Delete `tables/system/schemas/schema_cache.rs` (443 lines removed)
- [ ] T320: Delete `catalog/table_metadata.rs` (replaced by CachedTableData)
- [ ] T321: Update `catalog/mod.rs` to export only SchemaCache
- [ ] T322: Update all imports across codebase

**Phase 5: Performance Testing**
- [ ] T323: Benchmark cache hit rates (target: >99%)
- [ ] T324: Benchmark cache hit latency (target: <100μs)
- [ ] T325: Memory profiling to verify ~50% reduction
- [ ] T326: Run full test suite and verify all pass

## Benefits

1. **Memory Savings**: ~50% reduction in cache memory usage
2. **Simpler Code**: One cache instead of two (~1000 lines deleted)
3. **Consistency**: Impossible for caches to get out of sync
4. **Better Performance**: Single cache lookup instead of potentially two
5. **Easier Maintenance**: One place to add features/fixes
6. **Cleaner API**: No need for reverse indexes or dual key management

## Estimated Effort

- **Phase 1**: 2-3 hours (create cache + tests)
- **Phase 2**: 1-2 hours (update SqlExecutor)
- **Phase 3**: 2-3 hours (update consumers)
- **Phase 4**: 1 hour (cleanup + delete old files)
- **Phase 5**: 1-2 hours (testing)

**Total**: ~7-11 hours (reduced from 10-13 hours due to simpler design)

## Risks

- **Migration Complexity**: Need to ensure all code paths are updated
- **Testing Coverage**: Must verify all table operations work correctly
- **Performance**: Need to validate no performance regression

## Recommendation

**Implement immediately** as part of Phase 9 completion. This is a clear architectural improvement that will:
- Reduce technical debt
- Improve code maintainability  
- Free up memory for actual data (~50% savings)
- Eliminate cache consistency bugs
- Simplify future development
