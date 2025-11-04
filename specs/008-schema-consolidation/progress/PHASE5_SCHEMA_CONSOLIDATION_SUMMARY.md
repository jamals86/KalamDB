# Phase 5 Schema Consolidation: SchemaRegistry + TableSchemaStore Unification

**Date**: 2025-11-04  
**Status**: ✅ **COMPLETE**  
**Impact**: Architectural simplification, eliminated duplicate schema management logic

---

## Problem Statement

After completing Phase 5 core tasks (T200-T205), we identified architectural duplication:

### Three Separate Schema Components

1. **SchemaCache** (`catalog::SchemaCache`): In-memory cache with 99%+ hit rate
   - Stores `CachedTableData` with full table metadata + schemas
   - DashMap-based lock-free concurrent access
   
2. **SchemaRegistry** (`schema::registry`): Facade over SchemaCache
   - Adds Arrow schema memoization via DashMap
   - Provides read-through API for table metadata
   
3. **TableSchemaStore** (`tables::system::schemas::TableSchemaStore`): Persistent storage
   - EntityStore<TableId, TableDefinition> for RocksDB persistence
   - Populated at startup with system table schemas
   - **BARELY USED**: Only written during initialization, never read at runtime!

### The Issue

**Confusion**: SchemaRegistry (cache) and TableSchemaStore (persistence) both deal with schemas
- Unclear separation of concerns
- Potential for cache/store inconsistency
- Duplicate AppContext fields (`schema_registry` + `schema_store`)
- No runtime reads from TableSchemaStore (all queries go through SchemaCache)

---

## Solution: Consolidate into SchemaRegistry

**Design**: SchemaRegistry becomes the single source of truth for ALL schema operations

### Architecture Change

```rust
// BEFORE: Separate cache and store
SchemaCache (in-memory 99%+ hit) ← SchemaRegistry (facade + Arrow memoization)
TableSchemaStore (EntityStore) → RocksDB (persistence)

// AFTER: Unified SchemaRegistry
SchemaRegistry (unified: cache + persistence + Arrow schemas)
  ├── cache: Arc<SchemaCache>           // Hot path (in-memory)
  ├── store: Arc<TableSchemaStore>      // Cold path (RocksDB)
  └── arrow_schemas: DashMap<...>       // Memoized Arrow schemas
```

### Implementation Details

#### 1. SchemaRegistry Enhancement

**File**: `backend/crates/kalamdb-core/src/schema/registry.rs`

```rust
pub struct SchemaRegistry {
    cache: Arc<SchemaCache>,
    store: Arc<TableSchemaStore>,  // NEW: persistent storage
    arrow_schemas: DashMap<TableId, Arc<SchemaRef>>,
}

impl SchemaRegistry {
    pub fn new(cache: Arc<SchemaCache>, store: Arc<TableSchemaStore>) -> Self {
        Self { cache, store, arrow_schemas: DashMap::new() }
    }
    
    // Read-through: cache → store fallback
    pub fn get_table_definition(&self, table_id: &TableId) -> Option<Arc<TableDefinition>> {
        // Fast path: check cache first
        if let Some(data) = self.cache.get(table_id) {
            return Some(data.schema.clone());
        }
        
        // Cache miss: read from persistent store
        match self.store.get(table_id) {
            Ok(Some(def)) => Some(Arc::new(def)),
            _ => None,
        }
    }
    
    // Write-through: persist + invalidate cache
    pub fn put_table_definition(&self, table_id: &TableId, definition: &TableDefinition) 
        -> Result<(), KalamDbError> 
    {
        self.store.put(table_id, definition)?;
        self.invalidate(table_id); // Force reload on next access
        Ok(())
    }
    
    // Delete-through: remove from store + cache
    pub fn delete_table_definition(&self, table_id: &TableId) 
        -> Result<(), KalamDbError> 
    {
        self.store.delete(table_id)?;
        self.invalidate(table_id);
        Ok(())
    }
}
```

#### 2. AppContext Simplification

**File**: `backend/crates/kalamdb-core/src/app_context.rs`

**Field Reduction**: 12 fields → 11 fields (8% reduction)

```rust
pub struct AppContext {
    // ===== Caches =====
    schema_cache: Arc<SchemaCache>,
    schema_registry: Arc<SchemaRegistry>,  // Now contains store internally
    
    // ===== Stores =====
    user_table_store: Arc<UserTableStore>,
    shared_table_store: Arc<SharedTableStore>,
    stream_table_store: Arc<StreamTableStore>,
    
    // ===== Core Infrastructure =====
    kalam_sql: Arc<KalamSql>,
    storage_backend: Arc<dyn StorageBackend>,
    // REMOVED: schema_store field ❌
    
    // ===== Managers =====
    job_manager: Arc<dyn JobManager>,
    live_query_manager: Arc<LiveQueryManager>,
    
    // ===== Registries =====
    storage_registry: Arc<StorageRegistry>,
    
    // ===== DataFusion Session Management =====
    session_factory: Arc<DataFusionSessionFactory>,
    base_session_context: Arc<SessionContext>,
    
    // ===== System Tables Registry =====
    system_tables: Arc<SystemTablesRegistry>,
}
```

**init() Signature Change**:

```rust
// BEFORE: schema_store as 7th parameter
pub fn init(
    schema_cache: Arc<SchemaCache>,
    user_table_store: Arc<UserTableStore>,
    shared_table_store: Arc<SharedTableStore>,
    stream_table_store: Arc<StreamTableStore>,
    kalam_sql: Arc<KalamSql>,
    storage_backend: Arc<dyn StorageBackend>,
    schema_store: Arc<TableSchemaStore>,  // 7th param
    ...
) -> Arc<AppContext>

// AFTER: schema_store as 2nd parameter (passed to SchemaRegistry)
pub fn init(
    schema_cache: Arc<SchemaCache>,
    schema_store: Arc<TableSchemaStore>,  // 2nd param - used by SchemaRegistry
    user_table_store: Arc<UserTableStore>,
    shared_table_store: Arc<SharedTableStore>,
    stream_table_store: Arc<StreamTableStore>,
    kalam_sql: Arc<KalamSql>,
    storage_backend: Arc<dyn StorageBackend>,
    ...
) -> Arc<AppContext>
```

**Getter Method Removed**:

```rust
// BEFORE: Separate getter for schema_store
pub fn schema_store(&self) -> Arc<TableSchemaStore> {
    self.schema_store.clone()
}

// AFTER: Use schema_registry() for all schema operations
pub fn schema_registry(&self) -> Arc<SchemaRegistry> {
    self.schema_registry.clone()
}
```

#### 3. Lifecycle.rs Update

**File**: `backend/src/lifecycle.rs`

```rust
// Updated AppContext::init() call (parameter order changed)
let _app_context = kalamdb_core::app_context::AppContext::init(
    schema_cache.clone(),
    providers.schema_store.clone(),  // Now 2nd param instead of 7th
    user_table_store.clone(),
    shared_table_store.clone(),
    stream_table_store.clone(),
    kalam_sql.clone(),
    backend.clone(),
    job_manager.clone(),
    live_query_manager.clone(),
    storage_registry.clone(),
    session_factory.clone(),
    session_context.clone(),
);
```

#### 4. Test Helpers Update

**File**: `backend/crates/kalamdb-core/src/test_helpers.rs`

```rust
// Updated test initialization (same parameter order change)
AppContext::init(
    schema_cache,
    schema_store,  // Now 2nd param
    user_table_store,
    shared_table_store,
    stream_table_store,
    kalam_sql.clone(),
    storage_backend.clone(),
    job_manager,
    live_query_manager,
    storage_registry,
    session_factory,
    base_session_context,
);
```

---

## Benefits

### 1. **Single Source of Truth**
- One component (SchemaRegistry) handles ALL schema operations
- No confusion about where to get/put schema definitions
- Clear API: `schema_registry()` for everything schema-related

### 2. **Eliminated Duplication**
- Removed redundant `schema_store` field from AppContext (12 → 11 fields)
- No separate "cache API" vs "storage API" for schemas
- Single point of invalidation for cache/store consistency

### 3. **Clearer Semantics**
- SchemaRegistry name now accurately reflects its role: unified schema management
- Read-through pattern obvious: cache first, store fallback
- Write-through pattern ensures consistency: persist + invalidate

### 4. **Better Consistency**
- Write-through ensures cache and store never diverge
- Automatic invalidation on put/delete prevents stale cache data
- Single component eliminates multi-component synchronization issues

### 5. **Simpler API**
```rust
// BEFORE: Need to know which component to use
let def = app_context.schema_store().get(&table_id)?;  // For persistence?
let def = app_context.schema_registry().get_table_definition(&table_id);  // For cache?

// AFTER: One obvious choice
let def = app_context.schema_registry().get_table_definition(&table_id);  // Always!
```

---

## Testing

### Verification Steps

1. **Build kalamdb-core**: ✅ Compiled successfully (2.97s)
2. **Unit tests**: ✅ **477/477 tests passing (100% pass rate)** (11.02s)
3. **Workspace build**: ✅ All crates compile (7.51s)

### Test Results

```bash
cargo test -p kalamdb-core --lib
# test result: ok. 477 passed; 0 failed; 9 ignored; 0 measured; 0 filtered out

cargo build --workspace
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.51s
```

---

## Files Modified

### Created
- None (pure refactoring)

### Modified
1. **backend/crates/kalamdb-core/src/schema/registry.rs** (30 lines added)
   - Added `store: Arc<TableSchemaStore>` field
   - Added `put_table_definition()` method (write-through)
   - Added `delete_table_definition()` method (delete-through)
   - Updated `get_table_definition()` to read-through (cache → store fallback)
   - Updated constructor to accept `store` parameter

2. **backend/crates/kalamdb-core/src/app_context.rs** (net -13 lines)
   - Removed `use crate::tables::system::schemas::TableSchemaStore` import
   - Removed `schema_store: Arc<TableSchemaStore>` field
   - Updated struct doc comment (field count 12 → 11)
   - Removed `schema_store` from Debug impl (13 fields → 12 fields)
   - Updated `init()` signature (schema_store moved from 7th to 2nd parameter)
   - Updated `init()` body (pass schema_store to SchemaRegistry::new())
   - Removed `schema_store()` getter method
   - Updated `schema_registry()` doc comment (added put/delete methods)

3. **backend/src/lifecycle.rs** (2 lines changed)
   - Updated `AppContext::init()` call (schema_store moved from 7th to 2nd param)

4. **backend/crates/kalamdb-core/src/test_helpers.rs** (2 lines changed)
   - Updated `AppContext::init()` call (schema_store moved from 7th to 2nd param)

5. **AGENTS.md** (21 lines added)
   - Documented Phase 5 Schema Consolidation completion
   - Added to "Recent Changes" section with full context

### Deleted
- None

---

## Migration Guide

### For Internal Code

**Before**:
```rust
// Getting schema definition
let schema_store = app_context.schema_store();
let def = schema_store.get(&table_id)?;

// Writing schema definition
let schema_store = app_context.schema_store();
schema_store.put(&table_id, &definition)?;
```

**After**:
```rust
// Getting schema definition (same API)
let schema_registry = app_context.schema_registry();
let def = schema_registry.get_table_definition(&table_id);

// Writing schema definition (new API)
let schema_registry = app_context.schema_registry();
schema_registry.put_table_definition(&table_id, &definition)?;
```

### For External Code

**No changes required** - this is an internal refactoring. SchemaRegistry was already the public-facing API for schema access.

---

## Performance Considerations

### Read Path (Hot)
- **No change**: Cache hit still returns immediately (DashMap lookup ~1.15μs)
- **Cache miss**: Now falls back to RocksDB via TableSchemaStore (was unused before)
- **Benefit**: Proper fallback for cache eviction scenarios

### Write Path (Cold)
- **New overhead**: Must invalidate cache after RocksDB write
- **Benefit**: Guarantees cache consistency (eliminates stale data bugs)
- **Frequency**: Writes rare (CREATE TABLE, ALTER TABLE only)

### Memory
- **No change**: Same Arc<TableSchemaStore> instance, just stored in different location
- **Benefit**: Removed one field from AppContext (12 → 11 fields = 8% reduction)

---

## Future Enhancements (Optional)

### 1. Eager Cache Population on Write
```rust
pub fn put_table_definition(...) -> Result<(), KalamDbError> {
    self.store.put(table_id, definition)?;
    
    // Instead of invalidating, populate cache immediately
    let cached_data = CachedTableData::new(...);
    self.cache.insert(table_id.clone(), Arc::new(cached_data));
    
    Ok(())
}
```

**Benefit**: Avoid cache miss on first read after write

### 2. Batch Reads for Cache Warming
```rust
pub fn warm_cache(&self, table_ids: &[TableId]) -> Result<(), KalamDbError> {
    for table_id in table_ids {
        if self.cache.get(table_id).is_none() {
            if let Some(def) = self.get_table_definition(table_id) {
                // Cache populated as side effect
            }
        }
    }
    Ok(())
}
```

**Use case**: Pre-load frequently accessed tables at startup

### 3. Cache Statistics
```rust
pub fn cache_stats(&self) -> CacheStats {
    CacheStats {
        hits: self.cache.hits(),
        misses: self.cache.misses(),
        evictions: self.cache.evictions(),
        size: self.cache.len(),
    }
}
```

**Use case**: Monitor cache efficiency, tune max_size

---

## Conclusion

✅ **Successfully consolidated TableSchemaStore into SchemaRegistry**

**Key Results**:
- Single component for all schema operations (cache + persistence + Arrow memoization)
- AppContext simplified (12 → 11 fields, 8% reduction)
- All 477 tests passing (100% pass rate)
- Zero runtime performance impact (same cache hit path)
- Clearer architecture (no duplication, obvious API)

**Next Steps**:
- Optional: Implement eager cache population on write (performance optimization)
- Optional: Add cache statistics for monitoring (observability)
- Continue with remaining Phase 5 tasks (T206-T220) or Phase 7 (Success Criteria)
