# Architecture Comparison: Original vs Unified

## Original Plan (Just Documented)

### Crate Responsibilities
```
kalamdb-sql:
  ✅ Query system tables (7 tables)
  ❌ Query user tables (no SQL support)
  ❌ Query shared tables (no SQL support)
  ❌ Query stream tables (no SQL support)

kalamdb-store:
  ✅ INSERT/UPDATE/DELETE user tables
  ✅ INSERT/UPDATE/DELETE shared tables
  ✅ INSERT/UPDATE/DELETE stream tables
  ✅ SELECT user tables (via scan_user method)
  ✅ SELECT shared tables (via scan method)
  ❌ No SQL support
  
kalamdb-core:
  ✅ Business logic
  ❌ Direct RocksDB imports (after refactoring: removed)
```

### API Experience
```rust
// System tables: Use kalamdb-sql
let users = kalamdb_sql.execute("SELECT * FROM system.users")?;  // ✅ Works

// User tables: Use kalamdb-store methods (NO SQL)
let messages = user_store.scan_user(&ns, &table, &user_id)?;     // ✅ Works
// But can't do:
let messages = kalamdb_sql.execute("SELECT * FROM prod.messages WHERE user_id = ?")?;  // ❌ Error
```

### Problem
- **Fragmented query interface**: SQL for system, K/V for data
- **No unified /api/sql**: Can't query all tables via REST
- **Duplicate logic**: DataFusion for system, custom iteration for data
- **No JOINs**: Can't join system.users with prod.messages

---

## Unified Plan (Your Insight)

### Crate Responsibilities
```
kalamdb-sql:
  ✅ Query ALL tables via SQL (system + user + shared + stream)
  ✅ Route to appropriate TableProvider based on table_type
  ✅ Execute via DataFusion for consistent experience
  ✅ System table CRUD (insert_user, get_namespace, etc.)

kalamdb-store:
  ✅ INSERT/UPDATE/DELETE operations ONLY (write path)
  ✅ Low-level K/V interface for handlers
  ❌ NO read operations (use kalamdb-sql.execute() instead)
  
kalamdb-core:
  ✅ Business logic
  ✅ DML handlers use kalamdb-store for writes
  ✅ Services use kalamdb-sql for queries
  ✅ NO direct RocksDB imports
```

### API Experience
```rust
// ALL tables: Use kalamdb-sql unified interface
let users = kalamdb_sql.execute("SELECT * FROM system.users")?;               // ✅ System
let messages = kalamdb_sql.execute("SELECT * FROM prod.messages WHERE ...")?; // ✅ User
let convs = kalamdb_sql.execute("SELECT * FROM prod.conversations")?;         // ✅ Shared
let events = kalamdb_sql.execute("SELECT * FROM prod.events")?;               // ✅ Stream

// JOINs work across table types
let result = kalamdb_sql.execute("
    SELECT m.*, u.username 
    FROM prod.messages m
    JOIN system.users u ON m.user_id = u.user_id
")?;  // ✅ Works!
```

### Benefits
- ✅ **Unified SQL interface**: One API for all tables
- ✅ **Single /api/sql endpoint**: Works for everything
- ✅ **DataFusion everywhere**: Consistent query optimization
- ✅ **Cross-table JOINs**: System + user/shared/stream tables
- ✅ **Clear separation**: kalamdb-sql (read) + kalamdb-store (write)

---

## Side-by-Side Comparison

| Feature | Original Plan | Unified Plan |
|---------|--------------|--------------|
| **Query system tables** | kalamdb-sql | kalamdb-sql |
| **Query user tables** | kalamdb-store.scan_user() | kalamdb-sql.execute() |
| **Query shared tables** | kalamdb-store.scan() | kalamdb-sql.execute() |
| **Query stream tables** | kalamdb-store.scan() | kalamdb-sql.execute() |
| **INSERT operations** | kalamdb-store.put() | kalamdb-store.put() |
| **UPDATE operations** | kalamdb-store.put() | kalamdb-store.put() |
| **DELETE operations** | kalamdb-store.delete() | kalamdb-store.delete() |
| **SQL support** | System tables only | ALL tables |
| **JOINs across types** | ❌ No | ✅ Yes |
| **Single REST endpoint** | ❌ No | ✅ Yes |
| **DataFusion usage** | System only | ALL tables |

---

## Code Example: Complete Flow

### Write Flow (Same in both)
```rust
// Client: POST /api/sql "INSERT INTO prod.messages VALUES (...)"

// kalamdb-api/src/handlers/sql.rs
kalamdb_sql.execute(sql)?;  // Routes to INSERT handler

// kalamdb-sql/src/executor.rs
match (operation, table_type) {
    (INSERT, TableType::User) => {
        // Delegate to UserTableInsertHandler
        insert_handler.insert_row(namespace, table, user_id, data)?;
    }
}

// kalamdb-core/src/tables/user_table_insert.rs
impl UserTableInsertHandler {
    fn insert_row(...) -> Result<String> {
        // Use kalamdb-store for low-level write
        self.store.put(namespace, table, user_id, row_id, data)?;
    }
}

// kalamdb-store/src/user_table_store.rs
impl UserTableStore {
    fn put(...) -> Result<()> {
        // Write to RocksDB
        let key = format!("{}:{}", user_id, row_id);
        let cf = self.db.cf_handle(&cf_name)?;
        self.db.put_cf(cf, key, value)?;
    }
}
```

### Read Flow (DIFFERENT!)

#### Original Plan (Fragmented)
```rust
// System tables: Use SQL
kalamdb_sql.execute("SELECT * FROM system.users")?;  // ✅ Works

// User tables: Use K/V methods (NO SQL)
user_store.scan_user(&ns, &table, &user_id)?;        // ✅ Works but not SQL

// Can't do:
kalamdb_sql.execute("SELECT * FROM prod.messages")?; // ❌ Error: Not a system table
```

#### Unified Plan (Consistent)
```rust
// ALL tables: Use SQL
kalamdb_sql.execute("SELECT * FROM system.users")?;     // ✅ System table
kalamdb_sql.execute("SELECT * FROM prod.messages")?;    // ✅ User table
kalamdb_sql.execute("SELECT * FROM prod.conversations")?; // ✅ Shared table
kalamdb_sql.execute("SELECT * FROM prod.events")?;      // ✅ Stream table

// kalamdb-sql/src/executor.rs
match table_type {
    TableType::System => {
        let provider = UsersTableProvider::new(kalam_sql.clone());
        context.register_table("system.users", provider)?;
    }
    TableType::User => {
        let provider = UserTableProvider::new(db.clone(), ns, table, user_id);
        context.register_table("prod.messages", provider)?;
    }
    TableType::Shared => {
        let provider = SharedTableProvider::new(db.clone(), ns, table);
        context.register_table("prod.conversations", provider)?;
    }
    TableType::Stream => {
        let provider = StreamTableProvider::new(db.clone(), ns, table, user_id);
        context.register_table("prod.events", provider)?;
    }
}

// Execute via DataFusion
context.sql(sql).await?.collect().await?
```

---

## Your Key Insights Validated

### 1. System ≈ Shared Tables
```
Both have:
- No UserId prefix in keys
- Global visibility
- Similar query patterns

Difference:
- System: Never flush to Parquet
- Shared: Flush to Parquet

Solution:
- kalamdb-sql treats them similarly
- Only flush behavior differs
```

### 2. User ≈ Stream Tables
```
Both have:
- UserId-scoped keys
- Prefix-based isolation
- Similar query patterns

Difference:
- User: Flush to Parquet (durable)
- Stream: TTL eviction (ephemeral)

Solution:
- kalamdb-sql treats them similarly
- Only storage lifetime differs
```

### 3. All Tables Need SQL
```
Users want:
- Unified query interface
- JOINs across table types
- Single /api/sql endpoint

Solution:
- kalamdb-sql handles ALL table types
- kalamdb-store only for writes
- DataFusion for consistent experience
```

---

## Recommendation

**Adopt the Unified Plan** because:

1. ✅ Simpler mental model (SQL everywhere)
2. ✅ Better user experience (one API)
3. ✅ More powerful (cross-table JOINs)
4. ✅ Less code duplication (DataFusion everywhere)
5. ✅ Clear separation (read vs write paths)

**Changes to Phase 9.5 Tasks**:
- kalamdb-store: Remove `get()` and `scan_*()` methods (write-only)
- kalamdb-sql: Add unified `execute()` for all table types
- Update tasks.md to reflect unified SQL architecture

Should I update the plan.md and tasks.md with this unified approach?
