# Implementation Plan: Chat and AI Message History Storage System

**Branch**: `001-build-a-rust` | **Date**: 2025-10-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-build-a-rust/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Build KalamDB - a **SQL-first** chat and AI message history storage system that combines persistent storage with real-time streaming in a single Rust-based server. 

**Key Innovation**: Universal SQL interface (`POST /api/v1/query`) for all data operations - no specialized CRUD endpoints needed.

The system buffers writes in RocksDB (<1ms latency), consolidates to Parquet files (efficient storage + SQL querying via DataFusion), and streams real-time updates via WebSocket. 

**Two Conversation Types**:
1. **AI conversations**: User-owned storage (messages + content in `{userId}/`)
2. **Group conversations**: Messages duplicated per user, large content (≥100KB) and media stored once in shared folder

**Deletion Support**: SQL DELETE cascades to files with reference counting for shared content.

Messages use snowflake IDs for ordering and support flexible JSON metadata. Admin UI (React + Vite) uses same SQL interface.

## Technical Context

**Architecture**: Full-stack application with Rust backend (Actix-based server) and React frontend (Vite)

**Backend Stack**:
- **Language**: Rust 1.75+ (stable toolchain only)
- **Web Framework**: Actix Web (REST APIs + WebSocket support)
- **Write Buffer**: RocksDB (sub-millisecond write latency, WAL for durability)
- **Storage Format**: Apache Parquet (columnar, compressed, analytics-ready)
- **Query Engine**: DataFusion (SQL execution over Parquet files)
- **Data Format**: Apache Arrow (zero-copy in-memory representation, Arrow IPC for serialization)
- **Async Runtime**: Tokio (multi-threaded async executor)
- **Serialization**: Serde (JSON for APIs, Arrow for data)
- **Authentication**: JWT token validation (jsonwebtoken crate)
- **ID Generation**: Snowflake IDs for time-ordered unique identifiers
- **Configuration**: TOML-based config file (e.g., message size limits, consolidation thresholds)

**Frontend Stack**:
- **Framework**: React 18+ with TypeScript
- **Build Tool**: Vite (fast HMR, optimized production builds)
- **Communication**: REST API calls (fetch/axios) + WebSocket for real-time subscriptions
- **UI Components**: [NEEDS CLARIFICATION: UI component library - Material-UI, Ant Design, or custom?]
- **State Management**: [NEEDS CLARIFICATION: State management approach - React Context, Zustand, Redux Toolkit?]
- **Routing**: React Router for admin UI navigation

**Storage Architecture**:
- **Write Path**: Messages written to RocksDB immediately (target <1ms p99 latency)
- **Consolidation**: Background task flushes RocksDB to Parquet every 5 min OR 10k messages
- **File Organization**: `<storage-root>/<userId>/batch-<timestamp>-<index>.parquet`
- **Parquet Schema**: msgId (i64), conversationId (string), from (string), timestamp (i64), content (string), metadata (JSON string)
- **Row Groups**: Organized by conversationId for efficient filtering
- **Backends**: Support both local filesystem and S3-compatible object storage

**Data Model**:
- **Message**: Snowflake ID, conversationId, from (sender userId), timestamp (Unix microseconds), content, metadata (flexible JSON)
- **Conversation**: conversationId, firstMsgId, lastMsgId, created, updated (stored separately for fast lookups)
- **Subscription**: Active WebSocket connections with userId, conversationId filter, lastMsgId cursor

**Testing Strategy**:
- **Unit Tests**: cargo test for individual modules (RocksDB wrapper, Parquet writer, ID generation)
- **Integration Tests**: API contract tests, end-to-end message flow (write → consolidate → query)
- **Property-Based Tests**: proptest for data transformation invariants (Arrow ↔ Parquet roundtrip)
- **Benchmarks**: criterion.rs for write latency, consolidation throughput, query performance
- **Frontend Tests**: Vitest for React components, Playwright for E2E admin UI flows

**Performance Goals**:
- Message write acknowledgment: <100ms (p99)
- RocksDB write: <1ms (p99)
- Query 50 messages: <200ms for 100k message conversations
- SQL aggregation over 1M messages: <5s
- Real-time notification delivery: <500ms after write
- Throughput: 1000 messages/second sustained

**Deployment**:
- **Target**: Linux servers (x86_64), containerized (Docker)
- **Modes**: Standalone server (default), embeddable library (future)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle Alignment

**I. Simplicity First** ✅
- Direct code paths: RocksDB write → consolidation → Parquet → DataFusion query
- Clear separation: Backend (Rust/Actix) ↔ Frontend (React/Vite) via REST/WebSocket
- Minimal abstractions: Use established patterns (Actix handlers, React components)
- **Concern**: Admin UI adds complexity (React build pipeline, component state management)
  - **Justified**: Admin UI is essential for production operations (monitoring, troubleshooting, config)
  - **Mitigation**: Keep UI simple, use standard patterns, avoid over-engineering state management

**II. Performance by Design** ✅
- Writes <1ms via RocksDB with WAL
- Parquet provides 3x+ compression
- DataFusion enables standard SQL queries
- Benchmarks: criterion.rs for critical paths (write, consolidation, query)
- **Action**: Implement performance gates in CI (write latency, query benchmarks)

**III. Data Ownership** ✅
- User isolation: Data partitioned by `<userId>/` directory structure
- Export-friendly: Parquet files are standard, portable format
- JWT authentication enforces user identity verification
- **Note**: Encryption (AEAD) is planned for future but not in initial implementation
  - Constitution requires encryption as first-class feature
  - **Decision**: Mark as Phase 2+ enhancement, document in assumptions

**IV. Zero-Copy Efficiency** ✅
- Arrow IPC for serialization avoids deserialization overhead
- Parquet → Arrow → DataFusion query path minimizes copies
- WebSocket streaming uses efficient binary frames
- **Action**: Profile memory allocations, verify zero-copy paths in critical sections

**V. Open & Extensible** ✅
- Modular Rust backend (separate crates for storage, query, API)
- Frontend as separate React app (can be replaced/extended)
- Configuration via TOML (user-defined limits, backends)
- Future: Custom Parquet columns, pluggable auth
- **Action**: Design clean module boundaries (storage, query, API, streaming)

**VI. Transparency** ✅
- Structured logging with tracing crate (request IDs, timings)
- Admin UI dashboard exposes: storage stats, throughput, active subscriptions, performance metrics
- Events for state transitions: consolidation cycles, subscription connect/disconnect
- **Action**: Define logging standards, implement observability from start

**VII. Secure by Default** ⚠️
- JWT authentication required for all API endpoints
- User isolation enforced at storage partition boundaries
- **Gap**: AEAD encryption for message content not in initial scope
  - Constitution requires encryption as default
  - **Decision**: Document as assumption (encryption deferred to Phase 2+)
  - **Mitigation**: Transport security (TLS), JWT validation, tenant isolation prevent most threats

### Gates

| Gate | Status | Notes |
|------|--------|-------|
| Performance targets defined | ✅ | <1ms writes, <200ms queries, <5s SQL aggregations |
| User data isolation enforced | ✅ | Partitioned by userId at storage layer |
| JWT authentication required | ✅ | All API endpoints validate tokens |
| Zero-copy paths identified | ✅ | Arrow IPC, Parquet → DataFusion |
| Observability instrumented | ✅ | Structured logs, admin dashboard, metrics |
| AEAD encryption enabled | ⚠️ | **DEFERRED** to Phase 2+ (documented in assumptions) |

**Overall**: ✅ **PASS** (with documented exception for encryption deferral)

## Project Structure

### Documentation (this feature)

```
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```
kalamdb/                           # Repository root
│
├── backend/                       # Rust backend server
│   ├── Cargo.toml                 # Workspace root
│   ├── crates/
│   │   ├── kalamdb-core/          # Core storage engine
│   │   │   ├── src/
│   │   │   │   ├── lib.rs
│   │   │   │   ├── storage/       # RocksDB wrapper, Parquet writer
│   │   │   │   ├── models/        # Message, Conversation structs
│   │   │   │   ├── consolidation/ # Background consolidation task
│   │   │   │   └── ids/           # Snowflake ID generator
│   │   │   └── Cargo.toml
│   │   │
│   │   ├── kalamdb-query/         # SQL query engine (DataFusion wrapper)
│   │   │   ├── src/
│   │   │   │   ├── lib.rs
│   │   │   │   ├── engine.rs      # DataFusion context setup
│   │   │   │   └── planner.rs     # Query planning optimizations
│   │   │   └── Cargo.toml
│   │   │
│   │   ├── kalamdb-api/           # REST API + WebSocket server
│   │   │   ├── src/
│   │   │   │   ├── lib.rs
│   │   │   │   ├── handlers/      # Actix request handlers
│   │   │   │   ├── websocket/     # WebSocket subscription manager
│   │   │   │   ├── auth/          # JWT validation middleware
│   │   │   │   └── routes.rs      # Route definitions
│   │   │   └── Cargo.toml
│   │   │
│   │   └── kalamdb-server/        # Main server binary
│   │       ├── src/
│   │       │   ├── main.rs        # Entry point, config loading
│   │       │   └── config.rs      # TOML config parsing
│   │       └── Cargo.toml
│   │
│   ├── tests/
│   │   ├── integration/           # End-to-end API tests
│   │   ├── contracts/             # OpenAPI contract validation
│   │   └── benchmarks/            # Criterion performance benchmarks
│   │
│   └── config.example.toml        # Example configuration
│
├── frontend/                      # React admin UI
│   ├── src/
│   │   ├── main.tsx               # Entry point
│   │   ├── App.tsx                # Root component
│   │   ├── pages/                 # Admin UI pages
│   │   │   ├── Dashboard.tsx      # Metrics & storage stats
│   │   │   ├── QueryBrowser.tsx   # SQL query interface
│   │   │   ├── Subscriptions.tsx  # Active connections view
│   │   │   ├── Configuration.tsx  # Config management
│   │   │   └── Performance.tsx    # Server performance monitoring
│   │   ├── components/            # Reusable UI components
│   │   ├── services/              # API client (fetch/WebSocket)
│   │   ├── types/                 # TypeScript type definitions
│   │   └── utils/                 # Helper functions
│   │
│   ├── tests/
│   │   ├── unit/                  # Vitest component tests
│   │   └── e2e/                   # Playwright end-to-end tests
│   │
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   └── index.html
│
├── docs/                          # Documentation
│   ├── architecture/              # ADRs, design decisions
│   ├── api/                       # API documentation
│   └── operations/                # Deployment, monitoring guides
│
├── scripts/                       # Utility scripts
│   ├── setup-dev.sh               # Development environment setup
│   └── benchmark.sh               # Run performance benchmarks
│
├── .github/                       # CI/CD workflows
│   └── workflows/
│       ├── backend.yml            # Rust tests, benchmarks, clippy
│       └── frontend.yml           # React build, tests, lint
│
├── docker/
│   ├── Dockerfile.backend         # Rust server container
│   ├── Dockerfile.frontend        # React UI container
│   └── docker-compose.yml         # Local development stack
│
└── README.md
```

**Structure Decision**: Web application structure (Option 2) selected due to separate backend (Rust/Actix) and frontend (React/Vite) components. Backend uses Rust workspace with multiple crates for clean separation of concerns (core storage, query engine, API layer, server binary). Frontend is standard Vite React app with TypeScript. This structure supports independent development, testing, and deployment of each component.

## Complexity Tracking

*Fill ONLY if Constitution Check has violations that must be justified*

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| AEAD encryption deferred | Phase 1 focuses on core storage+streaming; encryption adds significant complexity (key management, rotation, performance impact) | Implementing encryption upfront would delay MVP; transport security (TLS) + JWT + tenant isolation provide baseline security; encryption can be added in Phase 2 without breaking changes to Parquet schema (metadata column can store encrypted content) |
| Admin UI adds React complexity | Production operations require visibility into system state (active subscriptions, storage metrics, performance monitoring); command-line tools insufficient for non-technical operators | Pure CLI approach excludes operators unfamiliar with terminal; separate monitoring tools (Grafana/Prometheus) add more infrastructure complexity than embedding lightweight React UI |
