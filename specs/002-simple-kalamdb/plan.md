# Implementation Plan: Simple KalamDB - User-Based Database with Live Queries

**Branch**: `002-simple-kalamdb` | **Date**: 2025-10-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-simple-kalamdb/spec.md`

## Summary

KalamDB is a user-centric database server with a table-per-user architecture that enables massive scalability for real-time subscriptions. The system provides:

- **Namespace-based organization** with user/shared/stream table types
- **Two-tier storage**: RocksDB for sub-millisecond writes, Parquet for analytics-ready persistence
- **Live query subscriptions** via WebSocket with change tracking (INSERT/UPDATE/DELETE notifications)
- **System tables** for managing users, storage locations, live queries, and background jobs
- **REST API** for SQL execution and WebSocket for real-time data streaming
- **Complete end-to-end testing** including server startup, table creation, queries, and live subscriptions

The table-per-user architecture allows millions of concurrent users to maintain real-time subscriptions without performance degradation, achieving O(1) complexity per user through isolated storage partitions.

## Technical Context

**Language/Version**: Rust 1.75+ (stable toolchain, edition 2021)

**Primary Dependencies**:
- **RocksDB 0.21**: Fast write path (<1ms), column families for table isolation + system tables
- **Apache Arrow 50.0**: Zero-copy in-memory data format
- **Apache Parquet 50.0**: Compressed columnar storage with bloom filters
- **DataFusion 35.0**: SQL query engine with custom TableProvider integration
- **sqlparser-rs**: SQL parsing library (used by kalamdb-sql crate for system tables)
- **Actix-Web 4.4**: HTTP server and WebSocket actor framework
- **tokio 1.35**: Async runtime for concurrent operations
- **serde 1.0**: JSON serialization for API responses and Arrow schemas
- **serde_json 1.0**: JSON handling for schemas and responses

**Storage Architecture**:
- **Hot tier**: RocksDB for write buffering (<1ms latency)
- **Cold tier**: Parquet files for analytics-ready persistence
- **Column families**: Isolated storage per table type (user/shared/system/stream) plus dedicated CFs for system tables and metadata
- **RocksDB-only metadata**: All configuration (namespaces, storage_locations, tables, schemas) stored in RocksDB system tables - NO JSON config files
- **System tables**: 7 tables (users, live_queries, storage_locations, jobs, namespaces, tables, table_schemas) managed via unified kalamdb-sql crate
- **Bloom filters**: _updated timestamp for efficient time-range queries
- **Per-user flush tracking**: user_table_counters CF tracks row counts per user per table for flush policy

**API Layer**:
- **REST API**: Single `/api/sql` POST endpoint for all SQL operations
- **WebSocket**: `/ws` endpoint for live query subscriptions
- **Authentication**: JWT tokens for user identification (CURRENT_USER() function)
- **CORS**: Enabled for web browser clients

**Testing Strategy**:
- **Unit tests**: cargo test for individual modules
- **Integration tests**: End-to-end scenarios with running server
- **Property-based tests**: Arrow schema transformations and serialization
- **Benchmarks**: criterion.rs for write/query performance tracking

**Target Platform**: Linux/macOS server, embeddable library option

**Project Type**: Multi-crate Rust workspace (kalamdb-core, kalamdb-sql, kalamdb-api, kalamdb-server)

**Performance Goals**:
- Writes: <1ms p99 (RocksDB path)
- Queries: Leverage DataFusion's optimization (projection pushdown, filter pushdown)
- Live notifications: <10ms from write to WebSocket delivery

**Constraints**:
- Zero-copy efficiency via Arrow IPC
- User isolation enforcement (table-per-user model)
- No shared table locks (column family isolation)
- Memory-efficient stream tables (TTL-based eviction)

**Scale/Scope**:
- Millions of concurrent users with active subscriptions
- Real-time chat and AI conversation storage
- User-partitioned data for horizontal scaling
- Hybrid streaming + durable persistence

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I: Simplicity First ✅
- **Compliant**: Single REST endpoint `/api/sql` for all SQL operations
- **Compliant**: DataFusion handles SQL parsing/execution (no custom SQL engine)
- **Compliant**: Standard WebSocket protocol for subscriptions
- **Compliant**: JSON configuration files (no complex config formats)

### Principle II: Performance by Design ✅
- **Compliant**: RocksDB for <1ms write path
- **Compliant**: Parquet for compressed analytics storage
- **Compliant**: DataFusion for SQL query execution
- **Planned**: Benchmark suite with criterion.rs for continuous monitoring

### Principle III: Data Ownership ✅
- **Compliant**: Table-per-user architecture with isolated partitions
- **Compliant**: User-specific Parquet files: `{userId}/batch-*.parquet`
- **Compliant**: Clear data boundaries enforced by column families
- **Compliant**: Export-friendly format (standard Parquet files)

### Principle IV: Zero-Copy Efficiency ✅
- **Compliant**: Arrow as in-memory format throughout the pipeline
- **Compliant**: Parquet files reduce deserialization overhead
- **Compliant**: DataFusion's zero-copy RecordBatch processing

### Principle V: Open & Extensible ✅
- **Compliant**: Modular crate structure (core/api/server)
- **Compliant**: Embeddable library (kalamdb-core)
- **Compliant**: Standard protocols (SQL, WebSocket, REST)

### Principle VI: Transparency ✅
- **Compliant**: system.jobs table tracks all background operations
- **Compliant**: system.live_queries shows active subscriptions
- **Compliant**: Metrics captured for all flush operations

### Principle VII: Test-Driven Stability ✅
- **Planned**: Integration tests for all user stories
- **Planned**: Contract tests for REST API
- **Planned**: WebSocket subscription tests
- **Planned**: Concurrent write/query tests

### Principle VIII: Self-Documenting Code ✅
- **Planned**: Module-level rustdoc for all crates
- **Planned**: Inline comments for complex algorithms (bloom filters, column family naming)
- **Planned**: Architecture Decision Records for key choices
- **Planned**: Code examples in public API documentation

**Documentation Requirements (Principle VIII - Self-Documenting Code)**:
- [x] Module-level rustdoc comments planned for all new modules
- [x] Public API documentation strategy defined (structs, enums, traits, functions)
- [x] Inline comment strategy for complex algorithms and architectural patterns identified
- [x] Architecture Decision Records (ADRs) planned for key design choices
- [x] Code examples planned for non-trivial public APIs

**Overall Assessment**: ✅ **PASSED** - No violations requiring justification


## Project Structure

### Documentation (this feature)

```
specs/002-simple-kalamdb/
├── plan.md                                    # This file
├── research.md                                # Phase 0 output (technical decisions)
├── data-model.md                              # Phase 1 output (entities and relationships)
├── quickstart.md                              # Phase 1 output (getting started guide)
├── contracts/                                 # Phase 1 output (API specifications)
│   ├── rest-api.yaml                         # OpenAPI spec for /api/sql endpoint
│   └── websocket-protocol.md                 # WebSocket message format
├── tasks.md                                   # Already exists
└── checklists/
    └── requirements.md                        # Already exists
```

### Source Code (repository root)

```
backend/
├── Cargo.toml                                 # Workspace manifest
├── config.toml                                # Runtime configuration (logging, ports, paths)
├── config.example.toml                        # Configuration template
├── crates/
│   ├── kalamdb-core/                         # Core library - NO direct RocksDB imports
│   │   ├── Cargo.toml                        # Dependencies: kalamdb-sql, kalamdb-store, datafusion, arrow, parquet
│   │   └── src/
│   │       ├── lib.rs                        # Public API exports
│   │       ├── error.rs                      # Error types
│   │       ├── catalog/                      # Namespace and table metadata (uses kalamdb-sql)
│   │       ├── config/                       # Configuration management
│   │       ├── schema/                       # Arrow schema management
│   │       ├── storage/                      # Storage backends (delegates to kalamdb-store)
│   │       ├── tables/                       # Table providers (use kalamdb-store for data ops)
│   │       ├── sql/                          # SQL execution (DataFusion integration)
│   │       ├── flush/                        # Flush policy management
│   │       ├── live_query/                   # Live subscription management
│   │       ├── jobs/                         # Background job framework
│   │       └── services/                     # Business logic services
│   ├── kalamdb-sql/                          # Unified SQL engine for system tables
│   │   ├── Cargo.toml                        # Dependencies: rocksdb, serde, serde_json, sqlparser, chrono, anyhow
│   │   └── src/
│   │       ├── lib.rs                        # Public API exports (execute, scan_all methods)
│   │       ├── models.rs                     # Rust types for 7 system tables
│   │       ├── parser.rs                     # SQL parsing (sqlparser-rs)
│   │       ├── executor.rs                   # SQL execution logic
│   │       └── adapter.rs                    # RocksDB read/write adapter (owns system_* CFs)
│   ├── kalamdb-store/                        # K/V store for user/shared/stream tables
│   │   ├── Cargo.toml                        # Dependencies: rocksdb, serde, serde_json, chrono, anyhow
│   │   └── src/
│   │       ├── lib.rs                        # Public API exports
│   │       ├── user_table_store.rs           # UserTableStore: put/get/delete/scan_user
│   │       ├── shared_table_store.rs         # SharedTableStore: put/get/delete/scan
│   │       ├── stream_table_store.rs         # StreamTableStore: put/get/delete with TTL
│   │       └── key_encoding.rs               # Key format utilities ({UserId}:{row_id}, etc.)
│   ├── kalamdb-api/                          # REST API and WebSocket
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                        # Public API exports
│   │       ├── routes.rs                     # Route definitions
│   │       ├── handlers/                     # HTTP/WebSocket handlers
│   │       ├── actors/                       # Actix actors
│   │       └── models/                       # API request/response models
│   └── kalamdb-server/                       # Server binary
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                       # Server entry point (initializes kalamdb-sql + kalamdb-store)
│           ├── config.rs                     # Config loading
│           └── logging.rs                    # Logging setup
└── tests/
    └── integration/                          # Integration tests
        ├── test_end_to_end.rs                # Complete workflow test
        └── ...                               # Other test files
```

**Structure Decision**: Multi-crate Rust workspace with three-layer architecture:
- **kalamdb-core**: Embeddable library with all business logic (NO direct RocksDB imports)
- **kalamdb-sql**: Unified SQL engine for system table operations (owns system_* column families)
- **kalamdb-store**: K/V store for user/shared/stream tables (owns user_table:*, shared_table:*, stream_table:* CFs)
- **kalamdb-api**: HTTP/WebSocket API layer
- **kalamdb-server**: Minimal binary entry point

**Key Architecture Changes** (from spec clarifications):
- **RocksDB-only metadata**: All namespace/table/schema metadata stored in RocksDB system tables (no JSON config files)
- **7 system tables**: users, live_queries, storage_locations, jobs, namespaces, tables, table_schemas
- **kalamdb-sql crate**: Unified CRUD operations for all system tables via SQL interface (eliminates 7x code duplication)
- **kalamdb-store crate**: K/V store abstraction for user/shared/stream tables (isolates RocksDB from business logic)
- **Three-layer architecture**: kalamdb-core (NO RocksDB) → kalamdb-sql (system metadata) + kalamdb-store (user data) → RocksDB
- **Future-ready**: Architecture prepared for distributed replication via future kalamdb-raft crate

## Complexity Tracking

*No violations detected - Constitution Check passed.*

## Phase 0: Research & Technical Decisions

**Status**: ✅ **COMPLETE**

All technical decisions have been resolved:

### Decision 1: DataFusion for SQL Execution
**Rationale**: Production-ready SQL parsing, query planning, and execution with zero-copy Arrow integration.

**Alternatives Considered**:
- Custom SQL parser (rejected: high complexity)
- SQLite FFI (rejected: doesn't support custom storage backends)

### Decision 2: RocksDB Column Families for Table Isolation
**Rationale**: Column families provide isolated storage per table with separate memtables and write buffers.

**Column Family Naming Convention**:
- User tables: `user_table:{namespace}:{table_name}`
- Shared tables: `shared_table:{namespace}:{table_name}`
- System tables: `system_{table_name}` (e.g., `system_users`, `system_namespaces`, `system_tables`, `system_table_schemas`, `system_live_queries`, `system_storage_locations`, `system_jobs`)
- Stream tables: `stream_table:{namespace}:{table_name}`
- Flush tracking: `user_table_counters` (stores per-user-per-table row counts for flush policy)

**System Tables** (7 total):
1. system_users: User authentication and permissions
2. system_live_queries: Active WebSocket subscriptions
3. system_storage_locations: Predefined storage locations
4. system_jobs: Background job history and metrics
5. system_namespaces: Namespace registry and options
6. system_tables: Table metadata and configuration
7. system_table_schemas: Arrow schema versions and history

**Update from Clarification Q2**: All metadata (namespaces, storage_locations, tables, schemas) is stored ONLY in RocksDB. JSON config files have been eliminated from the architecture.

### Decision 3: Actix-Web Actor Model for WebSocket Sessions
**Rationale**: Battle-tested actor framework for WebSocket connection management with message passing and lifecycle management.

### Decision 4: In-Memory Registry for Live Query Routing
**Rationale**: WebSocket actor addresses cannot be persisted. In-memory registry provides fast lookup.

**RocksDB stores**: system.live_queries metadata
**In-memory stores**: connection_id → actor address, live_id → connection_id

### Decision 5: Arrow Schema JSON Serialization
**Rationale**: DataFusion provides built-in `SchemaRef::to_json()` and `SchemaRef::from_json()` for schema versioning.

### Decision 6: Parquet Bloom Filters on _updated Column
**Rationale**: Enable efficient time-range queries by skipping files that don't overlap the query window.

### Decision 7: JWT for User Authentication
**Rationale**: Stateless authentication with user_id claim for CURRENT_USER() function.

### Decision 8: RocksDB-Only for Metadata Storage
**Rationale**: Simplifies architecture by eliminating dual JSON+RocksDB persistence. All metadata lives in system tables with transactional consistency.

**What Changed**: Originally, namespaces and storage_locations were stored in JSON files (`conf/namespaces.json`, `conf/storage_locations.json`). Now ALL metadata is stored in RocksDB system tables:
- system_namespaces (replaces namespaces.json)
- system_storage_locations (replaces storage_locations.json)
- system_tables (new - table metadata)
- system_table_schemas (new - Arrow schema versions)

**Benefits**:
- Single source of truth for all metadata
- Transactional consistency for metadata operations
- No file system sync issues
- Simplified backup/restore (RocksDB snapshots)
- Consistent query interface via kalamdb-sql

**Clarification Q2**: "namespaces should be only a rocksdb columnfamily table and not in disk" - This decision eliminates all JSON config files from the architecture.

### Decision 9: Unified kalamdb-sql Crate for System Tables
**Rationale**: Eliminates code duplication by providing a single SQL-based interface for all 7 system tables.

**Problem Solved**: Without unified crate, we'd need 7 separate CRUD implementations for each system table (users, live_queries, storage_locations, jobs, namespaces, tables, table_schemas). This creates:
- ~7x code duplication
- Inconsistent API patterns
- Higher maintenance burden
- More bugs from repeated logic

**Architecture**:
- **models.rs**: Rust structs for 7 system tables (User, LiveQuery, StorageLocation, Job, Namespace, Table, TableSchema)
- **parser.rs**: SQL parsing using sqlparser-rs (SELECT, INSERT, UPDATE, DELETE)
- **executor.rs**: SQL execution logic (query planning, filtering, projections)
- **adapter.rs**: RocksDB read/write operations (key encoding, batch operations)

**Usage Pattern**:
```rust
// kalamdb-core uses kalamdb-sql for all system table operations
let sql_engine = KalamSql::new(rocksdb_handle);

// Query system tables via SQL
let users = sql_engine.execute("SELECT * FROM system.users WHERE username = 'alice'")?;

// Insert into system tables
sql_engine.execute("INSERT INTO system.namespaces (namespace_id, name, options) VALUES (?, ?, ?)")?;
```

**Future-Ready**: Architecture designed to support distributed replication via future kalamdb-raft crate. System table mutations will be replicated as Raft log entries containing SQL operations.

**Clarification Q3**: "create a separate crate called kalamdb-sql...unified usage of system table"


## Phase 1: Design & Contracts

**Status**: ⚠️ **NEEDS UPDATE** (data-model.md must reflect 7 system tables + kalamdb-sql architecture)

### ✅ data-model.md
- 4 core entities documented (Namespace, TableMetadata, FlushPolicy, StorageLocation)
- **UPDATE NEEDED**: Add 3 new system tables (namespaces, tables, table_schemas) to data model
- **UPDATE NEEDED**: Document kalamdb-sql models and relationships
- Entity relationships and validation rules specified
- RocksDB key formats and column family naming conventions
- Type-safe wrappers (NamespaceId, TableName, UserId, etc.)
- State transition diagrams for all entities

### ✅ contracts/rest-api.yaml
- OpenAPI 3.0 specification for `/api/sql` endpoint
- Request/response schemas with examples
- Error responses (400, 401, 500)
- JWT authentication specification
- Multiple statement execution support

### ✅ contracts/websocket-protocol.md
- Complete WebSocket message protocol
- Client→Server messages (subscribe, unsubscribe, ping)
- Server→Client messages (initial_data, change notifications, error)
- Subscription lifecycle documentation
- Client implementation examples (JavaScript, Rust)
- Performance characteristics and limitations

### ✅ quickstart.md
- Complete end-to-end testing guide
- Step-by-step instructions from server startup to live queries
- REST API examples (namespace, table creation, CRUD operations)
- WebSocket subscription examples (initial data, INSERT/UPDATE/DELETE notifications)
- Automated test script
- Verification checklist
- Troubleshooting guide

## Phase 2: Tasks

**Status**: ⚠️ **NEEDS REGENERATION** (tasks.md must be updated for kalamdb-sql crate + RocksDB-only metadata)

The task breakdown must be updated to cover:
- kalamdb-sql crate implementation (models, parser, executor, adapter)
- RocksDB system table creation (7 tables, not 4)
- Removal of JSON config file logic
- Integration of kalamdb-sql into kalamdb-core
- Updated flush policy tracking (user_table_counters CF)

**Recommendation**: Run `/speckit.tasks` to regenerate tasks.md based on updated spec.md

## Implementation Readiness

**Status**: ⚠️ **NEEDS ARTIFACT UPDATES**

Prerequisites status:
- ✅ spec.md (feature specification - updated with 7 clarifications, kalamdb-sql, Raft prep)
- ✅ plan.md (this file - updated with new architecture)
- ✅ research.md (technical decisions - updated with new decisions 8-9)
- ⚠️ data-model.md (needs update for 7 system tables + kalamdb-sql models)
- ✅ contracts/ (API specifications - no changes needed)
- ✅ quickstart.md (end-to-end test guide - no changes needed)
- ⚠️ tasks.md (needs regeneration via /speckit.tasks)
- ✅ checklists/requirements.md (all items passed)

**Next Steps**:
1. Update data-model.md to include:
   - 7 system tables with complete schemas
   - kalamdb-sql crate models and relationships
   - Updated RocksDB column family architecture
2. Run `/speckit.tasks` to regenerate tasks.md with:
   - kalamdb-sql crate implementation tasks
   - RocksDB-only metadata tasks
   - Elimination of JSON config file logic
   - Integration tasks for kalamdb-sql into core

**After Updates Complete**: Run `/speckit.implement` to begin implementation

## Test Strategy

The quickstart.md serves as the acceptance test for the complete system:

1. **Setup Test**: Server starts successfully
2. **REST API Test**: Create namespace, create table, insert/query data
3. **WebSocket Test**: Establish connection, subscribe, receive initial data
4. **Live Query Test**: Verify INSERT/UPDATE/DELETE notifications
5. **System Tables Test**: Query system.live_queries, verify metadata
6. **Performance Test**: Verify <1ms writes, <10ms notifications

This validates end-to-end functionality matching the user's requirements: "test the server with complete functionality which will run the server, using rest api POST create a user table, query it using rest api post query, and also try to listen to live changes using websocket."

## Summary

**Feature**: Simple KalamDB - User-Based Database with Live Queries  
**Complexity**: Medium-High (multi-tier storage, real-time subscriptions, SQL execution, unified system table engine)  
**Timeline**: ~6-8 weeks for complete implementation (following tasks.md after regeneration)  
**Risk**: Low (all technical decisions validated, clear task breakdown needed)

**Key Innovations**:
- Table-per-user architecture for massive scalability
- Hybrid RocksDB+Parquet storage for sub-millisecond writes + analytics
- Actor-based WebSocket subscriptions for real-time notifications
- DataFusion integration for standard SQL support
- **NEW**: Unified kalamdb-sql crate eliminates system table code duplication
- **NEW**: RocksDB-only metadata (no JSON config files)
- **NEW**: Future-ready for distributed replication via kalamdb-raft

**Architecture Highlights**:
- Multi-crate Rust workspace (core/sql/api/server)
- 7 system tables with unified SQL interface
- Column family isolation per table
- In-memory + RocksDB hybrid for live query routing
- RocksDB-only metadata persistence (simplified architecture)
- Parquet bloom filters for query optimization
- Per-user-per-table flush tracking

**Major Architecture Changes** (from spec clarifications):
1. **Eliminated JSON config files**: All metadata now in RocksDB system tables
2. **Added 3 system tables**: namespaces, tables, table_schemas (total: 7)
3. **New kalamdb-sql crate**: Unified SQL engine for all system table operations
4. **New kalamdb-store crate**: K/V store abstraction for user/shared/stream tables (see Architecture Refactoring Phase below)
5. **Three-layer separation**: kalamdb-core (business logic) → kalamdb-sql + kalamdb-store (storage) → RocksDB

## Architecture Refactoring Phase (Before Phase 10)

**Status**: Planned (execute after Phase 9 completion)  
**Priority**: Critical for clean architecture before Phase 10 (table deletion)

### Motivation

After Phase 9 implementation (user table operations), code review revealed direct RocksDB coupling in kalamdb-core:

**Current Problems**:
1. System table providers use `CatalogStore` (direct RocksDB operations)
2. User table handlers import `rocksdb::DB` directly
3. kalamdb-core has RocksDB dependencies it shouldn't have
4. Difficult to test business logic without mocking RocksDB

**Solution**: Three-Layer Architecture

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

### Benefits

✅ **Clear Separation**: System metadata vs user data operations  
✅ **RocksDB Isolation**: Only kalamdb-sql and kalamdb-store import RocksDB  
✅ **Easier Testing**: Mock interfaces instead of RocksDB  
✅ **Code Clarity**: Intent clear from which crate is used  
✅ **Future Flexibility**: Can swap storage backends independently  
✅ **Simplified Dependencies**: kalamdb-core doesn't need column family or key encoding knowledge

### Migration Tasks Overview

**Phase A: Create kalamdb-store crate** (~10 tasks)
- Create crate structure with Cargo.toml
- Implement UserTableStore with put/get/delete/scan_user methods
- Implement SharedTableStore for shared tables
- Implement StreamTableStore with TTL support
- Add key_encoding.rs for key format utilities
- Comprehensive unit tests

**Phase B: Enhance kalamdb-sql** (~7 tasks)
- Add scan_all_users() method and iterator
- Add scan_all_namespaces(), scan_all_storage_locations(), etc. (7 methods total)
- Update adapter.rs with RocksDB iteration logic
- Update lib.rs to expose new public API
- Add tests for scan methods

**Phase C: Refactor system table providers** (~8 tasks)
- Update UsersTableProvider to use Arc<KalamSql>
- Update StorageLocationsTableProvider
- Update LiveQueriesTableProvider
- Update JobsTableProvider
- Update all tests to use KalamSql instead of CatalogStore
- Update service constructors

**Phase D: Refactor user table handlers** (~6 tasks)
- Update UserTableInsertHandler to use UserTableStore
- Update UserTableUpdateHandler to use UserTableStore
- Update UserTableDeleteHandler to use UserTableStore
- Remove RocksDB imports from these files
- Update all handler tests
- Update dependency injection in main.rs

**Phase E: Deprecate CatalogStore** (~2 tasks)
- Mark CatalogStore as #[deprecated]
- Add migration guide documentation

**Total**: ~35-40 tasks, estimated 6-9 days

### Success Criteria

- [ ] kalamdb-store crate exists with full test coverage
- [ ] All 7 system tables have scan_all methods in kalamdb-sql
- [ ] All 4 system table providers use kalamdb-sql only
- [ ] All 3 user table handlers use kalamdb-store only
- [ ] kalamdb-core has ZERO direct rocksdb imports
- [ ] All existing tests pass
- [ ] CatalogStore marked as deprecated

**Implementation Plan**: See detailed tasks in tasks.md Phase 9.5 (Architecture Refactoring)

---
4. **Future Raft preparation**: Architecture designed for distributed replication

**Documentation**: Needs Updates
- 8 specification/planning documents (plan.md updated ✅)
- 2 API contracts (no changes needed ✅)
- 1 comprehensive testing guide (no changes needed ✅)
- data-model.md needs update for 7 system tables ⚠️
- tasks.md needs regeneration via /speckit.tasks ⚠️

**Status**: Awaiting artifact updates before implementation can proceed �
