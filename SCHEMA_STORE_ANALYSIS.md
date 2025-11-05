# Schema Store Architecture Analysis

## Current State

### TableSchemaStore (in `schemas/` directory)
- **Purpose**: EntityStore for persisting `TableDefinition` objects (column definitions, data types, constraints, etc.)
- **Key Type**: `TableId`
- **Value Type**: `TableDefinition` (from kalamdb-commons)
- **Storage**: Uses `SystemTableStore<TableId, TableDefinition>` internally
- **RocksDB Partition**: `"system_table_schemas"`
- **Special Features**: 
  - `scan_namespace()` - prefix scanning by namespace
  - `get_all()` - retrieve all schemas across all namespaces
- **Usage**: Used by SchemaRegistry for persistent storage (read-through/write-through pattern)

### TablesTableProvider (in `tables_v2/` directory)
- **Purpose**: DataFusion TableProvider for **system.tables** table (table metadata registry)
- **Key Type**: `String` (table_id as string)
- **Value Type**: `SystemTable` (from kalamdb-commons/system.rs)
- **Storage**: Uses `SystemTableStore<String, SystemTable>` via `TablesStore` type alias
- **RocksDB Partition**: `"system_tables"`
- **Pattern**: Follows standard v2 pattern with 3 modules:
  - `tables_store.rs` - SystemTableStore wrapper
  - `tables_table.rs` - Schema definition (OnceLock)
  - `tables_provider.rs` - TableProvider implementation

### InformationSchemaTablesProvider
- **Purpose**: Virtual table for SQL-standard `information_schema.tables` view
- **Storage**: **NONE** - reads from TablesTableProvider + SchemaRegistry
- **Pattern**: Composite provider that joins data from multiple sources

## Key Data Models

### SystemTable (Table Metadata)
Registry information about a table (from `kalamdb-commons/models/system.rs`):
- `table_id`, `table_name`, `namespace`, `table_type`
- `created_at`, `storage_id`, `use_user_storage`
- `flush_policy`, `schema_version`, `deleted_retention_hours`, `access_level`

### TableDefinition (Schema Definition)
Logical columnar schema (from `kalamdb-commons/schemas/table_definition.rs`):
- `columns: Vec<ColumnDefinition>` - column names, types, constraints
- `table_options: TableOptions` - PKs, TTL, stream settings, etc.
- Schema versioning, Arrow schema conversion

**These are complementary, not interchangeable!**
- `SystemTable` = table metadata (WHO, WHAT, WHERE, WHEN)
- `TableDefinition` = schema definition (HOW - columns, types, constraints)

## Issues Identified

### 1. ❌ Inconsistent Patterns
- **TableSchemaStore**: Custom EntityStore implementation with special scan methods
- **Other v2 tables**: Standard pattern (store.rs + table.rs + provider.rs)
- **Result**: Harder to understand, maintain, and extend

### 2. ❌ Missing TableProvider
- **TableSchemaStore**: No DataFusion TableProvider implementation
- **Impact**: Cannot query schemas via SQL (e.g., `SELECT * FROM system.schemas`)
- **Comparison**: All other system tables are queryable

### 3. ❌ TODO Comments in Code
```rust
// tables_store.rs line 11:
pub type TablesStore = SystemTableStore<String, SystemTable>; //TODO: Need to use TableId?

// tables_store.rs line 22:
pub fn new_tables_store(backend: Arc<dyn StorageBackend>) -> TablesStore {
    SystemTableStore::new(backend, "system_tables") //TODO: user the enum partition name
}
```

### 4. ⚠️ Key Type Inconsistency
- **TableSchemaStore**: Uses `TableId` (strongly-typed)
- **TablesTableProvider**: Uses `String` (weakly-typed)
- **Best Practice**: Use strongly-typed keys (Phase 14 pattern)

## Recommended Solution

### Option A: Create `schemas_v2/` Module (Recommended)

Follow the same pattern as `tables_v2/`:

```
tables/system/schemas_v2/
├── mod.rs                    # Module exports
├── schemas_store.rs          # Type alias + helper function
├── schemas_table.rs          # Schema definition (OnceLock)
└── schemas_provider.rs       # TableProvider implementation
```

**Benefits:**
1. ✅ Consistent with other v2 system tables
2. ✅ Enables SQL queries: `SELECT * FROM system.schemas`
3. ✅ Clean separation: storage layer vs presentation layer
4. ✅ Easy to understand and maintain

**Implementation:**
```rust
// schemas_store.rs
pub type SchemasStore = SystemTableStore<TableId, TableDefinition>;

pub fn new_schemas_store(backend: Arc<dyn StorageBackend>) -> SchemasStore {
    SystemTableStore::new(backend, "system_table_schemas")
}

// schemas_table.rs
static SCHEMAS_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

pub struct SchemasTableSchema;
impl SchemasTableSchema {
    pub fn schema() -> SchemaRef {
        SCHEMAS_SCHEMA.get_or_init(|| {
            schemas_table_definition().to_arrow_schema()
                .expect("Failed to convert schemas TableDefinition to Arrow schema")
        }).clone()
    }
}

// schemas_provider.rs
pub struct SchemasTableProvider {
    store: SchemasStore,
    schema: SchemaRef,
}

impl SchemasTableProvider {
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_schemas_store(backend),
            schema: SchemasTableSchema::schema(),
        }
    }
    
    // Add scan_namespace() for SchemaRegistry compatibility
    pub fn scan_namespace(&self, namespace_id: &NamespaceId) 
        -> Result<Vec<(TableId, TableDefinition)>, KalamDbError> {
        // Use TableSchemaStore's scan_namespace() logic
    }
}
```

### Option B: Keep Current TableSchemaStore (Not Recommended)

**Reasons Against:**
- ❌ Inconsistent with project patterns
- ❌ No SQL queryability
- ❌ Harder for new developers to understand
- ❌ Already have TODO comments indicating issues

## Migration Path

### Phase 1: Create schemas_v2/ Module
1. Create `schemas_v2/schemas_store.rs` (SystemTableStore wrapper)
2. Create `schemas_v2/schemas_table.rs` (OnceLock schema definition)
3. Create `schemas_v2/schemas_provider.rs` (TableProvider + scan_namespace)
4. Export from `schemas_v2/mod.rs`

### Phase 2: Update SchemaRegistry
1. Replace `TableSchemaStore` with `SchemasTableProvider`
2. Keep existing API (`get_table_definition`, `put_table_definition`, etc.)
3. Add `scan_namespace()` method delegation

### Phase 3: Register in SystemTablesRegistry
1. Add `schemas: Arc<SchemasTableProvider>` field
2. Register as `system.schemas` table
3. Enable SQL queries: `SELECT * FROM system.schemas WHERE namespace = 'default'`

### Phase 4: Cleanup
1. Mark `tables/system/schemas/table_schema_store.rs` as deprecated
2. Remove after migration complete
3. Update documentation

## Additional Fixes Needed

### 1. Fix TablesStore Key Type
```rust
// Current:
pub type TablesStore = SystemTableStore<String, SystemTable>;

// Should be:
pub type TablesStore = SystemTableStore<TableId, SystemTable>;

// Update all usages:
- self.store.put(&table.table_id.to_string(), &table)
+ self.store.put(&table.table_id, &table)
```

### 2. Use Partition Enum (TODO in code)
```rust
// Instead of hardcoded strings:
SystemTableStore::new(backend, "system_tables")

// Use enum from kalamdb-commons:
use kalamdb_commons::system::SystemTable;
SystemTableStore::new(backend, SystemTable::Tables.column_family_name())
```

## Questions for Decision

1. **Do we want `system.schemas` to be queryable via SQL?**
   - If YES → Create schemas_v2/ module (Option A)
   - If NO → Keep current pattern but document why

2. **Should we fix TablesStore to use `TableId` instead of `String`?**
   - Recommended: YES (Phase 14 pattern is strongly-typed keys)

3. **Priority: P0, P1, or P2?**
   - If creating new schema tables soon → P0
   - If just for consistency → P1
   - If low priority → P2

## Summary

The current `TableSchemaStore` works functionally but violates project patterns. Creating a `schemas_v2/` module following the standard v2 pattern would:
- ✅ Enable SQL queries on schema definitions
- ✅ Maintain consistency across codebase
- ✅ Resolve existing TODO comments
- ✅ Make code easier to understand and maintain

**Recommendation: Option A (Create schemas_v2/) - Priority P1**
