# ADR-009: Three-Layer Architecture for RocksDB Isolation

**Date**: 2025-10-20  
**Status**: Accepted  
**Context**: Phase 1-17 (002-simple-kalamdb)

## Context

In the initial architecture (Phases 1-8), `kalamdb-core` had direct RocksDB access for both system tables and user data. This created several problems:

1. **Tight Coupling**: Business logic mixed with storage implementation details
2. **Testing Difficulty**: Hard to unit test services without spinning up RocksDB
3. **Unclear Boundaries**: No clear separation between metadata operations and data operations
4. **Error Propagation**: RocksDB errors bubbled up to high-level business logic
5. **Inconsistent Patterns**: System tables accessed differently than user/shared/stream tables

Example of problematic direct access:
```rust
// kalamdb-core service directly using RocksDB
impl UserTableService {
    fn insert(&self, table: &str, data: Value) -> Result<()> {
        let db = self.db.clone(); // Direct RocksDB handle
        let cf = db.cf_handle(&format!("user_table:{}", table))?;
        db.put_cf(cf, key, value)?; // Business logic knows RocksDB details
        Ok(())
    }
}
```

## Decision

We will refactor to a **three-layer architecture** with clear boundaries:

```
Layer 1: kalamdb-core (Business Logic)
- Table operations (create, drop, schema evolution)
- Live query management
- Flush/backup services
- DataFusion SQL execution
- NO DIRECT ROCKSDB ACCESS

Layer 2: kalamdb-sql + kalamdb-store (Data Access)
- kalamdb-sql: System tables (namespaces, tables, schemas, storage_locations, jobs, users, live_queries)
- kalamdb-store: User/shared/stream tables (UserTableStore, SharedTableStore, StreamTableStore)
- Abstracts RocksDB column families, key encoding, error handling
- Provides clean API for Layer 1

Layer 3: RocksDB (Persistence)
- Column families for all data
- Key-value storage only
- Only accessed by Layer 2
```

**Key Principle**: `kalamdb-core` **never** directly accesses RocksDB.

## Consequences

### Positive

1. **Clear Separation of Concerns**:
   ```rust
   // kalamdb-core: Business logic only
   impl UserTableService {
       fn insert(&self, namespace: &NamespaceId, table: &TableName, data: Value) -> Result<()> {
           // No RocksDB knowledge
           self.user_table_store.put(namespace, table, row_id, data)?;
           Ok(())
       }
   }
   
   // kalamdb-store: Storage implementation
   impl UserTableStore {
       pub fn put(&self, namespace: &NamespaceId, table: &TableName, ...) -> Result<()> {
           let key = self.encode_key(namespace, table, user_id, row_id);
           let cf = self.get_column_family(table)?;
           self.db.put_cf(cf, key, value)?; // RocksDB details hidden
           Ok(())
       }
   }
   ```

2. **Improved Testability**:
   - Unit test `kalamdb-core` services with mock stores
   - Integration test `kalamdb-store` independently
   - No need to spin up full RocksDB for logic tests

3. **Consistent Metadata Access**:
   ```rust
   // All system table access via kalamdb-sql
   let namespace = kalam_sql.get_namespace(namespace_id)?;
   let table = kalam_sql.get_table(table_id)?;
   let schemas = kalam_sql.get_table_schemas_for_table(table_id)?;
   ```

4. **Better Error Handling**:
   - Layer 2 wraps RocksDB errors with context
   - Layer 1 handles domain-specific errors (TableNotFound, etc.)
   - Clear error boundaries

5. **Easier Refactoring**:
   - Can swap RocksDB for another KV store (only Layer 2 changes)
   - Can optimize store implementation without touching business logic

6. **Enforced Isolation**:
   - `kalamdb-core` cannot accidentally bypass stores
   - All data access auditable at Layer 2

### Negative

1. **More Indirection**: Extra layer between core and storage
   - Mitigation: Minimal performance overhead (function calls, not network)
   - Benefit: Clarity outweighs minor overhead

2. **More Code**: Separate store implementations
   - Cost: ~500 lines per store (UserTableStore, SharedTableStore, StreamTableStore)
   - Benefit: Each store is simple and focused

3. **Learning Curve**: Developers must understand layer boundaries
   - Mitigation: Clear documentation (this ADR + code comments)

### Trade-offs

| Aspect | Direct RocksDB Access | Three-Layer Architecture |
|--------|----------------------|--------------------------|
| **Simplicity** | Fewer files, direct calls | More files, clear boundaries |
| **Testability** | Hard (needs RocksDB) | Easy (mock stores) |
| **Maintainability** | Mixed concerns | Separated concerns |
| **Performance** | Slightly faster | Negligible overhead |
| **Refactoring** | Difficult | Easy (change Layer 2 only) |

## Implementation Details

### Layer 1: kalamdb-core

**Responsibilities**:
- Orchestrate table operations
- Execute SQL via DataFusion
- Manage live queries
- Run flush/backup jobs

**Dependencies**:
- `Arc<KalamSql>` for metadata
- `Arc<UserTableStore>` for user data
- `Arc<SharedTableStore>` for shared data
- `Arc<StreamTableStore>` for stream data

**Example**:
```rust
pub struct UserTableService {
    user_table_store: Arc<UserTableStore>,
    kalam_sql: Arc<KalamSql>,
}

impl UserTableService {
    pub fn create_table(&self, namespace_id: &NamespaceId, table: &Table) -> Result<()> {
        // 1. Validate (business logic)
        self.validate_table_schema(&table.schema)?;
        
        // 2. Create metadata (via kalamdb-sql)
        self.kalam_sql.insert_table(table)?;
        
        // 3. Create storage (via kalamdb-store)
        self.user_table_store.create_column_family(namespace_id, &table.name)?;
        
        Ok(())
    }
}
```

### Layer 2a: kalamdb-sql

**Responsibilities**:
- System table CRUD (namespaces, tables, schemas, etc.)
- Metadata queries
- RocksDB system column families

**API**:
```rust
pub struct KalamSql {
    db: Arc<DB>, // RocksDB handle
}

impl KalamSql {
    // Namespace operations
    pub fn insert_namespace(&self, namespace: &Namespace) -> Result<()>;
    pub fn get_namespace(&self, id: &NamespaceId) -> Result<Option<Namespace>>;
    pub fn scan_all_namespaces(&self) -> Result<Vec<Namespace>>;
    
    // Table operations
    pub fn insert_table(&self, table: &Table) -> Result<()>;
    pub fn get_table(&self, id: &TableId) -> Result<Option<Table>>;
    pub fn scan_all_tables(&self) -> Result<Vec<Table>>;
    
    // Schema operations
    pub fn insert_table_schema(&self, schema: &TableSchema) -> Result<()>;
    pub fn get_table_schemas_for_table(&self, table_id: &TableId) -> Result<Vec<TableSchema>>;
    
    // ... more system table methods
}
```

### Layer 2b: kalamdb-store

**Responsibilities**:
- User/shared/stream table CRUD
- RocksDB data column families
- Key encoding/decoding
- Parquet file operations

**API**:
```rust
pub struct UserTableStore {
    db: Arc<DB>, // RocksDB handle
}

impl UserTableStore {
    // Column family operations
    pub fn create_column_family(&self, namespace: &NamespaceId, table: &TableName) -> Result<()>;
    pub fn drop_column_family(&self, namespace: &NamespaceId, table: &TableName) -> Result<()>;
    
    // Data operations
    pub fn put(&self, namespace: &NamespaceId, table: &TableName, 
               user_id: &UserId, row_id: &str, data: &JsonValue) -> Result<()>;
    pub fn get(&self, namespace: &NamespaceId, table: &TableName,
               user_id: &UserId, row_id: &str) -> Result<Option<JsonValue>>;
    pub fn scan(&self, namespace: &NamespaceId, table: &TableName,
                user_id: &UserId) -> Result<Vec<(String, JsonValue)>>;
    pub fn delete(&self, namespace: &NamespaceId, table: &TableName,
                  user_id: &UserId, row_id: &str) -> Result<()>;
}

// Similar APIs for SharedTableStore and StreamTableStore
```

### Layer 3: RocksDB

**Responsibilities**:
- Persistent key-value storage
- Column families for data organization
- WAL for durability
- Background compaction

**No direct access from Layer 1.**

## Migration Path

The refactoring was completed in phases:

1. **Phase 9.5**: Created `kalamdb-sql` crate for system tables
2. **Phase 10**: Created `kalamdb-store` crate with store implementations
3. **Phase 11-16**: Refactored all `kalamdb-core` services to use stores
4. **Phase 17**: Documented architecture (this ADR)

All direct RocksDB access removed from `kalamdb-core` by Phase 16.

## Alternatives Considered

### 1. Single Data Access Layer

Combine `kalamdb-sql` and `kalamdb-store` into one crate.

**Pros**:
- Fewer crates

**Cons**:
- Mixed metadata and data operations
- Less clear separation

**Rejected**: Clarity improved by splitting system tables (metadata) from user tables (data).

### 2. Repository Pattern

Use repository interfaces with trait-based abstraction.

**Pros**:
- Easily swappable implementations

**Cons**:
- More boilerplate (traits + impls)
- Performance overhead (dynamic dispatch)

**Rejected**: Concrete types sufficient for our use case. RocksDB unlikely to be replaced.

### 3. Service Layer Only

Keep direct RocksDB access, but wrap in service methods.

**Pros**:
- Simpler (no new crates)

**Cons**:
- Still couples business logic to storage
- Hard to test

**Rejected**: Does not achieve separation of concerns goal.

## Related ADRs

- ADR-001: Table-Per-User Architecture (benefits from clean isolation)
- ADR-004: RocksDB Column Families (Layer 2 implementation detail)
- ADR-005: RocksDB-Only Metadata (unified storage at Layer 3)

## References

- [Tasks](../../../specs/002-simple-kalamdb/tasks.md) - Phase 9.5: System Tables Refactor
- [Session Summary](../../../specs/002-simple-kalamdb/progress/SESSION_2025-10-17_SUMMARY.md) - Architecture refactoring discussion
- [README.md](../../README.md) - Three-Layer Architecture Diagram

## Revision History

- 2025-10-20: Initial version (accepted)
