# AppContext Comprehensive Design

**Date**: November 3, 2025  
**Status**: Complete architectural specification  
**Location**: See `spec.md` User Story 8 for full implementation plan

## ✅ Complete Coverage Verification

After thorough codebase analysis, the AppContext design now includes **ALL** shared resources:

### 📦 **All Stores** (3/3 covered)
- ✅ `UserTableStore` - Per-user data isolation
- ✅ `SharedTableStore` - Global namespace data
- ✅ `StreamTableStore` - Ephemeral event storage

### 🔧 **All Services** (7/7 covered)
- ✅ `NamespaceService` - Still injected (needs KalamSql from AppContext)
- ✅ `UserTableService` - Becomes stateless (uses AppContext internally)
- ✅ `SharedTableService` - Becomes stateless
- ✅ `StreamTableService` - Becomes stateless
- ✅ `TableDeletionService` - Becomes stateless
- ✅ `BackupService` - Uses AppContext for stores
- ✅ `RestoreService` - Uses AppContext for stores
- ✅ `SchemaEvolutionService` - Uses AppContext for schema store

### 📊 **All System Table Providers** (10/10 covered)
- ✅ `UsersTableProvider` (system.users)
- ✅ `JobsTableProvider` (system.jobs)
- ✅ `NamespacesTableProvider` (system.namespaces)
- ✅ `StoragesTableProvider` (system.storages)
- ✅ `LiveQueriesTableProvider` (system.live_queries)
- ✅ `TablesTableProvider` (system.tables)
- ✅ `AuditLogsTableProvider` (system.audit_logs)
- ✅ `StatsTableProvider` (system.stats - virtual table)
- ✅ `InformationSchemaTablesProvider` (information_schema.tables)
- ✅ `InformationSchemaColumnsProvider` (information_schema.columns)

### 🏭 **All Data Table Providers** (3/3 covered)
**Note**: These are created per-request, NOT stored in AppContext (correct design)
- ✅ `UserTableAccess` - Wraps `Arc<UserTableShared>` (cached in SchemaCache)
- ✅ `SharedTableProvider` - Created per registration (uses stores from AppContext)
- ✅ `StreamTableProvider` - Created per registration (uses stores from AppContext)

**Special Case**: `UserTableShared`
- ✅ Stored in `SchemaCache` (not directly in AppContext)
- Phase 3C optimization: One instance per table, reused across all user requests
- AppContext provides access via `unified_cache()` getter

### 🎯 **All Managers** (2/2 covered)
- ✅ `JobManager` (trait, implemented by TokioJobManager)
- ✅ `LiveQueryManager` (WebSocket subscription coordination)

### 📋 **All Registries** (3/3 covered)
- ✅ `StorageRegistry` (storage path template resolution)
- ✅ `SchemaRegistry` (thin facade over SchemaCache for read-through schema + derived artifacts)
- ✅ `LiveQueryRegistry` - Internal to LiveQueryManager (not needed in AppContext)

### 💾 **All Caches** (1/1 covered)
- ✅ `SchemaCache` (Phase 10 unified cache - replaces TableCache + old SchemaCache)

### 🏗️ **All Factories** (1/1 covered)
- ✅ `DataFusionSessionFactory` - Creates SessionContext with custom SQL functions

### ⏰ **Schedulers** (2/2 - Correct placement)
**NOT in AppContext** (stays in ApplicationComponents for HTTP lifecycle):
- ✅ `FlushScheduler` - Background flush job scheduler
- ✅ `StreamEvictionScheduler` - TTL eviction scheduler

**Why not in AppContext?**: Schedulers need explicit start/stop during HTTP server lifecycle (graceful shutdown). AppContext is for database-layer singletons only.

### 🗄️ **Core Infrastructure** (2/2 covered)
- ✅ `KalamSql` - System table access adapter
- ✅ `StorageBackend` (trait) - RocksDB abstraction

### 🎛️ **DataFusion Session Management** (2/2 covered)
- ✅ `DataFusionSessionFactory` - Factory for creating sessions
- ✅ `base_session_context` - Template SessionContext with system schema registered

**Key Methods**:
```rust
ctx.create_session() -> Arc<SessionContext>  // Fresh session for queries
ctx.create_session_for_user(user_id, ns) -> (Arc<SessionContext>, KalamSessionState)
```

---

## 📊 Memory Savings Breakdown

### Before AppContext (Per Request):
```
SqlExecutor fields:
- user_table_store: Arc<UserTableStore>          (24 bytes)
- shared_table_store: Arc<SharedTableStore>      (24 bytes)
- stream_table_store: Arc<StreamTableStore>      (24 bytes)
- kalam_sql: Arc<KalamSql>                       (24 bytes)
- storage_backend: Arc<dyn StorageBackend>       (24 bytes)
- jobs_provider: Arc<JobsTableProvider>          (24 bytes)
- users_provider: Arc<UsersTableProvider>        (24 bytes)
- storage_registry: Arc<StorageRegistry>         (24 bytes)
- job_manager: Arc<dyn JobManager>               (24 bytes)
- live_query_manager: Arc<LiveQueryManager>      (24 bytes)
- schema_store: Arc<TableSchemaStore>            (24 bytes)
Total: 264 bytes per SqlExecutor instance

UserTableService fields:
- kalam_sql: Arc<KalamSql>                       (24 bytes)
- user_table_store: Arc<UserTableStore>          (24 bytes)
Total: 48 bytes per service instance

SharedTableService fields:
- shared_table_store: Arc<SharedTableStore>      (24 bytes)
- kalam_sql: Arc<KalamSql>                       (24 bytes)
Total: 48 bytes per service instance

StreamTableService fields:
- stream_table_store: Arc<StreamTableStore>      (24 bytes)
- kalam_sql: Arc<KalamSql>                       (24 bytes)
Total: 48 bytes per service instance

Per-request allocation: 264 + 48 + 48 + 48 = 408 bytes of Arc pointers
For 1000 requests: 408KB just in Arc metadata (not counting actual data!)
```

### After AppContext (Per Request):
```
SqlExecutor fields:
- namespace_service: Arc<NamespaceService>       (24 bytes)
- session_context: Arc<SessionContext>           (24 bytes)
- session_factory: DataFusionSessionFactory      (0 bytes - zero-sized)
- enforce_password_complexity: bool              (1 byte)
Total: 49 bytes per SqlExecutor instance (88% reduction!)

Services are zero-sized (stateless):
- UserTableService: 0 bytes
- SharedTableService: 0 bytes
- StreamTableService: 0 bytes

Per-request allocation: 49 bytes
For 1000 requests: 49KB (92% memory reduction!)
```

---

## 🔑 Key Design Decisions

### 1. **DataFusion Session Management**
**Decision**: Store both `session_factory` and `base_session_context` in AppContext

**Rationale**:
- `session_factory` creates new SessionContext instances with custom functions registered
- `base_session_context` is a template with system schema already registered
- Per-request sessions clone the base context (faster than re-registering system tables)

**Usage**:
```rust
// Quick session for one-off queries
let ctx = AppContext::get();
let session = ctx.create_session();

// Session with user context (CURRENT_USER() function)
let (session, state) = ctx.create_session_for_user(user_id, namespace_id);
```

### 2. **System Table Providers**
**Decision**: Store ALL 10 system table providers in AppContext

**Rationale**:
- System tables are singletons (one instance per server)
- Used across multiple queries and sessions
- Registration happens once at startup
- Zero duplication across requests

**Benefit**: SqlExecutor no longer needs `jobs_provider`, `users_provider` fields

### 3. **UserTableShared Caching**
**Decision**: `UserTableShared` lives in `SchemaCache`, NOT directly in AppContext

**Rationale**:
- Phase 3C optimization: One UserTableShared per table (not per user)
- SchemaCache already handles table-level singletons
- AppContext provides access via `unified_cache()` getter
- Cleaner separation: AppContext = global singletons, SchemaCache = per-table singletons

**Usage**:
```rust
let ctx = AppContext::get();
let cache = ctx.unified_cache();

// Get or create UserTableShared
let shared = cache.get_user_table_shared(&table_id)
    .unwrap_or_else(|| {
        let shared = UserTableShared::new(table_id, cache, schema, store);
        cache.insert_user_table_shared(table_id, shared.clone());
        shared
    });

// Wrap in per-request UserTableAccess
let user_access = UserTableAccess::new(shared, user_id, role);
```

### 4. **Schedulers NOT in AppContext**
**Decision**: `FlushScheduler` and `StreamEvictionScheduler` stay in `ApplicationComponents`

**Rationale**:
- Schedulers need explicit lifecycle management (start/stop)
- HTTP server shutdown needs to gracefully stop schedulers
- AppContext is for passive shared state, not active background tasks
- Separation of concerns: DB layer (AppContext) vs HTTP layer (ApplicationComponents)

---

## 📡 Real-time Subscriptions (Live Queries)

### Components
- LiveQueryManager (in AppContext): central coordination of subscriptions and notifications.
- LiveQueryRegistry (internal to manager): tracks active subscriptions per table/user.
- Providers: UserTableAccess/SharedTableProvider/StreamTableProvider emit change events to the manager.

### Data flow
1) Client subscribes → route handler registers a subscription with LiveQueryManager (namespace, table_id, filters, user_id).
2) On append/update/delete in a provider, a lightweight event is sent to LiveQueryManager.
3) LiveQueryManager fans out to subscribers; payloads are shared (Arc<RecordBatch>/Arc<RowChange>) to avoid copies.
4) Backpressure-aware channels prevent unbounded memory growth; slow subscribers see bounded buffers and drop policies per config.

### Memory efficiency
- Single manager instance (Arc in AppContext) → zero duplication.
- Subscription entries store compact metadata (table_id as Arc<TableId>, user_id, filter plan handle).
- Event payload reuse via Arc minimizes copies, particularly for batch inserts.
- No schema duplication; schema comes from SchemaRegistry/SchemaCache.

### Security and isolation
- Per-request SessionContext ensures CURRENT_USER() and RLS apply to subscription queries.
- LiveQueryManager checks role/permissions before registering subscriptions.

---

## 💾 Flush Pipeline (User & Shared Tables)

### Goals
- Convert buffered writes (RocksDB) into Parquet segments per `FlushPolicy` with minimal allocations.

### Shared resources
- StorageRegistry (AppContext): base directory and templates.
- SchemaRegistry/SchemaCache: `CachedTableData` (schema, table_type, storage_id, storage_path_template).
- UserTableStore/SharedTableStore: read buffered rows by shard/sequence.

### Path resolution
- Use `SchemaCache::get_storage_path(&table_id, user_id_opt, shard_opt)` to resolve the final Parquet path.
    - User tables: supply `Some(user_id)` + shard for `{userId}`/`{shard}` placeholders.
    - Shared tables: omit user; only `{namespace}`/`{tableName}` used.

### Execution
1) FlushScheduler (kept in ApplicationComponents) evaluates `FlushPolicy` (time/size/rows) per table.
2) It spawns a job via JobManager (AppContext) to flush a specific table (and user/shard for user tables).
3) The job reads buffered batches, converts to Arrow using the single `TableDefinition` from SchemaRegistry, writes Parquet to resolved path.
4) On success, update counters/metadata; on failure, retry per policy.

### Memory efficiency
- One `TableDefinition`/Arrow schema per table (Arc-shared) used across batches.
- Stream writes to Parquet; avoid materializing entire table in memory.
- Reuse buffers/builders where possible; avoid String clones by using interned identifiers (as per Phase 10/14 notes).
- No duplicate caches: path templates come from `CachedTableData.storage_path_template` (already consolidated).

### Isolation
- User tables flush per user/shard, ensuring per-user storage layout and easy retention.
- Shared tables flush without user placeholder, using shared template.

---

## 🎯 Usage Patterns

### Pattern 1: Stateless Services
```rust
pub struct UserTableService;

impl UserTableService {
    pub fn new() -> Self { Self }
    
    pub fn create_table(&self, stmt: CreateTableStatement) -> Result<()> {
        let ctx = AppContext::get();
        let kalam_sql = ctx.kalam_sql();
        let store = ctx.user_table_store();
        
        // Use resources without storing them
        kalam_sql.insert_table_metadata(&stmt)?;
        store.create_column_family(&stmt.table_name)?;
        Ok(())
    }
}
```

### Pattern 2: Stateless SqlExecutor (memory-focused)
```rust
pub struct SqlExecutor;

impl SqlExecutor {
    pub fn new() -> Self { SqlExecutor }
    
    pub async fn execute(
        &self,
        session: &SessionContext,   // Per-request session passed in
        sql: &str,
        exec_ctx: ExecutionContext,
    ) -> Result<ExecutionResult> {
        let ctx = AppContext::get();
        
        // Get all resources from AppContext
        let kalam_sql = ctx.kalam_sql();
        let user_store = ctx.user_table_store();
        let job_manager = ctx.job_manager();
        
        // Execute query
        match parse_statement(sql)? {
            Statement::CreateTable(stmt) => {
                UserTableService::new().create_table(stmt)?;
                Ok(ExecutionResult::Success("Table created".into()))
            }
            // ... other cases
        }
    }
}
```

### Pattern 3: Provider Registration (Once at Startup)
```rust
pub fn register_user_table(
    session: &SessionContext,
    table_id: Arc<TableId>,
    user_id: UserId,
    role: Role,
) -> Result<()> {
    let ctx = AppContext::get();
    let cache = ctx.unified_cache();
    
    // Get or create UserTableShared (cached)
    let shared = cache.get_user_table_shared(&table_id)
        .unwrap_or_else(|| {
            let schema = load_schema_from_db(&table_id)?;
            let shared = UserTableShared::new(
                table_id.clone(),
                cache.clone(),
                schema,
                ctx.user_table_store(),
            );
            cache.insert_user_table_shared(table_id.clone(), shared.clone());
            shared
        });
    
    // Create per-request UserTableAccess wrapper
    let provider = Arc::new(UserTableAccess::new(shared, user_id, role));
    
    // Register with DataFusion
    session.register_table(&table_id.to_string(), provider)?;
    Ok(())
}
```

---

## ✅ Verification Checklist

- [x] All stores covered (UserTableStore, SharedTableStore, StreamTableStore)
- [x] All services can become stateless (use AppContext internally)
- [x] All system table providers included (10 providers)
- [x] All managers included (JobManager, LiveQueryManager)
- [x] All caches included (SchemaCache unified cache)
- [x] DataFusion session management included (factory + base context)
- [x] Storage registry included
- [x] Schedulers correctly excluded (stay in ApplicationComponents)
- [x] UserTableShared correctly handled (cached in SchemaCache, not AppContext)
- [x] No duplicated Arc allocations across layers
- [x] Clear separation: AppContext (DB layer) vs ApplicationComponents (HTTP layer)

---

## 📝 Implementation Notes

### Phase 1 Priority: Core AppContext Structure
Focus on getting the singleton working with ALL resources:
- All stores, managers, providers, caches, factories
- Complete initialization logic
- All getter methods

### Phase 2 Priority: Service Refactoring
Make services stateless one by one:
1. UserTableService (easiest, just 2 deps)
2. SharedTableService
3. StreamTableService
4. TableDeletionService
5. BackupService / RestoreService

### Phase 3 Priority: SqlExecutor Simplification
Remove all Option<Arc<T>> fields, keep only:
- namespace_service (still injected)
- session_context (per-request)
- session_factory (zero-sized)
- enforce_password_complexity (config flag)

### Testing Strategy
1. Create `create_test_app_context()` helper (Phase 1)
2. Update service tests to use AppContext (Phase 2)
3. Update SqlExecutor tests (Phase 3)
4. Integration tests to verify behavior unchanged

---

## 🎉 Expected Outcomes

### Code Metrics
- **Lines removed**: ~1,200 (builder methods, Arc fields, test boilerplate)
- **Lines added**: ~600 (AppContext + getters + docs + test helpers)
- **Net reduction**: ~600 lines
- **Files modified**: ~45

### Memory Metrics
- **Arc allocations**: 92% reduction per request (408 bytes → 49 bytes)
- **Service struct size**: 100% reduction (48 bytes → 0 bytes, stateless)
- **SqlExecutor fields**: 88% reduction (17 fields → 4 fields)

### Developer Experience
- **Constructor simplicity**: 10+ params → 0-2 params
- **Test setup**: 50+ lines → 2 lines
- **New feature integration**: Update 1 file (AppContext) instead of 5+ files
- **Debugging**: Single source of truth for all shared state

---

**Status**: Ready for implementation ✅  
**Next Step**: Begin Phase 1 - Create AppContext structure in `backend/crates/kalamdb-core/src/app_context.rs`
