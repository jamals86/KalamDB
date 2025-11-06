# Feature Specification: Core Architecture Refactoring v2

**Feature Branch**: `010-core-architecture-v2`  
**Created**: 2025-11-06  
**Status**: Draft  
**Input**: User description: "Core architecture refactoring: SchemaRegistry renaming, Arrow schema caching, LiveQueryManager consolidation, system tables initialization, views implementation, and AppContext centralization"

## User Scenarios & Testing *(mandatory)*

### User Story 0 - AppContext Centralization with NodeId (Priority: P0)

Developers and system components need a single source of truth for global configuration like NodeId. Currently, NodeId is instantiated multiple times across the codebase, leading to inconsistencies and configuration drift. AppContext should be the sole owner of NodeId (loaded once from config.toml) and passed everywhere NodeId or other global state is needed.

**Why this priority**: This is P0 (foundational) because it affects all other refactoring work. Without centralized AppContext, components continue to create their own NodeId instances, making debugging distributed operations impossible and violating single-source-of-truth principles.

**Independent Test**: Can be tested by starting the server, inspecting logs from multiple components (LiveQueryManager, job executors, audit logs), and verifying all log entries use the identical NodeId loaded from config.toml.

**Acceptance Scenarios**:

1. **Given** config.toml specifies `node_id = "node-prod-01"`, **When** the server initializes AppContext, **Then** NodeId is allocated exactly once and stored in AppContext
2. **Given** AppContext is initialized with NodeId, **When** any component (LiveQueryManager, job executors, handlers) needs NodeId, **Then** it accesses NodeId via AppContext reference without creating new instances
3. **Given** a developer inspects distributed logs from multiple operations, **When** correlating events by NodeId, **Then** all events from the same server instance share the identical NodeId value

---

### User Story 1 - Schema Registry Refactoring and Arrow Cache (Priority: P1)

Developers and database operations require fast, repeated access to table schemas and Arrow schemas without rebuilding them on every query. The schema/ directory must be renamed to schema_registry/ and SchemaRegistry should act as a centralized cache that eliminates redundant schema construction overhead. Arrow schema memoization must be added to SchemaCache for 50-100× performance improvement.

**Why this priority**: This is P1 because schema lookups happen on every query execution. Performance gains here directly impact all database operations. Without caching, DataFusion rebuilds Arrow schemas repeatedly (~50-100μs per call), causing measurable latency for high-throughput workloads. This must complete before executor refactoring.

**Independent Test**: Can be tested by executing 1000 SELECT queries against the same table and measuring schema construction time. Success means zero schema reconstructions after initial cache load (1-2μs cached vs 50-100μs uncached).

**Acceptance Scenarios**:

1. **Given** a table has been queried once, **When** a developer executes subsequent queries against that table, **Then** the Arrow schema is retrieved from memoized cache in under 2μs without reconstruction (50-100× faster than current)
2. **Given** SchemaCache includes Arrow schema memoization map, **When** all 11 TableProvider implementations call schema(), **Then** they receive cached Arc<Schema> via SchemaCache.get_arrow_schema()
3. **Given** a table's schema has been modified via ALTER TABLE, **When** any subsequent query accesses that table, **Then** the cached Arrow schema is invalidated and rebuilt exactly once
4. **Given** the SchemaRegistry is initialized with 1000 tables, **When** queries execute repeatedly, **Then** cache hit rates above 99% are reported for stable workloads with <2MB memory overhead

---

### User Story 2 - Unified Live Query Manager (Priority: P1)

System administrators and developers need a single, coherent component to manage all live query subscriptions, connections, change detection, and filter caching. Currently scattered across multiple structs, this functionality should be consolidated for easier debugging, monitoring, and maintenance.

**Why this priority**: This is P1 because live queries are a core differentiator for KalamDB. Current fragmentation causes confusion, makes testing difficult, and increases the risk of connection leaks or subscription bugs.

**Independent Test**: Can be tested by establishing 100 concurrent WebSocket subscriptions, inserting data, and verifying all clients receive notifications. Success means zero subscription leaks and complete notification delivery.

**Acceptance Scenarios**:

1. **Given** a client establishes a WebSocket connection and subscribes to a query, **When** data matching the subscription filter is inserted, **Then** the unified LiveQueryManager detects the change and notifies the client within 100ms
2. **Given** multiple clients subscribe to overlapping queries, **When** a developer inspects LiveQueryManager state, **Then** all active subscriptions, connections, and cached filters are visible in a single registry
3. **Given** a client disconnects or WebSocket connection closes, **When** the LiveQueryManager's event-driven cleanup triggers, **Then** the corresponding subscription, socket, and filter cache entries are removed atomically without requiring periodic jobs

---

### User Story 3 - System Tables as Regular Storage (Priority: P2)

Database administrators need system tables (users, jobs, namespaces, storages, live_queries, tables) to be stored and managed using the same storage mechanisms as user tables. This enables consistent backup/restore, replication, and query patterns.

**Why this priority**: This is P2 because while important for operational consistency, existing system tables work but use a different storage path. This refactoring improves maintainability but doesn't block core functionality.

**Independent Test**: Can be tested by creating a namespace, inserting data into system.users, flushing to Parquet, restarting the server, and verifying system.users data persists and is queryable.

**Acceptance Scenarios**:

1. **Given** the system initializes for the first time, **When** the database starts, **Then** system tables are created in storage with the same RocksDB/Parquet architecture as shared tables
2. **Given** a database upgrade adds a new system table (e.g., system.audit_logs), **When** the system starts with stored schema version < current version, **Then** the missing system table is automatically created and schema version is updated (enabling future migrations using ALTER TABLE pattern)
3. **Given** a DBA queries system.jobs, **When** the query executes, **Then** it uses the standard TableProvider interface and retrieves data from both RocksDB buffer and Parquet files
4. **Given** a system table reaches flush threshold, **When** the flush job executes, **Then** buffered rows are written to Parquet files and removed from RocksDB exactly like user tables

---

### User Story 4 - Virtual Views Support (Priority: P3)

Developers need the ability to define and query views that present alternative schemas over existing tables without physically storing data. Views should be registered in the SchemaRegistry and queryable via standard SQL SELECT statements.

**Why this priority**: This is P3 because views are a convenience feature that improves developer experience but aren't required for core database operations. They can be implemented after foundational refactoring is stable.

**Independent Test**: Can be tested by creating a view over information_schema.columns that filters to show only indexed columns, querying the view, and verifying results match the underlying table filter.

**Acceptance Scenarios**:

1. **Given** a developer defines a view `v_active_users` as `SELECT * FROM system.users WHERE deleted_at IS NULL`, **When** the view is registered in SchemaRegistry, **Then** it appears in information_schema.tables with table_type='VIEW'
2. **Given** a view is registered, **When** a user executes `SELECT * FROM v_active_users`, **Then** the query planner transparently rewrites it to the underlying table query
3. **Given** the underlying table schema changes, **When** the SchemaRegistry invalidates cache, **Then** dependent views are also invalidated and rebuilt on next access
4. **Given** a view `v_active_users` depends on system.users which is then dropped, **When** a user queries `SELECT * FROM v_active_users`, **Then** the query fails immediately with error message "View v_active_users references missing table system.users" (validation at query time, not DROP time)

---

## Clarifications

### Session 2025-11-06

- Q: What should be the SchemaRegistry cache size limit and eviction policy? → A: No limit needed - cache all tables. Table count won't exceed memory limits in practice.
- Q: How should LiveQueryManager handle abandoned subscriptions cleanup? → A: Connection-based cleanup - Remove subscription when WebSocket closes (event-driven)
- Q: How does system table initialization handle upgrades where new system tables are added? → A: Schema version comparison - Store schema version, compare on startup, create missing tables if version mismatch. This enables future migrations using same pattern as ALTER TABLE
- Q: When should view dependency validation happen if underlying table is dropped? → A: Query-time validation - Check dependencies when view is queried, fail with clear error (no CASCADE logic needed)
- Q: How should the 50-100× Arrow schema speedup be validated? → A: Benchmark test with 1000 queries - Automated test measures schema() latency for repeated queries (first call 50-100μs uncached, subsequent <2μs cached)

---

### Edge Cases

- SchemaRegistry caches all tables without eviction since table count stays within memory limits
- How does the system handle Arrow schema cache invalidation when tables are dropped? Must remove all cached entries.
- LiveQueryManager cleans up subscriptions when WebSocket connections close (event-driven, no periodic job needed)
- System table initialization uses schema version comparison on startup - compares stored version against current version, creates missing tables if mismatch. This pattern supports future migrations (same as ALTER TABLE).
- View dependency validation happens at query time - if underlying table is dropped, querying the view fails with clear error message (no CASCADE logic during DROP TABLE)

## Requirements *(mandatory)*

### Functional Requirements

#### Phase 1: Foundation (Must Complete First)

- **FR-000**: AppContext MUST be the single source of truth for NodeId, loaded once from config.toml during initialization
- **FR-001**: System MUST rename schema/ directory to schema_registry/ throughout the codebase (NOTE: SchemaCache struct will also be renamed to SchemaRegistry)
- **FR-002**: CachedTableData MUST add arrow_schema: Arc<RwLock<Option<Arc<Schema>>>> field (using std::sync::RwLock) for lazy Arrow schema initialization
- **FR-003**: CachedTableData MUST expose arrow_schema() method that computes on first call (~50-100μs) and returns cached Arc::clone() on subsequent calls (~1-2μs) via read-optimized RwLock with double-check locking
- **FR-004**: SchemaCache invalidate() and clear() methods automatically remove Arrow schemas when CachedTableData is removed (embedded design eliminates separate cleanup)
- **FR-005**: TableProviderCore MUST add arrow_schema() method that delegates to unified_cache.get_arrow_schema(&self.table_id). MUST also remove the old schema: SchemaRef field (redundant with memoization).
- **FR-006**: All 11 TableProvider implementations (UserTableAccess, SharedTableProvider, StreamTableProvider, 8 system tables) MUST use arrow_schema() in their schema() methods and MUST panic on error (not return empty schema)

#### Phase 2: Refactoring (Depends on Phase 1)

- **FR-007**: System MUST consolidate UserConnections, UserTableChangeDetector, and LiveQueryManager into a single LiveQueryManager struct
- **FR-008**: Unified LiveQueryManager MUST maintain registry of all active subscriptions with their WebSocket connections, filters, and metadata
- **FR-009**: System MUST initialize system tables (users, jobs, namespaces, storages, live_queries, tables) using the same storage backend as shared tables. On startup, compare schema version; if mismatch, create missing system tables to enable future migration path.
- **FR-010**: System tables MUST support flush operations that write buffered rows from RocksDB to Parquet files
- **FR-011**: System MUST support registering views in SchemaRegistry with view definitions stored separately from physical tables
- **FR-012**: Views MUST be queryable via SELECT statements with transparent rewriting to underlying table queries
- **FR-013**: SchemaRegistry MUST track view dependencies on base tables for cascade invalidation. Views MUST validate dependencies at query time - if base table is missing, query fails with clear error indicating broken dependency (no CASCADE enforcement during DROP TABLE)
- **FR-014**: All components requiring NodeId MUST receive it via AppContext parameter instead of instantiating NodeId locally
- **FR-015**: SqlExecutor refactoring from executor.rs to executor/mod.rs MUST be completed AFTER FR-000 to FR-006 are done
- **FR-016**: Refactored SqlExecutor MUST depend fully on AppContext and schema_registry (no legacy patterns)
- **FR-017**: System MUST ensure all unit tests and integration tests compile and pass after refactoring

### Key Entities

- **AppContext**: Singleton containing all global configuration and state. Single owner of NodeId loaded from config.toml. Passed by reference to all components requiring global state.
- **SchemaCache** (will be renamed to SchemaRegistry): Unified cache containing CachedTableData with embedded Arrow schema memoization. Single DashMap provides 50-100× speedup for repeated schema access.
- **CachedTableData**: Contains Arc<TableDefinition> as single source of truth AND lazy-initialized Arc<Schema> (embedded via std::sync::RwLock<Option>). Arrow schema computed on first arrow_schema() call, cached forever until invalidation. Clone semantics: All clones share the same RwLock (intentional).
- **TableProviderCore**: Common core for all providers. Has unified_cache: Arc<SchemaCache> field. Provides arrow_schema() method that delegates to unified_cache.get_arrow_schema(&table_id) → CachedTableData.arrow_schema() for memoized access. Will remove redundant schema: SchemaRef field.
- **LiveQueryManager**: Consolidated struct containing subscription registry (RwLock<LiveQueryRegistry>), user connections (HashMap<ConnectionId, UserConnectionSocket>), filter cache (RwLock<FilterCache>), initial data fetcher, schema registry reference, and node ID
- **SystemTableStorage**: New abstraction for system tables that uses standard TableProvider + StorageBackend pattern instead of custom system table logic
- **ViewDefinition**: Metadata structure for virtual views containing view name, SQL definition, dependent table IDs, and cached rewritten query plan

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-000**: NodeId is allocated exactly once per server instance and all logged events use the identical NodeId value
- **SC-001**: Arrow schema cache hit rate exceeds 99% for workloads with stable table schemas
- **SC-002**: Schema lookup latency reduces from 50-100μs (uncached) to under 2μs (memoized) for repeated accesses - achieving 50-100× speedup. Validated via automated benchmark test with 1000 repeated queries.
- **SC-003**: Arrow schema memoization adds less than 2MB memory overhead for 1000 tables (~1-2KB per table)
- **SC-004**: All 11 TableProvider implementations successfully use SchemaCache.get_arrow_schema() with zero performance regressions
- **SC-005**: LiveQueryManager consolidation reduces subscription management code by at least 30% (lines of code metric)
- **SC-006**: System table queries perform within 10% of equivalent shared table queries (no performance regression)
- **SC-007**: Zero duplicate NodeId instantiations detected in codebase after refactoring (validated via code review)
- **SC-008**: All 477 existing kalamdb-core tests pass without modification or with minimal fixture updates
- **SC-009**: View queries return results within 5% of direct table query performance (minimal rewriting overhead)
- **SC-010**: System table initialization completes in under 100ms on database startup

## Assumptions *(optional)*

- NodeId is currently instantiated multiple times using `NodeId::from(format!("node-{}", std::process::id()))` pattern
- Arrow schema construction is currently a performance bottleneck based on profiling data
- Existing SchemaCache implementation (DashMap-based) has sufficient concurrency for high-throughput workloads
- System tables require persistent storage for disaster recovery and auditability
- View definitions will use SQL strings as the storage format (not AST/protobuf serialization)
- LiveQueryManager consolidation won't require changes to WebSocket protocol or client SDKs

## Dependencies *(optional)*

- **Phase 5 SchemaRegistry Enhancement**: Already completed, provides foundation for caching extensions
- **Phase 10 Cache Consolidation**: Already completed, provides unified SchemaCache with LRU timestamps
- **StorageBackend Abstraction**: Already implemented, enables system tables to use RocksDB/Parquet
- **DataFusion 40.0**: Query planning infrastructure required for view rewriting logic

## Implementation Order *(critical)*

This refactoring MUST follow strict ordering to avoid breaking changes:

1. **First**: AppContext centralization (FR-000, FR-014) - Establish single source of truth for NodeId
2. **Second**: schema/ → schema_registry/ rename (FR-001) - Update all imports and module paths
3. **Third**: Arrow schema memoization (FR-002 to FR-006) - Complete implementation:
   - Add arrow_schemas DashMap to SchemaCache
   - Implement SchemaCache.get_arrow_schema() with compute-once-cache-forever pattern
   - Update invalidation methods (invalidate(), clear())
   - Add TableProviderCore.arrow_schema() method
   - Update all 11 TableProvider.schema() implementations to use memoized schemas
   - Add unit tests and benchmarks to verify 50-100× speedup
4. **Fourth**: SqlExecutor migration (FR-015, FR-016) - Refactor executor.rs → executor/mod.rs with AppContext/schema_registry dependencies
5. **Fifth**: LiveQueryManager consolidation (FR-007, FR-008) - Merge scattered structs
6. **Sixth**: System tables storage (FR-009, FR-010) - Use standard storage backend
7. **Seventh**: Views support (FR-011 to FR-013) - Add virtual table infrastructure
8. **Final**: Testing (FR-017) - Ensure all tests pass

**Note**: SqlExecutor refactoring (executor.rs → executor/mod.rs) was started but incomplete. It MUST wait until AppContext and schema_registry changes (Steps 1-3) are complete to avoid rework.

## Out of Scope *(optional)*

- Materialized views (views that physically store computed results)
- Cross-database views (views spanning multiple KalamDB instances)
- Updateable views (INSERT/UPDATE/DELETE through views)
- View permission inheritance from base tables (security model extension)
- Automatic view dependency detection for circular reference prevention
- Distributed live query federation across multiple nodes
- Completing executor.rs → executor/mod.rs migration before AppContext/schema_registry are ready (DEFERRED)

## Technical Notes *(for implementers)*

### Current State
- `backend/crates/kalamdb-core/src/sql/executor.rs` exists (legacy, large monolithic file)
- `backend/crates/kalamdb-core/src/sql/executor/mod.rs` exists (partial migration, INCOMPLETE)
- Migration was started but not completed - DO NOT continue until foundations are ready

### Required Before SqlExecutor Migration
1. AppContext must expose NodeId from config.toml (no more `NodeId::from(format!("node-{}", std::process::id()))`)
2. schema_registry must be fully renamed and operational with Arrow caching
3. All handlers/services must use AppContext reference pattern

### SqlExecutor Migration Strategy (After Foundations)
- Keep executor.rs as-is until FR-000 to FR-006 complete (AppContext + schema_registry + Arrow memoization)
- Once ready, complete migration to executor/mod.rs with:
  - All methods receive `&AppContext` parameter
  - All schema lookups via `app_context.schema_registry().get_arrow_schema()` (memoized)
  - All NodeId access via `app_context.node_id()`
  - Zero direct RocksDB/storage dependencies (use providers)
- Delete executor.rs only after executor/mod.rs is complete and tested

### Arrow Schema Memoization Details (FR-002 to FR-006)
**Files to modify**:
1. `schema_registry/schema_cache.rs` (will be renamed to schema_registry/registry.rs):
   - Add `arrow_schema: Arc<RwLock<Option<Arc<Schema>>>>` field to CachedTableData (using std::sync::RwLock)
   - Update CachedTableData::new() to initialize arrow_schema as Arc::new(RwLock::new(None))
   - Implement CachedTableData::arrow_schema() with double-check locking
   - Implement SchemaCache::get_arrow_schema() that delegates to CachedTableData
   - Add Clone semantics documentation to CachedTableData
2. `tables/base_table_provider.rs`:
   - Remove `schema: SchemaRef` field from TableProviderCore (redundant)
   - Add `arrow_schema()` method to TableProviderCore that delegates to unified_cache
   - Update constructor to not require schema parameter
3. All 11 TableProvider implementations:
   - `tables/user_tables/user_table_provider.rs` (UserTableAccess)
   - `tables/shared_tables/shared_table_provider.rs` (SharedTableProvider)
   - `tables/stream_tables/stream_table_provider.rs` (StreamTableProvider)
   - `tables/system/users/users_provider.rs` (UsersTableProvider)
   - `tables/system/jobs/jobs_provider.rs` (JobsTableProvider)
   - `tables/system/namespaces/namespaces_provider.rs` (NamespacesTableProvider)
   - `tables/system/storages/storages_provider.rs` (StoragesTableProvider)
   - `tables/system/live_queries/live_queries_provider.rs` (LiveQueriesTableProvider)
   - `tables/system/tables/tables_provider.rs` (TablesTableProvider)
   - `tables/system/audit_logs/audit_logs_provider.rs` (AuditLogsTableProvider)
   - `tables/system/stats.rs` (StatsTableProvider)

**Performance target**: 50-100× speedup for schema access (75μs → 1.5μs for repeated queries)

**Testing strategy**: Create automated benchmark executing 1000 identical SELECT queries measuring schema() method call latency. Success criteria: First call 50-100μs (uncached), subsequent calls <2μs (cached).
