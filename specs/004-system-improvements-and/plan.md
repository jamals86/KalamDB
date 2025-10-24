# Implementation Plan: System Improvements and Performance Optimization

**Branch**: `004-system-improvements-and-performance` | **Date**: 2025-10-24 | **Spec**: `specs/004-system-improvements-and/spec.md`
**Input**: Feature specification from `specs/004-system-improvements-and/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

This plan adds and tracks the newly confirmed requirements from 2025-10-24 covering SQL DDL semantics, strict integrity checks, API field naming, storage schema, and authorization model. Key deliverables:

- SQL DDL and integrity
  - Support `DEFAULT NOW()` for TIMESTAMP columns
  - Require PRIMARY KEY on all tables; allowed types BIGINT or STRING
  - Support AUTO_INCREMENT strategies: SNOWFLAKE and UUID(v7); STREAM tables must use auto-inc PK
  - Enforce NOT NULL on INSERT/UPDATE
  - Ensure `SELECT *` column order matches table creation order (engine-level)
- API and storage shape
  - Rename timing field to `took_ms` in API responses
  - Rename `system.storages.base_directory` to `uri` (filesystem paths and S3 URIs)
  - Block deleting a storage while referenced by tables (error with dependent count)
- DDL syntax cleanups
  - Remove `OWNER_ID` from `CREATE USER`
  - Remove `TABLE_TYPE shared`; prefer `CREATE USER/SHARED/STREAM TABLE` forms (using a shared parser implementation)
- Authorization model
  - Introduce roles enum: { user, service, dba, system } stored in `system.users.role`
  - Allow shared tables to declare `access` = { public | private | restricted }

Acceptance is via new integration tests in `backend/tests/integration/test_schema_integrity.rs` and minor updates to existing timing/storage tests.

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: Rust 1.75+ (stable toolchain)
**Primary Dependencies**: RocksDB (writes), Apache Arrow (zero-copy data), Apache Parquet (storage), DataFusion (SQL queries), tokio (async runtime), serde (serialization)
**Storage**: RocksDB for write path (<1ms), Parquet for analytics-ready compressed storage
**Testing**: cargo test, criterion.rs (benchmarks), property-based tests for data transformations
**Target Platform**: Linux server, embeddable library
**Project Type**: single (database server with library interface)
**Performance Goals**: Writes <1ms (p99 RocksDB path), SQL query performance via DataFusion
**Constraints**: Zero-copy efficiency via Arrow IPC, tenant isolation enforcement, JWT authentication required
**Scale/Scope**: Real-time chat/AI conversation storage, user-partitioned data, streaming + durable persistence

Additional notes for this update:
- SQL Functions: Implement unified function registry at `/backend/crates/kalamdb-core/src/sql/functions` with NOW(), SNOWFLAKE_ID(), UUID_V7(), ULID(), CURRENT_USER(); align with DataFusion ScalarUDF architecture; support in DEFAULT, SELECT, and WHERE contexts; extensible for custom functions
- API timing: Switch serializers/formatters to `took_ms`; update CLI formatter accordingly
- Storage: Replace `base_directory` uses with `uri` across schema, data access, and documentation; for S3 require `s3://...` format
- Authorization: Add roles enum to `system.users`; propagate to auth middleware; add `access` metadata handling for shared tables

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I: Simplicity First ✅ PASS
- SQL DDL semantics (DEFAULT NOW, PRIMARY KEY, AUTO_INCREMENT) follow standard database patterns
- Roles enum and access control use straightforward { user, service, dba, system } model
- Parser consolidation reduces duplication, improving maintainability
- No over-engineering; each abstraction serves concrete needs

### Principle II: Performance by Design ✅ PASS
- API response timing field (`took_ms`) enables performance monitoring
- Snowflake and UUIDv7 ID generators provide monotonic keys for efficient RocksDB ordering
- SELECT * column order preservation maintains engine-level performance guarantees
- NOT NULL enforcement prevents validation overhead on queries

### Principle III: Data Ownership ✅ PASS
- User table isolation maintained through storage_id references and per-user partitioning
- Roles and access control enable fine-grained data sovereignty
- Storage URI abstraction supports multi-cloud and on-premise deployments
- Blocking storage deletion when in-use protects against data loss

### Principle IV: Zero-Copy Efficiency ✅ PASS
- Arrow/Parquet formats continue to minimize deserialization
- Column order preservation avoids runtime reordering overhead
- Storage URI abstraction doesn't add intermediate data copies

### Principle V: Open & Extensible ✅ PASS
- kalamdb-link library provides embeddable client SDK
- Roles enum is extensible for future authorization models
- Storage URI supports filesystem and S3, extensible to Azure/GCS
- DDL parser consolidation creates clean extension points

### Principle VI: Transparency ✅ PASS
- `took_ms` field provides explicit query timing
- Role-based access enables audit trails
- Storage metadata (uri, credentials) visible in system.storages

### Principle VII: Secure by Default ✅ PASS
- Roles enum (user, service, dba, system) enforces authorization boundaries
- Shared table `access` attribute (public/private/restricted) controls visibility
- Credentials in system.storages enables secure cloud storage authentication
- JWT authentication continues to protect endpoints

### Principle VIII: Self-Documenting Code ✅ PASS

**Documentation Requirements**:
- [x] Module-level rustdoc comments planned for:
  - ID generators (Snowflake, UUIDv7)
  - Roles and access control enforcement
  - Storage URI abstraction layer
- [x] Public API documentation strategy defined:
  - DDL statement structs with column default examples
  - Role and access enums with permission matrices
  - Storage models with URI format specifications
- [x] Inline comment strategy identified:
  - DEFAULT NOW() evaluation timing in INSERT path
  - Snowflake/UUIDv7 monotonicity guarantees
  - Column order preservation in projection planning
  - Role-based authorization checks in middleware
- [x] Architecture Decision Records planned:
  - ADR-013: DEFAULT ID generation functions (SNOWFLAKE_ID, UUID_V7, ULID) comparison and use cases
  - ADR-014: Roles and access control model
  - ADR-015: Storage URI unification (filesystem + S3)
- [x] Code examples planned:
  - CREATE TABLE with DEFAULT NOW() and DEFAULT SNOWFLAKE_ID()
  - CREATE TABLE with DEFAULT UUID_V7() and DEFAULT ULID()
  - DEFAULT functions on non-PK columns
  - CREATE USER with role assignment
  - CREATE SHARED TABLE with access control
  - CREATE STORAGE with S3 URI and credentials

**Constitution Check Result**: ✅ ALL GATES PASS - Ready for Phase 0 research

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
backend/
├── crates/
│   ├── kalamdb-commons/      # Shared types: UserId, NamespaceId, TableName, StorageId
│   │   └── src/
│   │       ├── models.rs     # Role enum, type-safe wrappers
│   │       ├── constants.rs  # System table names, CF constants
│   │       └── lib.rs
│   ├── kalamdb-sql/          # SQL parsing and DDL
│   │   └── src/
│   │       ├── ddl/          # DDL statement types
│   │       │   ├── create_table.rs  # DEFAULT functions support
│   │       │   └── ...
│   │       ├── keywords.rs   # Centralized SQL keywords enum
│   │       └── lib.rs
│   ├── kalamdb-core/         # Execution engine
│   │   └── src/
│   │       ├── sql/
│   │       │   └── functions/       # NEW: Unified SQL function registry
│   │       │       ├── mod.rs       # Function registry, DataFusion integration
│   │       │       ├── snowflake_id.rs   # SNOWFLAKE_ID()
│   │       │       ├── uuid_v7.rs   # UUID_V7()
│   │       │       ├── ulid.rs      # ULID()
│   │       │       ├── now.rs       # NOW()
│   │       │       ├── current_timestamp.rs  # CURRENT_TIMESTAMP()
│   │       │       └── current_user.rs       # CURRENT_USER()
│   │       ├── system_tables/
│   │       │   ├── users.rs         # Role column
│   │       │   ├── storages.rs      # URI column, credentials
│   │       │   └── tables.rs        # Access metadata for shared
│   │       ├── auth/                # NEW: Role-based authorization
│   │       └── lib.rs
│   ├── kalamdb-store/        # Storage abstraction
│   │   └── src/
│   │       ├── storage_trait.rs
│   │       ├── rocksdb_impl.rs
│   │       ├── s3_storage.rs        # URI + credentials support
│   │       └── lib.rs
│   ├── kalamdb-api/          # REST API
│   │   └── src/
│   │       ├── handlers/
│   │       │   └── sql_handler.rs   # took_ms response field
│   │       ├── middleware/
│   │       │   └── auth.rs          # Role enforcement
│   │       └── lib.rs
│   ├── kalamdb-server/       # Server binary
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── routes.rs
│   │       ├── middleware.rs
│   │       └── lifecycle.rs
│   └── kalamdb-live/         # WebSocket subscriptions
│       └── src/lib.rs
├── tests/
│   └── integration/
│       ├── test_schema_integrity.rs  # NEW: FR-DB-001 to FR-DB-014
│       ├── test_api_versioning.rs
│       └── ...
└── Cargo.toml

cli/
├── kalam-link/               # Client SDK library
│   └── src/
│       ├── client.rs         # took_ms response parsing
│       └── lib.rs
├── kalam-cli/                # Interactive CLI
│   └── src/
│       ├── formatter.rs      # took_ms display
│       └── main.rs
└── Cargo.toml

docs/
├── architecture/
│   └── adrs/
│       ├── ADR-013-default-id-functions.md      # NEW
│       ├── ADR-014-roles-and-access.md          # NEW
│       └── ADR-015-storage-uri-unification.md   # NEW
└── examples/
    └── schema_examples.md                        # NEW: DEFAULT NOW, AUTO_INCREMENT
```

**Structure Decision**: Backend monorepo with modular crates for database engine, CLI workspace for client tooling. This aligns with Rust best practices and enables independent versioning of client SDK (kalam-link) vs server components. New additions for this phase:
- ID generators module for Snowflake and UUIDv7
- Role-based authorization in kalamdb-core/auth
- Storage URI abstraction enhancements
- Schema integrity integration tests

## Complexity Tracking

*This section documents any deviations from constitution principles that require justification.*

**Status**: ✅ NO VIOLATIONS

All requirements align with constitution principles:
- Simplicity First: Standard SQL patterns, minimal abstractions
- Performance by Design: took_ms monitoring, efficient ID generation
- Data Ownership: Per-user isolation, role-based access
- Zero-Copy Efficiency: No new data copy layers
- Open & Extensible: Clean SDK boundaries, extensible enums
- Transparency: Timing metrics, role visibility
- Secure by Default: Multi-tier authorization model
- Self-Documenting: Comprehensive doc plan in place

## Phase 0: Research (COMPLETE - outputs in research.md)

**Prerequisites**: Constitution Check passed ✅

The following technical decisions were researched and documented in `research.md`:

### R1: DEFAULT NOW() Implementation
- **Decision**: Server-side evaluation on INSERT when column omitted
- **Rationale**: Matches PostgreSQL/MySQL semantics; avoids clock skew
- **Alternatives**: Client-side (rejected: unreliable clocks), trigger-based (rejected: complexity)

### R2: SQL Function Architecture
- **Decision**: Implement unified SQL function registry at `/backend/crates/kalamdb-core/src/sql/functions` with NOW(), SNOWFLAKE_ID(), UUID_V7(), ULID(), CURRENT_USER(); align with DataFusion ScalarUDF patterns; usable in DEFAULT, SELECT, WHERE contexts
- **Rationale**: Treating ID generators as SQL functions (not special-case generators) enables reuse across query contexts, reduces code duplication, aligns with standard SQL semantics, and provides clean extension points for custom functions and future scripting
- **Alternatives**: Separate id_generators module (rejected: duplication, not usable in queries), DataFusion built-ins only (rejected: custom ID strategies needed), macro-based codegen (deferred: premature optimization)

### R3: Roles and Access Control Model
- **Decision**: Four-tier roles { user, service, dba, system } + per-table access { public, private, restricted }
- **Rationale**: Covers common authorization patterns; aligns with PostgreSQL role model
- **Alternatives**: RBAC with permissions matrix (rejected: over-engineering for current needs)

### R4: Storage URI Unification
- **Decision**: Single `uri` column accepting filesystem paths and S3 URIs (s3://...)
- **Rationale**: Simplifies schema; URI format is self-describing
- **Alternatives**: Separate path + type columns (rejected: redundant with type enum)

### R5: NOT NULL Enforcement Timing
- **Decision**: Validation at INSERT/UPDATE before RocksDB write
- **Rationale**: Fail-fast; prevents partial writes
- **Alternatives**: Post-write validation (rejected: dirty reads possible)

### R6: Column Order Preservation
- **Decision**: DataFusion projection planning must preserve creation order
- **Rationale**: Matches PostgreSQL/MySQL behavior; avoids user surprise
- **Alternatives**: Alphabetical (rejected: breaks compatibility), hash-based (rejected: non-deterministic)

**Research Artifacts**: See `specs/004-system-improvements-and/research.md` for full decision logs

## Phase 1: Design (COMPLETE - outputs in data-model.md, contracts/, quickstart.md)

**Prerequisites**: Research complete ✅

### Data Model Extensions (data-model.md)

**New Entities**:
1. **Role Enum**: `{ user, service, dba, system }`
2. **Access Enum**: `{ public, private, restricted }`
3. **DefaultValueFunction Enum**: `{ Now, SnowflakeId, UuidV7, Ulid }` - server-side evaluation on INSERT

**Schema Changes**:
1. **system.users**: Add `role` column (Role enum, NOT NULL, default='user')
2. **system.storages**: Rename `base_directory` → `uri`; keep `credentials` (TEXT, nullable)
3. **system.tables** (shared tables): Add `access` column (Access enum, nullable, default='public')
4. **system.columns**: Add `default_expression` column (TEXT, nullable) to store DEFAULT function calls (NOW(), SNOWFLAKE_ID(), UUID_V7(), ULID(), CURRENT_USER())
5. **User/Shared/Stream Tables**: Enforce PRIMARY KEY requirement; support DEFAULT SQL functions on ANY column

**SQL Function Registry** (`/backend/crates/kalamdb-core/src/sql/functions`):
- **ID Generation Functions**: SNOWFLAKE_ID() → BIGINT, UUID_V7() → STRING, ULID() → STRING
- **Temporal Functions**: NOW() → TIMESTAMP, CURRENT_TIMESTAMP() → TIMESTAMP
- **Context Functions**: CURRENT_USER() → STRING
- **Architecture**: DataFusion ScalarUDF patterns, usable in DEFAULT, SELECT, WHERE contexts
- **Extensibility**: Clean extension points for custom functions and future scripting support

**Validation Rules**:
- PRIMARY KEY: MUST be BIGINT or STRING (TEXT/VARCHAR)
- NOT NULL: Enforced on INSERT and UPDATE
- DEFAULT NOW(): Evaluated server-side on INSERT when column omitted (TIMESTAMP columns only)
- DEFAULT SNOWFLAKE_ID(): Evaluated server-side, returns BIGINT (64-bit time-ordered)
- DEFAULT UUID_V7(): Evaluated server-side, returns STRING (128-bit RFC 9562)
- DEFAULT ULID(): Evaluated server-side, returns STRING (26-char time-sortable, URL-safe)
- DEFAULT CURRENT_USER(): Evaluated server-side, returns STRING (authenticated username)
- Storage deletion: Blocked if referenced by any table

### API Contracts (contracts/)

**Modified Endpoints**:
1. `POST /v1/api/sql`:
   - Response: Replace `execution_time_ms` → `took_ms`
   - DDL: Support DEFAULT function calls in CREATE TABLE statements
2. `GET /v1/api/sql` (system.storages query):
   - Column: `uri` (was `base_directory`)

**New DDL Syntax** (contracts/ddl-examples.sql):
```sql
-- Numeric ID (default - Snowflake)
CREATE USER TABLE app.files (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  uploaded_at TIMESTAMP DEFAULT NOW(),
  filename TEXT NOT NULL
);

-- Time-sortable UUID
CREATE USER TABLE events (
  event_id STRING PRIMARY KEY DEFAULT UUID_V7(),
  data JSONB,
  created_at TIMESTAMP DEFAULT NOW()
);

-- URL-safe sortable string
CREATE SHARED TABLE requests (
  request_id STRING PRIMARY KEY DEFAULT ULID(),
  endpoint TEXT NOT NULL,
  response_time_ms INT
) ACCESS public;

-- DEFAULT functions on non-PK columns
CREATE USER TABLE audit_log (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  correlation_id STRING DEFAULT ULID(),  -- Non-PK column with DEFAULT
  action TEXT NOT NULL,
  timestamp TIMESTAMP DEFAULT NOW(),
  created_by STRING DEFAULT CURRENT_USER()  -- Context function
);

-- Functions in SELECT queries
SELECT 
  NOW() as current_time,
  SNOWFLAKE_ID() as new_id,
  UUID_V7() as new_uuid,
  ULID() as new_ulid,
  CURRENT_USER() as username
FROM system.users LIMIT 1;

-- Functions in WHERE clauses
SELECT * FROM app.files 
WHERE uploaded_at < NOW() - INTERVAL '7 days';

-- Role assignment
CREATE USER admin_user WITH ROLE dba;

-- Storage with URI
CREATE STORAGE s3_prod
  TYPE S3
  URI 's3://kalamdb-prod/data'
  CREDENTIALS '{"access_key": "...", "secret_key": "..."}';
```

### Quickstart Guide (quickstart.md)

Updated quickstart to demonstrate:
1. Creating tables with DEFAULT NOW() and AUTO_INCREMENT
2. Role-based user creation
3. Shared tables with access control
4. S3 storage configuration with URI

### Agent Context Update

**Action**: Run `.specify/scripts/bash/update-agent-context.sh copilot` to update `.github/copilot-instructions.md` with:
- New ID generation utilities (Snowflake, UUIDv7)
- Roles and access control enums
- Storage URI format
- Schema integrity requirements

## Phase 2: Planning (Next Step - tasks.md generation)

**Prerequisites**: Design complete, agent context updated

**Next Command**: `/speckit.tasks` to generate implementation tasks aligned with:
- FR-DB-001 to FR-DB-014 (schema and integrity requirements)
- Integration tests in test_schema_integrity.rs
- Documentation tasks (ADRs, rustdoc, examples)

**Task Categories**:
1. ID Generators: SNOWFLAKE_ID(), UUID_V7(), and ULID() implementations
2. DDL Parser: DEFAULT NOW(), DEFAULT SNOWFLAKE_ID(), DEFAULT UUID_V7(), DEFAULT ULID() syntax
3. Schema Evolution: Migrate base_directory → uri, add role/access columns, add default_expression to system.columns
4. Validation: NOT NULL enforcement, PRIMARY KEY requirement, DEFAULT function type validation
5. Authorization: Role-based middleware, access control checks
6. API: Replace execution_time_ms → took_ms
7. Storage: URI abstraction for filesystem + S3
8. Testing: 11+ integration tests for FR-DB-001 to FR-DB-014 (including DEFAULT functions on non-PK columns)
9. Documentation: 3 ADRs, rustdoc updates, SQL examples

## Deliverables Summary

**Specification**: ✅ Updated with Session 2025-10-24 clarifications and FR-DB-001 to FR-DB-014
**Plan**: ✅ This file - constitution check, research decisions, design artifacts
**Research**: ✅ Decision logs for 6 technical choices
**Data Model**: ✅ Schema changes, validation rules, enums
**Contracts**: ✅ DDL examples, API response changes
**Quickstart**: ✅ Tutorial updates for new features
**Agent Context**: ⏳ Pending - run update script after Phase 1 approval

**Next Step**: Generate tasks.md via `/speckit.tasks` command
