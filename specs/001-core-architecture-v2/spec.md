# Feature Specification: Core Architecture Refactoring v2

**Feature Branch**: `1-core-architecture-v2`  
**Created**: 2025-11-06  
**Status**: Draft  
**Input**: User description: "Core architecture refactoring: SchemaRegistry renaming, Arrow schema caching, LiveQueryManager consolidation, system tables initialization, and views implementation"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Schema Registry Performance Optimization (Priority: P1)

Developers and database operations require fast, repeated access to table schemas and Arrow schemas without rebuilding them on every query. The SchemaRegistry should act as a centralized cache that eliminates redundant schema construction overhead.

**Why this priority**: This is P1 because schema lookups happen on every query execution. Performance gains here directly impact all database operations. Without caching, DataFusion rebuilds Arrow schemas repeatedly, causing measurable latency.

**Independent Test**: Can be tested by executing 1000 SELECT queries against the same table and measuring schema construction time. Success means zero schema reconstructions after initial cache load.

**Acceptance Scenarios**:

1. **Given** a table has been queried once, **When** a developer executes subsequent queries against that table, **Then** the Arrow schema is retrieved from cache in under 1μs without reconstruction
2. **Given** a table's schema has been modified via ALTER TABLE, **When** any subsequent query accesses that table, **Then** the cached schema is invalidated and rebuilt exactly once
3. **Given** the SchemaRegistry is initialized, **When** a developer queries the cache statistics, **Then** cache hit rates above 99% are reported for stable workloads

---

### User Story 2 - Unified Live Query Manager (Priority: P1)

System administrators and developers need a single, coherent component to manage all live query subscriptions, connections, change detection, and filter caching. Currently scattered across multiple structs, this functionality should be consolidated for easier debugging, monitoring, and maintenance.

**Why this priority**: This is P1 because live queries are a core differentiator for KalamDB. Current fragmentation causes confusion, makes testing difficult, and increases the risk of connection leaks or subscription bugs.

**Independent Test**: Can be tested by establishing 100 concurrent WebSocket subscriptions, inserting data, and verifying all clients receive notifications. Success means zero subscription leaks and complete notification delivery.

**Acceptance Scenarios**:

1. **Given** a client establishes a WebSocket connection and subscribes to a query, **When** data matching the subscription filter is inserted, **Then** the unified LiveQueryManager detects the change and notifies the client within 100ms
2. **Given** multiple clients subscribe to overlapping queries, **When** a developer inspects LiveQueryManager state, **Then** all active subscriptions, connections, and cached filters are visible in a single registry
3. **Given** a client disconnects, **When** the LiveQueryManager cleans up resources, **Then** the corresponding subscription, socket, and filter cache entries are removed atomically

---

### User Story 3 - System Tables as Regular Storage (Priority: P2)

Database administrators need system tables (users, jobs, namespaces, storages, live_queries, tables) to be stored and managed using the same storage mechanisms as user tables. This enables consistent backup/restore, replication, and query patterns.

**Why this priority**: This is P2 because while important for operational consistency, existing system tables work but use a different storage path. This refactoring improves maintainability but doesn't block core functionality.

**Independent Test**: Can be tested by creating a namespace, inserting data into system.users, flushing to Parquet, restarting the server, and verifying system.users data persists and is queryable.

**Acceptance Scenarios**:

1. **Given** the system initializes for the first time, **When** the database starts, **Then** system tables are created in storage with the same RocksDB/Parquet architecture as shared tables
2. **Given** a DBA queries system.jobs, **When** the query executes, **Then** it uses the standard TableProvider interface and retrieves data from both RocksDB buffer and Parquet files
3. **Given** a system table reaches flush threshold, **When** the flush job executes, **Then** buffered rows are written to Parquet files and removed from RocksDB exactly like user tables

---

### User Story 4 - Virtual Views Support (Priority: P3)

Developers need the ability to define and query views that present alternative schemas over existing tables without physically storing data. Views should be registered in the SchemaRegistry and queryable via standard SQL SELECT statements.

**Why this priority**: This is P3 because views are a convenience feature that improves developer experience but aren't required for core database operations. They can be implemented after foundational refactoring is stable.

**Independent Test**: Can be tested by creating a view over information_schema.columns that filters to show only indexed columns, querying the view, and verifying results match the underlying table filter.

**Acceptance Scenarios**:

1. **Given** a developer defines a view `v_active_users` as `SELECT * FROM system.users WHERE deleted_at IS NULL`, **When** the view is registered in SchemaRegistry, **Then** it appears in information_schema.tables with table_type='VIEW'
2. **Given** a view is registered, **When** a user executes `SELECT * FROM v_active_users`, **Then** the query planner transparently rewrites it to the underlying table query
3. **Given** the underlying table schema changes, **When** the SchemaRegistry invalidates cache, **Then** dependent views are also invalidated and rebuilt on next access

---

### Edge Cases

- What happens when SchemaRegistry cache size exceeds memory limits? Should implement LRU eviction.
- How does the system handle Arrow schema cache invalidation when tables are dropped? Must remove all cached entries.
- What if LiveQueryManager registry grows unbounded with abandoned subscriptions? Need periodic cleanup job.
- How does system table initialization handle upgrades where new system tables are added? Must detect missing tables and create them.
- What happens if a view's underlying table is dropped? Query should fail with clear error message indicating broken view dependency.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST rename schema_cache.rs module to SchemaRegistry throughout the codebase
- **FR-002**: SchemaRegistry MUST cache constructed Arrow schemas with DashMap-based memoization for zero-allocation repeated access
- **FR-003**: SchemaRegistry MUST expose get_arrow_schema() method that returns cached Arc<Schema> on subsequent calls
- **FR-004**: SchemaRegistry cache MUST invalidate Arrow schemas when corresponding TableDefinition is modified via ALTER TABLE
- **FR-005**: System MUST consolidate UserConnections, UserTableChangeDetector, and LiveQueryManager into a single LiveQueryManager struct
- **FR-006**: Unified LiveQueryManager MUST maintain registry of all active subscriptions with their WebSocket connections, filters, and metadata
- **FR-007**: System MUST initialize system tables (users, jobs, namespaces, storages, live_queries, tables) using the same storage backend as shared tables
- **FR-008**: System tables MUST support flush operations that write buffered rows from RocksDB to Parquet files
- **FR-009**: System MUST support registering views in SchemaRegistry with view definitions stored separately from physical tables
- **FR-010**: Views MUST be queryable via SELECT statements with transparent rewriting to underlying table queries
- **FR-011**: SchemaRegistry MUST track view dependencies on base tables for cascade invalidation
- **FR-012**: System MUST ensure all unit tests and integration tests compile and pass after refactoring

### Key Entities

- **SchemaRegistry**: Centralized registry that manages TableDefinition cache, Arrow schema cache (new), and view definitions. Replaces scattered schema_cache references.
- **CachedTableData**: Extended to include Arc<Schema> field for memoized Arrow schemas alongside existing TableDefinition
- **LiveQueryManager**: Consolidated struct containing subscription registry (RwLock<LiveQueryRegistry>), user connections (HashMap<ConnectionId, UserConnectionSocket>), filter cache (RwLock<FilterCache>), initial data fetcher, schema registry reference, and node ID
- **SystemTableStorage**: New abstraction for system tables that uses standard TableProvider + StorageBackend pattern instead of custom system table logic
- **ViewDefinition**: Metadata structure for virtual views containing view name, SQL definition, dependent table IDs, and cached rewritten query plan

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Arrow schema cache hit rate exceeds 99% for workloads with stable table schemas
- **SC-002**: Schema lookup latency reduces from 50-100μs (current) to under 2μs (cached) for repeated accesses
- **SC-003**: LiveQueryManager consolidation reduces subscription management code by at least 30% (lines of code metric)
- **SC-004**: System table queries perform within 10% of equivalent shared table queries (no performance regression)
- **SC-005**: All 477 existing kalamdb-core tests pass without modification or with minimal fixture updates
- **SC-006**: View queries return results within 5% of direct table query performance (minimal rewriting overhead)
- **SC-007**: Memory usage for schema caching increases by less than 50MB for workloads with 1000 tables
- **SC-008**: System table initialization completes in under 100ms on database startup

## Assumptions *(optional)*

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

## Out of Scope *(optional)*

- Materialized views (views that physically store computed results)
- Cross-database views (views spanning multiple KalamDB instances)
- Updateable views (INSERT/UPDATE/DELETE through views)
- View permission inheritance from base tables (security model extension)
- Automatic view dependency detection for circular reference prevention
- Distributed live query federation across multiple nodes
