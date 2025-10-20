# DataFusion Catalog Registration - Critical Finding

**Date**: 2025-10-19
**Priority**: HIGH - Blocks all table queries
**Component**: SqlExecutor / Table Services Integration

---

## 🚨 Problem Statement

**Symptom**: CREATE TABLE succeeds, but subsequent INSERT/SELECT queries fail with "table not found"

**Example**:
```bash
# This works:
CREATE TABLE messages (id VARCHAR, content TEXT)
→ {"status": "success", "message": "Table created successfully"}

# This fails:
INSERT INTO messages VALUES ('1', 'Hello')
→ {"status": "error", "message": "table 'kalam.default.messages' not found"}
```

---

## 🔍 Root Cause

KalamDB has a **two-layer table architecture**:

### Layer 1: Persistence (RocksDB) ✅ Working
- Table metadata stored via `kalamdb-store` (UserTableStore, SharedTableStore, StreamTableStore)
- Table services (UserTableService, SharedTableService, StreamTableService) manage persistence
- CREATE TABLE correctly persists to RocksDB

### Layer 2: Query Engine (DataFusion) ❌ Not Connected
- DataFusion maintains an in-memory catalog of available tables
- Each table needs a **TableProvider** registered in the catalog
- Queries can only see tables registered in DataFusion's catalog
- **CREATE TABLE currently only updates Layer 1, not Layer 2**

---

## 📊 Architecture Diagram

```
Current (Broken):
┌─────────────────┐
│  CREATE TABLE   │
└────────┬────────┘
         │
         v
┌─────────────────┐
│ Table Service   │  ✅ Persists to RocksDB
│  (RocksDB)      │
└─────────────────┘
         
         ❌ NOT REGISTERED
         
┌─────────────────┐
│   DataFusion    │  ❌ Catalog doesn't know table exists
│    Catalog      │
└─────────────────┘
         │
         v
┌─────────────────┐
│ INSERT/SELECT   │  ❌ Fails: "table not found"
└─────────────────┘
```

```
Required (Fixed):
┌─────────────────┐
│  CREATE TABLE   │
└────────┬────────┘
         │
         v
┌─────────────────┐
│ Table Service   │  ✅ Persists to RocksDB
│  (RocksDB)      │
└────────┬────────┘
         │
         v
┌─────────────────┐
│ TableProvider   │  ✅ Create provider instance
│   Creation      │
└────────┬────────┘
         │
         v
┌─────────────────┐
│   DataFusion    │  ✅ Register with catalog
│    Catalog      │
└─────────────────┘
         │
         v
┌─────────────────┐
│ INSERT/SELECT   │  ✅ Works: table is queryable
└─────────────────┘
```

---

## 🔧 Solution Overview

After creating a table in RocksDB, we must:

1. **Create a TableProvider** instance (UserTableProvider, SharedTableProvider, or StreamTableProvider)
2. **Register it** with the appropriate DataFusion schema

### Code Location
File: `backend/crates/kalamdb-core/src/sql/executor.rs`
Method: `execute_create_table()` (around line 171)

### Required Changes

#### Current Code (Simplified):
```rust
async fn execute_create_table(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
    let namespace_id = NamespaceId::new("default");
    
    if /* user table pattern */ {
        let stmt = CreateUserTableStatement::parse(sql, &namespace_id)?;
        self.user_table_service.create_table(stmt, None)?;  // ✅ Persists to RocksDB
        return Ok(ExecutionResult::Success("User table created"));
        // ❌ MISSING: Register with DataFusion
    }
    
    // Similar for stream and shared tables...
}
```

#### Required Code (Fixed):
```rust
async fn execute_create_table(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
    let namespace_id = NamespaceId::new("default");
    
    if /* user table pattern */ {
        let stmt = CreateUserTableStatement::parse(sql, &namespace_id)?;
        
        // Step 1: Persist to RocksDB
        self.user_table_service.create_table(stmt.clone(), None)?;
        
        // Step 2: Create TableProvider
        let provider = Arc::new(UserTableProvider::new(
            stmt.table_name.clone(),
            stmt.namespace_id.clone(),
            stmt.schema.clone(),
            self.user_table_service.clone(),  // Or appropriate store
        ));
        
        // Step 3: Register with DataFusion
        let schema = self.session_context
            .catalog("kalam")
            .ok_or(KalamDbError::Internal("Catalog not found".into()))?
            .schema("default")
            .ok_or(KalamDbError::Internal("Schema not found".into()))?;
        
        schema.register_table(stmt.table_name.as_str(), provider)?;
        
        return Ok(ExecutionResult::Success("User table created"));
    }
    
    // Similar for stream and shared tables...
}
```

---

## 📋 Implementation Checklist

### Phase 1: CREATE TABLE Registration
- [ ] Update `execute_create_table()` in `sql/executor.rs`
- [ ] Add TableProvider creation for shared tables
- [ ] Add TableProvider creation for user tables
- [ ] Add TableProvider creation for stream tables
- [ ] Register each provider with appropriate DataFusion schema
- [ ] Test: CREATE TABLE → INSERT → SELECT workflow

### Phase 2: Startup Table Loading
When server starts, existing tables in RocksDB are not loaded into DataFusion.

- [ ] Add `load_existing_tables()` method to SqlExecutor
- [ ] Scan RocksDB for all existing tables (via table stores)
- [ ] Create TableProviders for each
- [ ] Register all with DataFusion catalog
- [ ] Call during server initialization in `main.rs`
- [ ] Test: Restart server → tables still queryable

### Phase 3: DROP TABLE Integration
- [ ] Implement `drop_table()` in UserTableService
- [ ] Implement `drop_table()` in SharedTableService
- [ ] Implement `drop_table()` in StreamTableService
- [ ] Update `execute_drop_table()` to unregister from DataFusion
- [ ] Test: CREATE → DROP → verify both layers removed

---

## 🧪 Testing Strategy

### Test 1: Basic CREATE → INSERT → SELECT
```bash
# Should all succeed:
CREATE TABLE test1 (id VARCHAR, data TEXT)
INSERT INTO test1 VALUES ('1', 'Hello')
SELECT * FROM test1
```

### Test 2: Multiple Tables
```bash
CREATE TABLE test2 (id INT, name VARCHAR)
CREATE TABLE test3 (id INT, value DOUBLE)
SELECT * FROM test2
SELECT * FROM test3
```

### Test 3: Server Restart Persistence
```bash
# Create table and insert data
CREATE TABLE test4 (id VARCHAR)
INSERT INTO test4 VALUES ('persistent')

# Restart server
# Query should still work:
SELECT * FROM test4  # Should return 'persistent'
```

### Test 4: User-Specific Tables
```bash
CREATE TABLE user_data (id VARCHAR) LOCATION '/data/${USER_ID}/data'
INSERT INTO user_data VALUES ('user1-data')
SELECT * FROM user_data
```

### Test 5: Stream Tables
```bash
CREATE TABLE events (id VARCHAR, ts TIMESTAMP) WITH (
    TTL = 3600,
    BUFFER_SIZE = 1000
)
INSERT INTO events VALUES ('evt1', NOW())
SELECT * FROM events
```

---

## 📚 Related Components

### Files to Modify
1. **`backend/crates/kalamdb-core/src/sql/executor.rs`**
   - `execute_create_table()` - add registration
   - `execute_drop_table()` - add unregistration
   - Add `load_existing_tables()` method

2. **`backend/crates/kalamdb-server/src/main.rs`**
   - Call `load_existing_tables()` after SqlExecutor creation
   - Before starting HTTP server

### Files to Reference
3. **`backend/crates/kalamdb-core/src/tables/user_table_provider.rs`**
   - UserTableProvider constructor
   - Check what it needs

4. **`backend/crates/kalamdb-core/src/tables/shared_table_provider.rs`**
   - SharedTableProvider constructor

5. **`backend/crates/kalamdb-core/src/tables/stream_table_provider.rs`**
   - StreamTableProvider constructor (if exists)

### Related Services
6. **Table Stores** (in `kalamdb-store` crate):
   - `UserTableStore` - has methods to scan all user tables
   - `SharedTableStore` - has methods to scan all shared tables
   - `StreamTableStore` - has methods to scan all stream tables

---

## ⚠️ Potential Issues

### Issue 1: TableProvider Constructor Mismatch
**Problem**: TableProvider constructors may need different parameters than what we have in execute_create_table.

**Solution**: Review each TableProvider's `new()` signature and adjust accordingly.

### Issue 2: Schema vs Namespace
**Problem**: DataFusion has schemas, KalamDB has namespaces. May need mapping logic.

**Solution**: For now, map each namespace to a DataFusion schema. Default namespace → "default" schema.

### Issue 3: Concurrent Access
**Problem**: Multiple requests creating tables simultaneously could cause race conditions.

**Solution**: Consider locking or serializing CREATE TABLE operations within a namespace.

---

## 🎯 Success Criteria

Once this is implemented, the following should work end-to-end:

1. ✅ Server starts without panic
2. ✅ CREATE TABLE persists and registers
3. ✅ INSERT works on created tables
4. ✅ SELECT works on created tables
5. ✅ UPDATE works on created tables
6. ✅ DELETE works on created tables
7. ✅ Server restart preserves tables
8. ✅ All three table types (user, shared, stream) work
9. ✅ Quickstart test suite passes

---

## 📝 Related Tasks

- **T227** (COMPLETE): Automated quickstart test script - revealed this issue
- **T228** (PENDING): Run comprehensive benchmarks - blocked by this issue
- **T229** (PENDING): Integration tests - blocked by this issue

---

## 💡 Implementation Priority

**Priority**: **CRITICAL** 🔴
**Effort**: Medium (2-3 hours)
**Impact**: High (unlocks all query operations)
**Dependencies**: None (can implement immediately)

**Recommended Timeline**:
- Phase 1 (CREATE TABLE registration): 1-2 hours
- Phase 2 (Startup loading): 30-60 minutes
- Phase 3 (DROP TABLE): 30-45 minutes
- Testing: 30 minutes

**Total Estimated Time**: 3-4 hours

---

## 🔗 References

- **System Schema Documentation**: `PART1_SYSTEM_SCHEMA_COMPLETE.md`
- **CREATE TABLE Implementation**: `T227_CREATE_TABLE_IMPLEMENTATION_COMPLETE.md`
- **Architecture Spec**: `specs/002-simple-kalamdb/plan.md`
- **Live Query Architecture**: `specs/002-simple-kalamdb/LIVE_QUERY_SUBSCRIPTION_ARCHITECTURE.md`

---

**Document Status**: Active Issue - Requires Implementation
**Last Updated**: 2025-10-19
**Next Review**: After Phase 1 implementation
