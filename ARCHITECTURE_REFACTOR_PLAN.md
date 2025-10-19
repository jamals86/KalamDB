# Architecture Refactoring Plan: Three-Layer Separation

**Date**: 2025-10-17  
**Status**: Planned (Not Yet Implemented)  
**Priority**: High (Complete after Phase 9)

## Problem Statement

Current architecture has **direct RocksDB coupling** in multiple places:

1. System table providers use `CatalogStore` (direct RocksDB operations)
2. User table handlers (INSERT/UPDATE/DELETE) have `rocksdb::DB` imports
3. No clear separation between system metadata and user data operations

## Proposed Solution: Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ kalamdb-core (Business Logic)                               │
│ - NO direct RocksDB imports                                 │
│ - Uses kalamdb-sql for system tables                        │
│ - Uses kalamdb-store for user tables                        │
└──────────────┬──────────────────────────────────────────────┘
               │
               ├──────────────────┬───────────────────────────┐
               ▼                  ▼                           ▼
┌──────────────────────┐ ┌─────────────────────┐ ┌───────────────────┐
│ kalamdb-sql          │ │ kalamdb-store       │ │ DataFusion/       │
│ (System Tables)      │ │ (User Tables)       │ │ Arrow/Parquet     │
├──────────────────────┤ ├─────────────────────┤ └───────────────────┘
│ - 7 system tables    │ │ - User table K/V    │
│ - SQL interface      │ │ - Simple put/get    │
│ - RocksDB adapter    │ │ - Key: UserId:rowid │
└──────────┬───────────┘ └──────────┬──────────┘
           │                        │
           └────────────┬───────────┘
                        ▼
              ┌───────────────────┐
              │ RocksDB           │
              └───────────────────┘
```

## Benefits

1. **Clear Separation**: System metadata vs user data operations
2. **RocksDB Isolation**: Only 2 crates import RocksDB
3. **Easier Testing**: Mock interfaces instead of RocksDB
4. **Code Clarity**: Intent clear from which crate is used
5. **Future Flexibility**: Can swap storage backends independently

## Implementation Plan

### **Step 1: Create kalamdb-store Crate**

**Location**: `backend/crates/kalamdb-store/`

**Files to Create**:
- `Cargo.toml`
- `src/lib.rs` - Public API exports
- `src/user_table_store.rs` - K/V operations for user tables
- `src/shared_table_store.rs` - K/V operations for shared tables
- `src/stream_table_store.rs` - K/V operations for stream tables
- `src/key_encoding.rs` - Key format utilities

**Public API**:
```rust
pub struct UserTableStore {
    db: Arc<DB>,
}

impl UserTableStore {
    pub fn new(db: Arc<DB>) -> Result<Self>;
    
    pub fn put(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
        row_data: JsonValue,
    ) -> Result<()>;
    
    pub fn get(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
    ) -> Result<Option<JsonValue>>;
    
    pub fn delete(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_id: &str,
        hard: bool,
    ) -> Result<()>;
    
    pub fn scan_user(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
    ) -> Result<impl Iterator<Item = (String, JsonValue)>>;
}
```

**Dependencies**:
```toml
[dependencies]
rocksdb = "0.24"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4.39"
anyhow = "1.0"
```

**Tests**: Comprehensive unit tests with temporary RocksDB instances

---

### **Step 2: Add scan_all Methods to kalamdb-sql**

**Files to Update**:
- `backend/crates/kalamdb-sql/src/adapter.rs`
- `backend/crates/kalamdb-sql/src/lib.rs`

**New Methods**:
```rust
// adapter.rs
impl RocksDbAdapter {
    pub fn scan_all_users(&self) -> Result<Vec<User>>;
    pub fn scan_all_namespaces(&self) -> Result<Vec<Namespace>>;
    pub fn scan_all_storage_locations(&self) -> Result<Vec<StorageLocation>>;
    pub fn scan_all_live_queries(&self) -> Result<Vec<LiveQuery>>;
    pub fn scan_all_jobs(&self) -> Result<Vec<Job>>;
    pub fn scan_all_tables(&self) -> Result<Vec<Table>>;
    pub fn scan_all_table_schemas(&self) -> Result<Vec<TableSchema>>;
}

// lib.rs
impl KalamSql {
    pub fn scan_all_users(&self) -> Result<Vec<User>>;
    pub fn scan_all_namespaces(&self) -> Result<Vec<Namespace>>;
    // ... etc for all 7 system tables
}
```

**Implementation Pattern**:
```rust
pub fn scan_all_users(&self) -> Result<Vec<User>> {
    let cf = self.db.cf_handle("system_users")
        .ok_or_else(|| anyhow!("Column family not found: system_users"))?;
    
    let mut users = Vec::new();
    
    for item in self.db.iterator_cf(cf, rocksdb::IteratorMode::Start) {
        let (key, value) = item?;
        let user: User = serde_json::from_slice(&value)?;
        users.push(user);
    }
    
    Ok(users)
}
```

---

### **Step 3: Refactor System Table Providers**

**Files to Update**:
- `backend/crates/kalamdb-core/src/tables/system/users_provider.rs`
- `backend/crates/kalamdb-core/src/tables/system/storage_locations_provider.rs`
- `backend/crates/kalamdb-core/src/tables/system/live_queries_provider.rs`
- `backend/crates/kalamdb-core/src/tables/system/jobs_provider.rs`

**Changes**:

**Before**:
```rust
use crate::catalog::CatalogStore;

pub struct UsersTableProvider {
    catalog_store: Arc<CatalogStore>,
}

impl UsersTableProvider {
    pub fn new(catalog_store: Arc<CatalogStore>) -> Self {
        Self { catalog_store, schema: UsersTable::schema() }
    }
    
    pub fn insert_user(&self, user: UserRecord) -> Result<()> {
        self.catalog_store.put_user(&user_id, &user)
    }
}
```

**After**:
```rust
use kalamdb_sql::KalamSql;

pub struct UsersTableProvider {
    kalam_sql: Arc<KalamSql>,
}

impl UsersTableProvider {
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self { kalam_sql, schema: UsersTable::schema() }
    }
    
    pub fn insert_user(&self, user: User) -> Result<()> {
        self.kalam_sql.insert_user(&user)
    }
    
    pub fn scan_all_users(&self) -> Result<Vec<User>> {
        self.kalam_sql.scan_all_users()
    }
}
```

**Impact**: 4 provider files, approximately 200 lines changed per file

---

### **Step 4: Refactor User Table Handlers**

**Files to Update**:
- `backend/crates/kalamdb-core/src/tables/user_table_insert.rs`
- `backend/crates/kalamdb-core/src/tables/user_table_update.rs`
- `backend/crates/kalamdb-core/src/tables/user_table_delete.rs`

**Changes**:

**Before**:
```rust
use rocksdb::DB;

pub struct UserTableInsertHandler {
    db: Arc<DB>,
}

impl UserTableInsertHandler {
    pub fn insert_row(...) -> Result<String> {
        let cf_name = ColumnFamilyManager::column_family_name(...);
        let cf = self.db.cf_handle(&cf_name)?;
        
        // Inject system columns
        // Serialize
        // Write to RocksDB
        self.db.put_cf(cf, key.as_bytes(), &value_bytes)?;
    }
}
```

**After**:
```rust
use kalamdb_store::UserTableStore;

pub struct UserTableInsertHandler {
    store: Arc<UserTableStore>,
}

impl UserTableInsertHandler {
    pub fn insert_row(...) -> Result<String> {
        let row_id = self.generate_row_id()?;
        
        // kalamdb-store handles:
        // - System column injection
        // - CF lookup
        // - Key encoding
        self.store.put(namespace_id, table_name, user_id, &row_id, row_data)?;
        
        Ok(row_id)
    }
}
```

**Impact**: 3 handler files, approximately 100 lines simplified per file

---

### **Step 5: Update Dependency Injection**

**Files to Update**:
- `backend/crates/kalamdb-server/src/main.rs`
- `backend/crates/kalamdb-core/src/services/namespace_service.rs`
- `backend/crates/kalamdb-core/src/services/user_table_service.rs`

**Before**:
```rust
// main.rs
let db = Arc::new(DB::open_cf(&opts, db_path, &cf_names)?);
let catalog_store = Arc::new(CatalogStore::new(db.clone()));
let namespace_service = Arc::new(NamespaceService::new(catalog_store.clone()));
```

**After**:
```rust
// main.rs
let db = Arc::new(DB::open_cf(&opts, db_path, &cf_names)?);
let kalam_sql = Arc::new(KalamSql::new(db.clone())?);
let user_store = Arc::new(UserTableStore::new(db.clone())?);

let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
let insert_handler = Arc::new(UserTableInsertHandler::new(user_store.clone()));
```

---

### **Step 6: Deprecate CatalogStore**

**Files to Update**:
- `backend/crates/kalamdb-core/src/catalog/catalog_store.rs`

**Changes**:
```rust
#[deprecated(
    since = "0.2.0",
    note = "Use kalamdb-sql for system tables and kalamdb-store for user tables"
)]
pub struct CatalogStore {
    db: Arc<DB>,
}
```

**Migration Guide**: Add to documentation explaining how to migrate

---

## Testing Strategy

### **Unit Tests**

1. **kalamdb-store tests**:
   - Test put/get/delete for all table types
   - Test scan_user with prefix filtering
   - Test system column injection
   - Test soft delete vs hard delete

2. **kalamdb-sql tests**:
   - Test scan_all methods for all 7 system tables
   - Test iterator-based scanning
   - Test empty table handling

3. **Integration tests**:
   - Test full flow: business logic → kalamdb-sql → RocksDB
   - Test full flow: business logic → kalamdb-store → RocksDB

### **Migration Tests**

Create tests that verify:
- Old CatalogStore behavior matches new kalamdb-sql behavior
- Old direct RocksDB writes match new kalamdb-store behavior

---

## Rollout Plan

### **Phase A: Create New Crates (Non-Breaking)**
1. Create kalamdb-store crate
2. Add scan_all methods to kalamdb-sql
3. Add comprehensive tests
4. **No changes to kalamdb-core yet**

### **Phase B: Refactor System Tables**
1. Update 4 system table providers
2. Update main.rs initialization
3. Update tests
4. **User table handlers unchanged**

### **Phase C: Refactor User Tables**
1. Update 3 user table handlers
2. Update main.rs initialization
3. Update tests
4. **All RocksDB imports removed from kalamdb-core**

### **Phase D: Cleanup**
1. Deprecate CatalogStore
2. Update documentation
3. Remove CatalogStore in future version

---

## Timeline Estimate

- **Phase A**: 2-3 days (create kalamdb-store, add scan_all)
- **Phase B**: 2-3 days (refactor system table providers)
- **Phase C**: 1-2 days (refactor user table handlers)
- **Phase D**: 1 day (deprecation and docs)

**Total**: 6-9 days of development work

---

## Success Criteria

- [ ] kalamdb-store crate exists with full test coverage
- [ ] All 7 system tables have scan_all methods in kalamdb-sql
- [ ] All 4 system table providers use kalamdb-sql only
- [ ] All 3 user table handlers use kalamdb-store only
- [ ] kalamdb-core has ZERO direct rocksdb imports
- [ ] All existing tests pass
- [ ] CatalogStore marked as deprecated
- [ ] Documentation updated

---

## Notes

- This refactoring improves code quality but does NOT change functionality
- Can be done incrementally without breaking existing features
- Should be completed before production deployment
- Priority: Complete after Phase 9 (user table operations)

---

## Related Documents

- **Spec Update**: `specs/002-simple-kalamdb/spec.md` (Section: "Unified SQL Engine for System Tables")
- **Clarifications**: `specs/002-simple-kalamdb/spec.md` (Session 2025-10-17, last 2 entries)
- **Current Implementation**: Phase 9 tasks T123-T125 completed (INSERT/UPDATE/DELETE handlers with direct RocksDB)
