# Unified SQL Architecture - Revised Design

**Date**: 2025-10-17  
**Status**: Design Proposal  
**Priority**: HIGH - Simplifies architecture significantly

## The Problem With Current Plan

The architecture I just documented has a **critical flaw**:

### Planned Architecture (Flawed)
```
kalamdb-sql: Handles ONLY system tables (7 tables)
kalamdb-store: Handles ONLY user/shared/stream tables (K/V operations)
```

**Issues**:
1. ❌ Users can't query user/shared/stream tables via SQL
2. ❌ kalamdb-store is just K/V, not integrated with DataFusion
3. ❌ Duplicate query logic: DataFusion for system tables, custom logic for data tables
4. ❌ No unified `/api/sql` endpoint for all tables

### Your Insight: Unified SQL Engine

**Better Architecture**:
```
kalamdb-sql: Unified SQL engine for ALL tables (system + user + shared + stream)
kalamdb-store: Low-level K/V operations for write path (INSERT/UPDATE/DELETE handlers)
```

## Revised Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ kalamdb-core (Business Logic Layer)                             │
│ - DML Handlers use kalamdb-store for writes                     │
│ - Services use kalamdb-sql for queries                          │
│ - NO direct RocksDB imports                                     │
└──────────────┬──────────────────────────────────────────────────┘
               │
               ├────────────────────────┬─────────────────────────┐
               ▼                        ▼                         ▼
┌──────────────────────────┐ ┌──────────────────────┐ ┌──────────────────┐
│ kalamdb-sql              │ │ kalamdb-store        │ │ DataFusion/      │
│ (Unified SQL Engine)     │ │ (K/V Store)          │ │ Arrow/Parquet    │
├──────────────────────────┤ ├──────────────────────┤ └──────────────────┘
│ Purpose:                 │ │ Purpose:             │
│ - Query ALL tables       │ │ - Write operations   │
│ - System metadata CRUD   │ │ - Low-level K/V      │
│ - User/shared/stream SQL │ │ - INSERT/UPDATE/DEL  │
│                          │ │                      │
│ Tables Handled:          │ │ Used By:             │
│ - system.users           │ │ - INSERT handlers    │
│ - system.namespaces      │ │ - UPDATE handlers    │
│ - system.tables          │ │ - DELETE handlers    │
│ - system.table_schemas   │ │ - Flush jobs         │
│ - system.storage_locs    │ │                      │
│ - system.live_queries    │ │ NOT used for:        │
│ - system.jobs            │ │ - SELECT queries     │
│ - {ns}.{user_table}      │ │   (use kalamdb-sql)  │
│ - {ns}.{shared_table}    │ │                      │
│ - {ns}.{stream_table}    │ │                      │
└──────────────┬───────────┘ └──────────┬───────────┘
               │                        │
               └────────────┬───────────┘
                            ▼
                  ┌───────────────────┐
                  │ RocksDB + Parquet │
                  │ (Storage Layer)   │
                  └───────────────────┘
```

## Key Differences From Previous Plan

### 1. kalamdb-sql Responsibilities (EXPANDED)

**Before** (narrow):
```rust
// kalamdb-sql: ONLY system tables
kalamdb_sql.execute("SELECT * FROM system.users")?;  // ✅ Works
kalamdb_sql.execute("SELECT * FROM prod.messages")?; // ❌ Error: Not a system table
```

**After** (unified):
```rust
// kalamdb-sql: ALL tables via unified SQL engine
kalamdb_sql.execute("SELECT * FROM system.users")?;      // ✅ System table
kalamdb_sql.execute("SELECT * FROM prod.messages")?;     // ✅ User table
kalamdb_sql.execute("SELECT * FROM prod.conversations")?;// ✅ Shared table
kalamdb_sql.execute("SELECT * FROM prod.events")?;       // ✅ Stream table
```

### 2. kalamdb-store Responsibilities (NARROWED)

**Before** (confused):
```rust
// kalamdb-store: Used for both writes AND reads
store.put(namespace, table, user_id, row_id, data)?;  // ✅ Write
store.get(namespace, table, user_id, row_id)?;        // ❌ Why? Use SQL!
store.scan_user(namespace, table, user_id)?;          // ❌ Why? Use SQL!
```

**After** (focused):
```rust
// kalamdb-store: ONLY write operations (K/V interface)
store.put(namespace, table, user_id, row_id, data)?;     // ✅ Write path
store.delete(namespace, table, user_id, row_id, hard)?;  // ✅ Write path

// For reads: Use kalamdb-sql instead
kalamdb_sql.execute("SELECT * FROM prod.messages WHERE user_id = ?")?;  // ✅ Read path
```

## Unified Flow Examples

### Write Flow (INSERT)
```
Client: POST /api/sql "INSERT INTO prod.messages (...)"
    ↓
kalamdb-sql: Parse SQL → Extract table/values
    ↓
kalamdb-sql: Lookup table metadata (system.tables)
    ↓
kalamdb-sql: Route to appropriate handler based on table_type
    ↓
UserTableInsertHandler: Use kalamdb-store.put()
    ↓
kalamdb-store: Write to RocksDB with key {UserId}:{row_id}
    ↓
Response: {"rows_inserted": 1}
```

### Read Flow (SELECT)
```
Client: POST /api/sql "SELECT * FROM prod.messages WHERE ..."
    ↓
kalamdb-sql: Parse SQL → Extract table/filters
    ↓
kalamdb-sql: Lookup table metadata (system.tables)
    ↓
kalamdb-sql: Create DataFusion TableProvider (UserTableProvider)
    ↓
UserTableProvider: Scan RocksDB + Parquet using DataFusion
    ↓
kalamdb-sql: Execute query via DataFusion
    ↓
Response: RecordBatch as JSON
```

### System Query Flow
```
Client: POST /api/sql "SELECT * FROM system.users"
    ↓
kalamdb-sql: Parse SQL → Recognize system table
    ↓
kalamdb-sql: Create SystemTableProvider (UsersTableProvider)
    ↓
UsersTableProvider: Scan system_users CF directly
    ↓
kalamdb-sql: Execute query via DataFusion
    ↓
Response: RecordBatch as JSON
```

## Revised Crate Structures

### kalamdb-sql (Unified SQL Engine)

```rust
// backend/crates/kalamdb-sql/src/lib.rs

pub struct KalamSql {
    db: Arc<DB>,
    catalog: SessionContext,  // DataFusion catalog
}

impl KalamSql {
    /// Execute any SQL query against any table
    pub fn execute(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        // 1. Parse SQL
        // 2. Extract table name
        // 3. Lookup table type from system.tables
        // 4. Create appropriate TableProvider:
        //    - System table → SystemTableProvider
        //    - User table → UserTableProvider
        //    - Shared table → SharedTableProvider
        //    - Stream table → StreamTableProvider
        // 5. Execute via DataFusion
        // 6. Return results
    }
    
    // Convenience methods for system tables (direct K/V access)
    pub fn insert_user(&self, user: &User) -> Result<()>;
    pub fn get_user(&self, user_id: &str) -> Result<Option<User>>;
    pub fn scan_all_users(&self) -> Result<Vec<User>>;
    // ... same for other 6 system tables
}
```

### kalamdb-store (Write-Only K/V Store)

```rust
// backend/crates/kalamdb-store/src/lib.rs

pub struct UserTableStore {
    db: Arc<DB>,
}

impl UserTableStore {
    /// Write a row (INSERT or UPDATE)
    pub fn put(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
        row_data: JsonValue,
    ) -> Result<()>;
    
    /// Delete a row (soft or hard)
    pub fn delete(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
        hard: bool,
    ) -> Result<()>;
    
    // NO get() or scan() methods - use kalamdb-sql for reads!
}

pub struct SharedTableStore {
    db: Arc<DB>,
}

impl SharedTableStore {
    pub fn put(...) -> Result<()>;
    pub fn delete(...) -> Result<()>;
}

pub struct StreamTableStore {
    db: Arc<DB>,
}

impl StreamTableStore {
    pub fn put(...) -> Result<()>;
    pub fn delete(...) -> Result<()>;
}
```

### kalamdb-core (Business Logic)

```rust
// backend/crates/kalamdb-core/src/tables/user_table_insert.rs

use kalamdb_store::UserTableStore;

pub struct UserTableInsertHandler {
    store: Arc<UserTableStore>,  // For writes
}

impl UserTableInsertHandler {
    pub fn insert_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_data: JsonValue,
    ) -> Result<String> {
        let row_id = self.generate_row_id()?;
        
        // Use kalamdb-store for write
        self.store.put(namespace_id, table_name, user_id, &row_id, row_data)?;
        
        Ok(row_id)
    }
}

// backend/crates/kalamdb-core/src/tables/user_table_provider.rs

use kalamdb_sql::KalamSql;

pub struct UserTableProvider {
    kalam_sql: Arc<KalamSql>,  // For metadata lookups
    db: Arc<DB>,               // For direct RocksDB scans in DataFusion
    namespace_id: NamespaceId,
    table_name: TableName,
}

impl TableProvider for UserTableProvider {
    fn scan(&self, ...) -> Result<...> {
        // Scan RocksDB + Parquet for this user table
        // This is called BY kalamdb-sql during query execution
    }
}
```

## Benefits of Unified Architecture

### 1. Single SQL Entry Point
```rust
// ONE API for ALL tables
kalamdb_sql.execute(sql) → Works for system/user/shared/stream
```

### 2. Consistent Query Experience
```sql
-- All tables queryable via SQL
SELECT * FROM system.users;
SELECT * FROM prod.messages;
SELECT * FROM prod.conversations;
SELECT * FROM prod.events;

-- JOINs work across table types
SELECT m.*, u.username 
FROM prod.messages m
JOIN system.users u ON m.user_id = u.user_id;
```

### 3. Clear Separation of Concerns
- **kalamdb-sql**: Query engine (SELECT for all tables + system CRUD)
- **kalamdb-store**: Write engine (INSERT/UPDATE/DELETE for data tables)
- **kalamdb-core**: Business logic (orchestration)

### 4. Simplified API Layer
```rust
// backend/crates/kalamdb-api/src/handlers/sql.rs

pub async fn execute_sql(
    sql: String,
    kalam_sql: Arc<KalamSql>,
) -> Result<HttpResponse> {
    // Just delegate to kalamdb-sql
    let results = kalam_sql.execute(&sql)?;
    Ok(HttpResponse::Ok().json(results))
}
```

## Similarities You Identified

### System Tables ≈ Shared Tables
**Similarities**:
- Both have **no UserId** in key (global data)
- Both queryable by **all users**
- Both use **same RocksDB pattern** (key = row_id)

**Differences**:
- System tables: No flush to Parquet (always hot)
- Shared tables: Flush to Parquet (hybrid storage)

**Unified Handling**:
```rust
// kalamdb-sql can treat them similarly for queries
match table_type {
    TableType::System => SystemTableProvider::new(cf_name),
    TableType::Shared => SharedTableProvider::new(cf_name),
    // Both scan CF without UserId prefix
}
```

### User Tables ≈ Stream Tables
**Similarities**:
- Both have **UserId-scoped** data
- Both use **prefix keys** for isolation
- Both queryable via **same SQL interface**

**Differences**:
- User tables: Flush to Parquet, durable
- Stream tables: TTL eviction, ephemeral

**Unified Handling**:
```rust
// kalamdb-sql can treat them similarly for queries
match table_type {
    TableType::User => UserTableProvider::new(cf_name, user_id_filter),
    TableType::Stream => StreamTableProvider::new(cf_name, user_id_filter),
    // Both scan CF with UserId prefix
}
```

## Updated Refactoring Tasks

### Changes to Phase 9.5 Tasks

**Step A: Create kalamdb-store** - ✅ Keep as-is
- But remove `get()` and `scan_user()` methods (use kalamdb-sql for reads)

**Step B: Enhance kalamdb-sql** - 🔄 Expand scope
- Add unified execute() method for all tables
- Add table routing logic (system vs user vs shared vs stream)
- Keep scan_all_* methods for system tables

**Step C: Refactor System Providers** - ✅ Keep as-is
- Still use kalamdb-sql

**Step D: Refactor User Handlers** - 🔄 Update
- Insert/Update/Delete handlers use kalamdb-store (write-only)
- UserTableProvider uses DataFusion (read-only)

**Step E: Verify** - ✅ Keep as-is

## Questions for You

1. **Write Path**: Should INSERT/UPDATE/DELETE go through kalamdb-sql.execute() or directly to handlers?
   - Option A: `kalamdb_sql.execute("INSERT INTO ...")` → routes to handler → kalamdb_store.put()
   - Option B: `insert_handler.insert_row(...)` → kalamdb_store.put() directly

2. **Read Path**: Should UserTableProvider access RocksDB directly or through kalamdb-store?
   - Option A: UserTableProvider scans RocksDB directly (current DataFusion pattern)
   - Option B: UserTableProvider calls kalamdb-store (extra abstraction layer)

3. **System vs Shared**: Should we merge system and shared table handling?
   - They're almost identical except flush behavior

Let me know your thoughts and I'll update the plan.md and tasks.md accordingly!
