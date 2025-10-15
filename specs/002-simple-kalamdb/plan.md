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
- **RocksDB 0.21**: Fast write path (<1ms), column families for table isolation
- **Apache Arrow 50.0**: Zero-copy in-memory data format
- **Apache Parquet 50.0**: Compressed columnar storage with bloom filters
- **DataFusion 35.0**: SQL query engine with custom TableProvider integration
- **Actix-Web 4.4**: HTTP server and WebSocket actor framework
- **tokio 1.35**: Async runtime for concurrent operations
- **serde 1.0**: JSON serialization for configuration and API
- **serde_json 1.0**: JSON handling for schemas and responses

**Storage Architecture**:
- **Hot tier**: RocksDB for write buffering (<1ms latency)
- **Cold tier**: Parquet files for analytics-ready persistence
- **Column families**: Isolated storage per table type (user/shared/system/stream)
- **Bloom filters**: _updated timestamp for efficient time-range queries

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

**Project Type**: Multi-crate Rust workspace (kalamdb-core, kalamdb-api, kalamdb-server)

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
├── config.toml                                # Runtime configuration
├── config.example.toml                        # Configuration template
├── crates/
│   ├── kalamdb-core/                         # Core library (embeddable)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                        # Public API exports
│   │       ├── error.rs                      # Error types
│   │       ├── catalog/                      # Namespace and table metadata
│   │       ├── config/                       # Configuration management
│   │       ├── schema/                       # Arrow schema management
│   │       ├── storage/                      # Storage backends
│   │       ├── tables/                       # Table providers
│   │       ├── sql/                          # SQL execution
│   │       ├── flush/                        # Flush policy management
│   │       ├── live_query/                   # Live subscription management
│   │       ├── jobs/                         # Background job framework
│   │       └── services/                     # Business logic services
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
│           ├── main.rs                       # Server entry point
│           ├── config.rs                     # Config loading
│           └── logging.rs                    # Logging setup
├── conf/                                      # Configuration persistence
│   ├── namespaces.json                       # Namespace registry
│   └── storage_locations.json                # Storage location registry
└── tests/
    └── integration/                          # Integration tests
        ├── test_end_to_end.rs                # Complete workflow test
        └── ...                               # Other test files
```

**Structure Decision**: Multi-crate Rust workspace with clear separation of concerns:
- **kalamdb-core**: Embeddable library with all business logic
- **kalamdb-api**: HTTP/WebSocket API layer
- **kalamdb-server**: Minimal binary entry point

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

**Naming Convention**:
- User tables: `user_table:{namespace}:{table_name}`
- Shared tables: `shared_table:{namespace}:{table_name}`
- System tables: `system_table:{table_name}`
- Stream tables: `stream_table:{namespace}:{table_name}`

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

### Decision 8: JSON Configuration Files
**Rationale**: Human-readable, Git-friendly, simple backup/restore. RocksDB is ONLY for table data and system tables.


## Phase 1: Design & Contracts

**Status**: ✅ **COMPLETE**

All Phase 1 artifacts have been created:

### ✅ data-model.md
- 4 core entities documented (Namespace, TableMetadata, FlushPolicy, StorageLocation)
- 4 system tables defined (users, live_queries, storage_locations, jobs)
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

**Status**: ✅ **COMPLETE** (tasks.md already exists)

The task breakdown covers all implementation work:
- Phase 1: Setup & Code Removal (13 tasks)
- Phase 2: Foundational (37 tasks)
- Phases 3-12: User Stories (150+ tasks)

## Implementation Readiness

**Status**: ✅ **READY TO IMPLEMENT**

All prerequisites met:
- ✅ spec.md (feature specification)
- ✅ plan.md (this file - technical plan)
- ✅ research.md (technical decisions)
- ✅ data-model.md (entities and relationships)
- ✅ contracts/ (API specifications)
- ✅ quickstart.md (end-to-end test guide)
- ✅ tasks.md (implementation tasks)
- ✅ checklists/requirements.md (all items passed)

**Next Command**: Run `/speckit.implement` to begin Phase 1 implementation (Setup & Code Removal).

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
**Complexity**: Medium-High (multi-tier storage, real-time subscriptions, SQL execution)  
**Timeline**: ~6-8 weeks for complete implementation (following tasks.md)  
**Risk**: Low (all technical decisions validated, clear task breakdown)

**Key Innovations**:
- Table-per-user architecture for massive scalability
- Hybrid RocksDB+Parquet storage for sub-millisecond writes + analytics
- Actor-based WebSocket subscriptions for real-time notifications
- DataFusion integration for standard SQL support

**Architecture Highlights**:
- Multi-crate Rust workspace (core/api/server)
- Column family isolation per table
- In-memory + RocksDB hybrid for live query routing
- JSON configuration files for metadata
- Parquet bloom filters for query optimization

**Documentation**: Complete
- 8 specification/planning documents
- 2 API contracts (OpenAPI + WebSocket protocol)
- 1 comprehensive testing guide
- 186 detailed implementation tasks

Ready to proceed with implementation! 🚀
