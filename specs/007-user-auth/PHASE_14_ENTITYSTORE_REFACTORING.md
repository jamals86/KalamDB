# Phase 14: EntityStore Architecture Refactoring

**Status**: Planned  
**Priority**: P0 - Architectural Foundation  
**Estimated Effort**: Large (3-5 days)  
**Dependencies**: Phase 13 (Generic Index Infrastructure)

## Overview

Refactor all table storage implementations to use a unified `EntityStore<K, V>` trait with type-safe keys, eliminating ~600+ lines of duplicated CRUD code across `SharedTableStore`, `UserTableStore`, `StreamTableStore`, and system table providers.

## Motivation

### Current Problems

1. **Code Duplication**: 600+ lines of near-identical CRUD code across 4 different stores:
   - `SharedTableStore` (~200 lines of CRUD)
   - `UserTableStore` (~200 lines of CRUD)
   - `StreamTableStore` (~200 lines of CRUD + TTL)
   - System table providers (manual RocksDB access in each provider)

2. **Inconsistent APIs**: Each store has slightly different method signatures:
   ```rust
   // SharedTableStore
   fn put(&self, namespace_id: &str, table_name: &str, row_id: &str, data: JsonValue)
   
   // UserTableStore
   fn put(&self, namespace_id: &str, table_name: &str, user_id: &str, row_id: &str, data: JsonValue)
   
   // System tables
   fn insert_user(&self, user: CreateUserRequest) // Different per table!
   ```

3. **Lack of Type Safety**: All keys are strings, easy to pass wrong parameters:
   ```rust
   // Current: Easy to make mistakes
   store.get("namespace", "table", "user123")  // Which param is which?
   
   // Desired: Compile-time safety
   store.get(&TableKey { namespace, table, row_id })
   ```

4. **No Shared Behavior**: System tables and shared tables both:
   - Are cross-user (not user-isolated)
   - Use simple keys (not composite)
   - Need indexes
   - But have zero code sharing

### Proposed Solution

```rust
// 1. Generic base trait with type-safe keys
pub trait EntityStore<K, V> 
where
    K: AsRef<[u8]> + Clone + Send + Sync,
    V: Serialize + Deserialize + Send + Sync
{
    fn backend(&self) -> &Arc<dyn StorageBackend>;
    fn partition(&self) -> &str;
    
    // Provided methods (automatic serialization)
    fn put(&self, key: &K, value: &V) -> Result<()> { ... }
    fn get(&self, key: &K) -> Result<Option<V>> { ... }
    fn delete(&self, key: &K) -> Result<()> { ... }
    fn scan_prefix(&self, prefix: &K) -> Result<Vec<(K, V)>> { ... }
    fn scan_all(&self) -> Result<Vec<(K, V)>> { ... }
}

// 2. Specialized trait for cross-user tables (system + shared)
pub trait CrossUserTableStore<K, V>: EntityStore<K, V> {
    fn table_access(&self) -> Option<TableAccess>;
    
    fn can_read(&self, user_role: &Role) -> bool {
        match self.table_access() {
            None => user_role >= Role::Dba,  // System tables
            Some(TableAccess::Public) => true,
            Some(TableAccess::Private) => false,
            Some(TableAccess::Restricted) => user_role >= Role::Service,
        }
    }
}

// 3. Type-safe key models (like UserId)
pub struct RowId(Vec<u8>);        // For shared/stream tables
pub struct UserRowId {            // For user tables
    user_id: UserId,
    row_id: Vec<u8>,
}
pub struct TableMetadataId(String);  // For system.tables
pub struct JobId(String);             // For system.jobs
pub struct NamespaceKey(NamespaceId); // For system.namespaces
```

## Architecture

### Before (Current)

```text
SharedTableStore          UserTableStore          StreamTableStore         UsersTableProvider
├─ put() [200 lines]      ├─ put() [200 lines]    ├─ put() [200 lines]     ├─ insert_user()
├─ get()                  ├─ get()                ├─ get()                 ├─ update_user()
├─ delete()               ├─ delete()             ├─ delete()              ├─ delete_user()
├─ scan()                 ├─ scan()               ├─ scan()                └─ get_user()
└─ ... [duplicated]       └─ ... [duplicated]     └─ ... [duplicated + TTL]

Each reimplements serialization, partition management, error handling
```

### After (Proposed)

```text
EntityStore<K, V> Trait (50 lines)
├─ Automatic serialization
├─ Partition management
├─ Error handling
└─ Scan operations

CrossUserTableStore<K, V> Trait (30 lines)
└─ Access control logic

Implementations (100 lines each):
├─ SystemTableStore<V> implements EntityStore<Vec<u8>, V>
│   └─ Used by: users, tables, jobs, namespaces, storages, live_queries
├─ SharedTableStoreImpl implements CrossUserTableStore<RowId, SharedTableRow>
├─ UserTableStoreImpl implements EntityStore<UserRowId, UserTableRow>
└─ StreamTableStoreImpl implements EntityStore<RowId, StreamTableRow> + TTL logic
```

**Code Reduction**: 
- Before: ~800 lines (4 stores × 200 lines)
- After: ~400 lines (trait + 4 implementations)
- **Savings: 50% reduction, ~400 lines eliminated**

## Type-Safe Key Models

All key models will be placed in `backend/crates/kalamdb-commons/src/models/`:

### 1. RowId (for shared/stream tables)

```rust
// File: kalamdb-commons/src/models/row_id.rs

/// Type-safe row identifier for shared and stream tables.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RowId(Vec<u8>);

impl RowId {
    pub fn new(id: impl Into<Vec<u8>>) -> Self { ... }
    pub fn as_bytes(&self) -> &[u8] { ... }
    pub fn from_string(s: &str) -> Self { ... }
}

impl AsRef<[u8]> for RowId {
    fn as_ref(&self) -> &[u8] { &self.0 }
}
```

### 2. UserRowId (for user tables)

```rust
// File: kalamdb-commons/src/models/user_row_id.rs

/// Composite key for user-scoped table rows: {user_id}:{row_id}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct UserRowId {
    user_id: UserId,
    row_id: Vec<u8>,
}

impl UserRowId {
    pub fn new(user_id: UserId, row_id: Vec<u8>) -> Self { ... }
    pub fn user_id(&self) -> &UserId { ... }
    pub fn row_id(&self) -> &[u8] { ... }
}

impl AsRef<[u8]> for UserRowId {
    fn as_ref(&self) -> &[u8] {
        // Format: "{user_id}:{row_id}"
        ...
    }
}
```

### 3. System Table Keys

```rust
// File: kalamdb-commons/src/models/table_id.rs
pub struct TableId {  // For system.tables (composite: namespace:table)
    namespace_id: NamespaceId,
    table_name: TableName,
}

// File: kalamdb-commons/src/models/job_id.rs
pub struct JobId(String);  // For system.jobs

// File: kalamdb-commons/src/models/user_name.rs
pub struct UserName(String);  // For system.users secondary index (username → UserId)

// NOTE: system.users uses existing UserId as primary key
// NOTE: system.namespaces uses existing NamespaceId directly (no wrapper needed)
// NOTE: system.storages uses existing StorageId directly (no wrapper needed)

// File: kalamdb-commons/src/models/live_query_id.rs
pub struct LiveQueryId(String);  // For system.live_queries

// File: kalamdb-commons/src/models/table_schema_id.rs
pub struct TableSchemaId {  // For system.table_schemas
    namespace_id: NamespaceId,
    table_name: TableName,
    version: u32,
}
```

## New Folder Structure

### Tables Directory Organization

```
backend/crates/kalamdb-core/src/tables/
├── system/                           # System tables (cross-user, admin-only)
│   ├── users/                        # system.users table
│   │   ├── users_provider.rs         # UsersTableProvider (DataFusion integration)
│   │   ├── users_store.rs            # SystemTableStore<UserId, User>
│   │   ├── users_table.rs            # Table schema model (columns, constraints)
│   │   └── users_username_index.rs   # SecondaryIndex<User, UserName> (username → UserId)
│   │
│   ├── tables/                       # system.tables table
│   │   ├── tables_provider.rs        # SystemTablesTableProvider
│   │   ├── tables_store.rs           # SystemTableStore<TableId, TableMetadata>
│   │   └── tables_table.rs           # Table schema model
│   │
│   ├── jobs/                         # system.jobs table
│   │   ├── jobs_provider.rs          # JobsTableProvider
│   │   ├── jobs_store.rs             # SystemTableStore<JobId, Job>
│   │   └── jobs_table.rs             # Table schema model
│   │
│   ├── namespaces/                   # system.namespaces table
│   │   ├── namespaces_provider.rs    # NamespacesTableProvider
│   │   ├── namespaces_store.rs       # SystemTableStore<NamespaceId, Namespace>
│   │   └── namespaces_table.rs       # Table schema model
│   │
│   ├── storages/                     # system.storages table
│   │   ├── storages_provider.rs      # StoragesTableProvider
│   │   ├── storages_store.rs         # SystemTableStore<StorageId, Storage>
│   │   └── storages_table.rs         # Table schema model
│   │
│   └── live_queries/                 # system.live_queries table
│       ├── live_queries_provider.rs  # LiveQueriesTableProvider
│       ├── live_queries_store.rs     # SystemTableStore<LiveQueryId, LiveQuery>
│       └── live_queries_table.rs     # Table schema model
│
├── user/                             # User-isolated tables
│   ├── user_table_provider.rs        # UserTableProvider (DataFusion integration)
│   └── user_table_store.rs           # UserTableStoreImpl: EntityStore<UserRowId, UserTableRow>
│
├── shared/                           # Cross-user shared tables
│   ├── shared_table_provider.rs      # SharedTableProvider (DataFusion integration)
│   └── shared_table_store.rs         # SharedTableStoreImpl: EntityStore<RowId, SharedTableRow>
│                                     #                      + CrossUserTableStore<RowId, SharedTableRow>
│
└── stream/                           # Time-series stream tables
    ├── stream_table_provider.rs      # StreamTableProvider (DataFusion integration)
    └── stream_table_store.rs         # StreamTableStoreImpl: EntityStore<RowId, StreamTableRow>
```

**Benefits of This Structure**:
1. **Clear Separation**: System/User/Shared/Stream tables are visually separated
2. **Modular**: Each system table is self-contained in its own folder
3. **Scalable**: Easy to add new system tables without cluttering
4. **Consistent**: Same pattern for all table types
5. **Discoverable**: Table schema models are co-located with providers
6. **Index Organization**: Secondary indexes live with their table (e.g., `users_username_index.rs`)

## Implementation Plan

### Step 1: Create Type-Safe Key Models (T180-T186)

Create 6 new key type wrappers in `kalamdb-commons/src/models/`:
- `RowId` - for shared/stream tables
- `UserRowId` - composite key for user tables  
- `TableId` - composite key for system.tables (NamespaceId:TableName)
- `JobId` - for system.jobs
- `LiveQueryId` - for system.live_queries
- `UserName` - secondary index key for system.users (username → UserId lookup)

**Note**: system.users uses existing `UserId` as primary key, system.namespaces uses existing `NamespaceId`, system.storages uses existing `StorageId` (no new wrappers needed)

### Step 2: Define EntityStore Traits (T187-T189)

Create `backend/crates/kalamdb-store/src/entity_store.rs`:
- `EntityStore<K, V>` base trait
- `CrossUserTableStore<K, V>` for shared/system tables
- Export from `kalamdb-store/src/lib.rs`

**Important**: This will coexist with the old `EntityStore<T>` in `traits.rs` during migration. The old `EntityStore<T>` (which uses string keys) will be deprecated after migration is complete.

### Step 3: Create SystemTableStore Generic Implementation (T189)

Create `backend/crates/kalamdb-core/src/stores/system_table.rs`:
- `SystemTableStore<K, V>` implementing `EntityStore<K, V> + CrossUserTableStore<K, V>`
- **Generic over BOTH key and value types** (each system table has different key type)
- Example instantiations:
  ```rust
  SystemTableStore<UserId, User>           // system.users (UserId as primary key)
  SystemTableStore<TableId, TableMetadata> // system.tables  
  SystemTableStore<JobId, Job>             // system.jobs
  SystemTableStore<NamespaceId, Namespace> // system.namespaces
  SystemTableStore<StorageId, Storage>     // system.storages
  ```

### Step 4: Migrate System Table Providers (T191-T196)

Update each provider in `backend/crates/kalamdb-core/src/tables/system/` to use `SystemTableStore<K, V>`:

**New Folder Structure**: Each system table gets its own subfolder with:
- `{table}_provider.rs` - TableProvider implementation
- `{table}_store.rs` - SystemTableStore<K, V> wrapper
- `{table}_table.rs` - Table schema model (columns, DataFusion integration)
- `{table}_{index}_index.rs` - Secondary index implementations (if any)

**System Table Migrations**:
- `tables/system/users/`
  - `users_provider.rs` - UsersTableProvider
  - `users_store.rs` - SystemTableStore<UserId, User>
  - `users_table.rs` - system.users schema model
  - `users_username_index.rs` - SecondaryIndex<User, UserName> for username → UserId lookups
  
- `tables/system/tables/`
  - `tables_provider.rs` - SystemTablesTableProvider
  - `tables_store.rs` - SystemTableStore<TableId, TableMetadata>
  - `tables_table.rs` - system.tables schema model

- `tables/system/jobs/`
  - `jobs_provider.rs` - JobsTableProvider
  - `jobs_store.rs` - SystemTableStore<JobId, Job>
  - `jobs_table.rs` - system.jobs schema model

- `tables/system/namespaces/`
  - `namespaces_provider.rs` - NamespacesTableProvider
  - `namespaces_store.rs` - SystemTableStore<NamespaceId, Namespace>
  - `namespaces_table.rs` - system.namespaces schema model

- `tables/system/storages/`
  - `storages_provider.rs` - StoragesTableProvider
  - `storages_store.rs` - SystemTableStore<StorageId, Storage>
  - `storages_table.rs` - system.storages schema model

- `tables/system/live_queries/`
  - `live_queries_provider.rs` - LiveQueriesTableProvider
  - `live_queries_store.rs` - SystemTableStore<LiveQueryId, LiveQuery>
  - `live_queries_table.rs` - system.live_queries schema model

### Step 5: Refactor User/Shared/Stream Tables (T197-T199)

**New Folder Structure**: Each table type gets its own subfolder:

- `tables/user/`
  - `user_table_provider.rs` - UserTableProvider
  - `user_table_store.rs` - UserTableStoreImpl implementing EntityStore<UserRowId, UserTableRow>

- `tables/shared/`
  - `shared_table_provider.rs` - SharedTableProvider
  - `shared_table_store.rs` - SharedTableStoreImpl implementing EntityStore<RowId, SharedTableRow> + CrossUserTableStore<RowId, SharedTableRow>

- `tables/stream/`
  - `stream_table_provider.rs` - StreamTableProvider
  - `stream_table_store.rs` - StreamTableStoreImpl implementing EntityStore<RowId, StreamTableRow>

### Step 6: Update All Callers (T200-T205)

Update all code that uses the old store APIs to use new type-safe keys.

### Step 7: Integration & Testing (T206-T210)

- Update existing tests to use new APIs
- Add new tests for type safety
- Verify all integration tests pass

### Step 8: Documentation & Cleanup (T211-T213)

- Update AGENTS.md with EntityStore<K, V> pattern
- Create ENTITYSTORE_PATTERN.md architecture doc
- Deprecate old `EntityStore<T>` in traits.rs (add deprecation notice)

## Benefits

### 1. Code Reduction
- **Eliminate 400+ lines** of duplicated CRUD code
- **Single source of truth** for entity serialization/deserialization
- **Easier maintenance**: Fix once, applies to all stores

### 2. Type Safety
```rust
// ❌ Before: String soup, easy to make mistakes
store.put("default", "users", "u123", data)?;  // Which param is which?

// ✅ After: Compile-time verification
let key = RowId::from_string("u123");
store.put(&key, &row)?;  // Can't pass wrong types!
```

### 3. Consistent APIs
```rust
// All stores now have identical interface
impl<V> EntityStore<Vec<u8>, V> for SystemTableStore<V> { ... }
impl EntityStore<RowId, SharedTableRow> for SharedTableStore { ... }
impl EntityStore<UserRowId, UserTableRow> for UserTableStore { ... }

// Same methods everywhere: put, get, delete, scan_prefix, scan_all
```

### 4. Index Integration
```rust
// Indexes work seamlessly with EntityStore
struct UserStore {
    store: Arc<dyn EntityStore<Vec<u8>, User>>,
    indexes: UserIndexManager,
}

impl UserStore {
    pub fn put_user(&self, user_id: &[u8], user: &User, old: Option<&User>) -> Result<()> {
        self.store.put(user_id, user)?;  // Store entity
        self.indexes.put(user_id, user, old)?;  // Update indexes
        Ok(())
    }
}
```

### 5. Testing Improvements
```rust
// Easy to create mock stores for testing
struct MockEntityStore<K, V> {
    data: HashMap<K, V>,
}

impl<K, V> EntityStore<K, V> for MockEntityStore<K, V> {
    // In-memory implementation for tests
}
```

## Migration Strategy

### Phase 1: Non-Breaking Additions
1. Add new key types to `kalamdb-commons`
2. Add `EntityStore` traits to `kalamdb-store`
3. Create new `SystemTableStore` alongside existing providers
4. Add new `*Impl` stores alongside old ones

### Phase 2: Gradual Migration
5. Update one system table provider at a time
6. Update shared/user/stream stores one at a time
7. Update callers module by module
8. Maintain backward compatibility via adapter layer if needed

### Phase 3: Cleanup
9. Remove old store implementations
10. Remove adapter code
11. Update documentation
12. Verify all tests pass

## Success Criteria

- [ ] All 7 key types created with full test coverage
- [ ] `EntityStore<K, V>` trait defined with provided methods
- [ ] `SystemTableStore<V>` generic implementation complete
- [ ] All 6 system table providers migrated
- [ ] Shared/User/Stream table stores refactored
- [ ] Zero compilation errors/warnings
- [ ] All existing tests pass
- [ ] New type-safety tests added
- [ ] Code reduction: ≥400 lines eliminated
- [ ] Documentation updated (AGENTS.md, architecture docs)

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking changes to existing code | High | Use adapter pattern during transition |
| Type conversion overhead | Low | Implement efficient `AsRef<[u8]>` conversions |
| Complex generic constraints | Medium | Document trait bounds clearly, provide examples |
| Test migration effort | Medium | Migrate tests incrementally, one module at a time |
| Index compatibility | Low | Indexes already use generic `SecondaryIndex<T, K>` |

## Related Work

- **Phase 13**: Generic Index Infrastructure (SecondaryIndex<T, K>) - COMPLETE
- **Phase 0**: System Model Consolidation (User, Job, LiveQuery models) - COMPLETE
- **Phase 0.5**: Storage Backend Abstraction - IN PROGRESS

## References

- [EntityStore trait pattern discussion](../AGENTS.md)
- [Generic Index Infrastructure](./PHASE_13_INDEX_INFRASTRUCTURE.md)
- [Type-safe domain models](../../backend/crates/kalamdb-commons/src/models/)
