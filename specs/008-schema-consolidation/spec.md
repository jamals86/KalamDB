# Feature Specification: Schema Consolidation & Unified Data Type System

**Feature Branch**: `008-schema-consolidation`  
**Created**: November 1, 2025  
**Updated**: November 3, 2025 (Added User Story 8 - AppContext Consolidation)  
**Status**: Draft  
**Input**: User description: "Schema consolidation and unified data type system with comprehensive test fixing for Alpha release readiness"

## 🚨 Critical Addition: AppContext Architectural Refactoring

**User Story 8 (P0)** addresses a fundamental architectural issue discovered during Phase 10 analysis:

**Problem**: Multiple Arc-wrapped instances of the same resources (KalamSql, stores, managers) are created and passed through layers, causing:
- ~70% wasted memory (3× UserTableStore, 3× SharedTableStore, 2× KalamSql per request)
- Constructor pollution (10+ parameter constructors, 8+ builder methods)
- Test complexity (50+ lines of boilerplate per test)
- Maintenance burden (adding a dependency requires updating 5+ files)

**Solution**: AppContext singleton pattern
- Single source of truth for all shared resources
- Zero-parameter service constructors (stateless services)
- 2-parameter SqlExecutor constructor (down from 10+ with 8 builders)
- 2-line test setup (down from 50+ lines)
- ~800 net lines of code removed

**Impact**: This is a **P0 critical refactoring** that will save significant development effort in the future. While it requires touching ~40 files, the long-term benefits far outweigh the short-term cost. All existing tests will pass after refactoring - behavior is identical, only the wiring changes.

**Timeline**: 4 days (well-defined phases with clear deliverables)

See **User Story 8** below for complete implementation plan.

## Clarifications

### Session 2025-11-01

- Q: What level of observability (logging, metrics, tracing) should the schema consolidation system implement for Alpha release? → A: Minimal observability - Basic error logging only (log errors to stderr, no metrics or tracing)
- Q: What should the system do when the schema cache encounters an error (eviction failure, corruption, full capacity despite eviction)? → A: Fallback to EntityStore - Bypass cache for failed operations, serve from EntityStore directly
- Q: What error message format should schema operations use when returning errors to users/APIs? → A: Simple string messages - Return plain error strings like "Invalid type: FOO", ensuring messages are straightforward, helpful, and avoid duplication (no "server error: query error: ..." chains)
- Q: What access control should apply to schema operations (viewing and modifying table schemas)? → A: All authenticated users read-only - All users can view schemas, only DBA/system roles can modify (CREATE/ALTER/DROP)
- Q: How should the system enforce the EMBEDDING dimension maximum of 8192? → A: Hard limit with validation error - Reject CREATE TABLE if EMBEDDING dimension > 8192 with clear error

### Session 2025-11-03

- Q: For Phase 10 cache consolidation, when table providers query/insert data, should TableId be recreated on each operation or stored with the provider? → A: Store at registration - TableId created once at table registration time and stored as Arc<TableId> in table provider struct (UserTableProvider, SharedTableProvider, etc.), eliminating repeated allocation. Cache lookups use Arc<TableId> reference (zero-copy), not recreating from (namespace, table_name) tuple on every query.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Single Source of Truth for Table Schemas (Priority: P1)

Developers need a unified, consistent way to define and query table schemas across the entire codebase. Currently, schema information is scattered across multiple files and models, leading to duplication, inconsistencies, and maintenance burden. The system must consolidate all schema-related models into a single, cacheable location that serves as the authoritative source for table structure definitions.

**Why this priority**: This is foundational infrastructure that blocks all other improvements. Without schema consolidation, any schema-related bug fix or feature enhancement requires changes in multiple places, increasing risk of inconsistencies. This directly impacts developer productivity and system reliability.

**Independent Test**: Can be fully tested by creating a table, querying its schema from multiple code paths (CREATE TABLE parser, DESCRIBE TABLE command, information_schema queries), and verifying all paths return identical schema definitions. Delivers immediate value by eliminating schema-related bugs.

**Acceptance Scenarios**:

1. **Given** a developer creates a new table with columns, **When** they query the schema via DESCRIBE TABLE, information_schema.columns, and internal APIs, **Then** all sources return identical column definitions (names, types, nullability, defaults, ordinal positions)

2. **Given** system tables (users, jobs, namespaces) are defined, **When** developers query their schemas, **Then** schemas come from the consolidated models in \`kalamdb-commons/src/models/schemas/\` directory

3. **Given** multiple concurrent reads of table schema, **When** schema is cached in memory, **Then** repeated schema lookups complete in under 100 microseconds without re-parsing

4. **Given** a table schema is modified via ALTER TABLE, **When** schema version increments, **Then** historical schema versions are preserved in TableDefinition.schema_history array

5. **Given** column definitions include metadata (is_nullable, is_primary_key, is_partition_key, default_value, ordinal_position), **When** querying column details, **Then** all metadata is available through ColumnDefinition model

6. **Given** a table is created with columns (id, name, email, created_at), **When** executing SELECT * FROM table, **Then** columns are returned in ordinal_position order (1: id, 2: name, 3: email, 4: created_at)

7. **Given** TableDefinition is stored via EntityStore<TableId, TableDefinition>, **When** querying table schema, **Then** schema is retrieved from EntityStore using consistent API with other system tables

8. **Given** a table schema is modified via ALTER TABLE ADD COLUMN, **When** new SchemaVersion is created, **Then** schema history is persisted as part of TableDefinition in EntityStore (not separate storage)

---

### User Story 2 - Unified Data Type System with Arrow/DataFusion Conversion (Priority: P1)

Developers need a single, canonical data type system that eliminates type conversion errors and provides fast, cached translations to Arrow and DataFusion types. Currently, multiple type representations exist across the codebase, causing conversion bugs and performance overhead. The system must introduce KalamDataType enum as the single source of truth with efficient, cached conversions.

**Why this priority**: Type system inconsistencies cause data corruption bugs and query execution errors. This is a critical reliability issue that must be fixed before Alpha release. Unified types also enable performance optimizations through caching.

**Independent Test**: Can be fully tested by creating tables with all supported data types, executing queries that use type conversions (Arrow RecordBatch creation, DataFusion query planning), and verifying no type-related errors occur. Delivers immediate value by eliminating type conversion bugs.

**Acceptance Scenarios**:

1. **Given** a table column is defined with KalamDataType::TEXT, **When** converting to Arrow DataType, **Then** conversion completes in under 10 microseconds via cached lookup

2. **Given** all supported KalamDataTypes (BOOLEAN, INT, BIGINT, DOUBLE, FLOAT, TEXT, TIMESTAMP, DATE, DATETIME, TIME, JSON, BYTES, EMBEDDING), **When** converting to/from Arrow types, **Then** bidirectional conversions are lossless and deterministic

3. **Given** a table column is defined with KalamDataType::EMBEDDING(1536), **When** converting to Arrow DataType, **Then** conversion produces FixedSizeList<Float32> with dimension 1536 for vector embeddings

4. **Given** a query uses TIMESTAMP columns, **When** DataFusion execution engine accesses column types, **Then** KalamDataType provides Arrow timestamp type with microsecond precision

5. **Given** ColumnDefinition uses KalamDataType for data_type field, **When** serializing table schema, **Then** type information includes wire format tag (0x01-0x0D) and dimension parameters for parameterized types

6. **Given** legacy code references old type representations, **When** refactored to use KalamDataType, **Then** all codebase type conversions go through centralized conversion functions

---

### User Story 3 - Comprehensive Test Suite Passing for Alpha Release (Priority: P1)

The database must have all tests passing across backend, CLI, and link (WASM) components to ensure production readiness. Currently, failing tests indicate incomplete functionality or bugs that block Alpha release. The system must fix all test failures by implementing missing features, correcting bugs, and updating tests to match current implementation.

**Why this priority**: Passing tests are the minimum quality bar for Alpha release. Shipping with failing tests risks data loss, security vulnerabilities, and customer trust damage. This is a release blocker.

**Independent Test**: Can be fully tested by running \`cargo test\` in backend/, cli/, and link/ directories and verifying 100% pass rate. Delivers immediate value by proving system reliability and catching regressions.

**Acceptance Scenarios**:

1. **Given** backend test suite runs, **When** executing \`cargo test\` in backend/, **Then** all integration and unit tests pass with 0 failures

2. **Given** CLI test suite runs, **When** executing tests in cli/, **Then** all CLI command tests (auth, queries, subscriptions) pass successfully

3. **Given** link/WASM SDK test suite runs, **When** executing TypeScript SDK tests, **Then** all 14+ tests pass including connection, query, and subscription scenarios

4. **Given** a test failure is identified, **When** investigating root cause, **Then** fix is applied to implementation code (not test mocking) to restore correct behavior

5. **Given** schema consolidation is complete, **When** running tests that query schemas, **Then** tests pass using new unified schema models without legacy model references

---

### User Story 4 - Performance-Optimized Schema Caching (Priority: P2)

The system should cache frequently accessed schema information to minimize repeated parsing and deserialization overhead. Developers querying table schemas multiple times should experience sub-millisecond response times through intelligent caching of TableDefinition and ColumnDefinition objects.

**Why this priority**: While not blocking Alpha, caching significantly improves query performance and reduces CPU usage. This is valuable for production workloads with high query rates.

**Independent Test**: Can be fully tested by running a benchmark that queries the same table schema 10,000 times and verifying cache hit rate above 99% with average lookup time under 100 microseconds. Delivers measurable performance improvement.

**Acceptance Scenarios**:

1. **Given** a table schema is loaded into cache, **When** the same schema is requested again, **Then** response comes from cache without database roundtrip

2. **Given** schema cache is implemented with LRU eviction, **When** cache size limit is reached, **Then** least recently used schemas are evicted while maintaining correctness

3. **Given** table schema is modified via ALTER TABLE, **When** cached schema exists, **Then** cache is invalidated and next request loads fresh schema version

4. **Given** high concurrent read load on schema cache, **When** using lock-free data structures (DashMap), **Then** cache access does not become a bottleneck

---

### User Story 5 - Critical P0 Datatype Expansion (Priority: P0)

The type system must support essential datatypes required for modern database applications: UUID for distributed identifiers, DECIMAL for financial precision, and SMALLINT for integer storage efficiency. Currently, the absence of these types forces workarounds using TEXT (for UUID) and DOUBLE (for money), leading to data integrity issues and performance degradation.

**Why this priority**: These are **critical missing types** that block common use cases:
- **UUID**: Almost every modern app needs distributed unique identifiers (primary keys, API tokens, session IDs)
- **DECIMAL**: Financial applications CANNOT use FLOAT/DOUBLE for money due to rounding errors - this is a data corruption bug
- **SMALLINT**: Efficient storage for enum values, status codes, small counters (uses 2 bytes vs 4 bytes for INT)

**Independent Test**: Can be fully tested by creating tables with UUID, DECIMAL, and SMALLINT columns, inserting values, flushing to Parquet, reading back, and verifying exact value preservation including UUID format, decimal precision, and smallint range validation. Delivers immediate value by enabling financial and distributed system use cases.

**Acceptance Scenarios**:

1. **Given** a table column is defined with KalamDataType::Uuid, **When** converting to Arrow DataType, **Then** conversion produces FixedSizeBinary(16) for 128-bit UUID storage

2. **Given** a UUID value "550e8400-e29b-41d4-a716-446655440000" is inserted, **When** flushing to Parquet and reading back, **Then** UUID is preserved exactly in RFC 4122 format

3. **Given** a table column is defined with KalamDataType::Decimal { precision: 10, scale: 2 }, **When** storing monetary value $1234.56, **Then** value is stored as Decimal128 with exact precision (no floating-point rounding errors)

4. **Given** a DECIMAL(10, 2) column stores financial calculations, **When** querying sum, average, or other aggregations, **Then** results maintain exact decimal precision throughout computation

5. **Given** a table column is defined with KalamDataType::SmallInt, **When** converting to Arrow DataType, **Then** conversion produces Int16 for 16-bit signed integer storage (-32,768 to 32,767 range)

6. **Given** a SMALLINT column stores enum values (0-255), **When** inserting values outside valid range (e.g., 40000), **Then** system rejects with clear error "Value 40000 out of range for SMALLINT (-32,768 to 32,767)"

7. **Given** wire format tags for new types (UUID=0x0E, DECIMAL=0x0F, SMALLINT=0x10), **When** serializing/deserializing table schemas, **Then** new types roundtrip correctly through Parquet metadata

8. **Given** existing Parquet files with old type tags (0x01-0x0D), **When** system reads old files, **Then** backward compatibility is maintained and old types decode correctly

9. **Given** DECIMAL precision validation (1-38), **When** CREATE TABLE specifies DECIMAL(0, 0) or DECIMAL(50, 2), **Then** system rejects with error "DECIMAL precision must be between 1 and 38"

10. **Given** DECIMAL scale validation (0 ≤ scale ≤ precision), **When** CREATE TABLE specifies DECIMAL(10, 11), **Then** system rejects with error "DECIMAL scale (11) cannot exceed precision (10)"

11. **Given** DateTime column stores "2025-01-01T12:00:00+02:00", **When** converting to UTC for storage, **Then** value is stored as "2025-01-01T10:00:00Z" (UTC normalization) and original timezone offset is LOST (applications must store timezone separately if needed)

---

### User Story 6 - CLI Smoke Tests Group (Priority: P0)

Operators and developers need a lightning-fast way to verify a running server provides basic functionality. Provide a dedicated CLI smoke test group named `smoke` (referenced as "smoke-test" in tooling where needed) that covers namespaces, shared tables, subscriptions, CRUD, system tables, users, stream tables, and per-user isolation for user tables.

Why this priority: Early detection of regressions saves time. A minimal, deterministic suite reduces friction for contributors and CI.

Independent Test: Run only the smoke group against a running server (local or remote). Complete in under 1 minute, with clear pass/fail output.

Acceptance Scenarios:

Test 1: User table with subscription lifecycle
1. Given a clean server, when a namespace is created, then it succeeds
2. When a user table is created, then it succeeds
3. When rows are inserted into the user table, then inserts succeed
4. When subscribing to this user table, then a subscription opens successfully
5. Then new insert/update/delete operations emit corresponding events to the subscription
6. When selecting from the table, then results reflect the applied changes
7. When flushing the user table, then a corresponding job is visible in `system.jobs`

Test 2: Shared table CRUD
1. Given a new namespace and shared table, when rows are inserted, then a select returns all rows
2. When a row is deleted and another updated, then a select returns rows reflecting these changes
3. When the table is dropped, then selecting or listing shows it was deleted

Test 3: System tables and user lifecycle
1. When selecting from each system table (`system.jobs`, `system.users`, `system.live_queries`, `system.tables`, `system.namespaces`), then at least one row is returned where applicable
2. When a new user is created, then selecting from `system.users` shows the user
3. When the user is deleted, then selection shows it removed or soft-deleted per policy
4. When issuing a FLUSH ALL TABLES command, then a job is recorded in `system.jobs`

Test 4: Stream table subscription
1. Given stream tables are enabled, when a namespace and stream table are created, then subscription to the stream table opens
2. When rows are inserted into the stream table, then the subscription receives those rows

Test 5: User table per-user isolation (RLS)
1. Given a root user creates a namespace and a user table, when root inserts several rows, then those rows exist under root’s ownership
2. When a new regular (non-admin) user is created and the CLI logs in as this user, then authentication succeeds
3. When the regular user inserts multiple rows, updates one row, deletes one row, and then selects all from the user table, then the results include only this user’s rows and changes (root’s rows are not visible)
4. And the regular user is allowed to insert into the user table (no permission errors)

Implementation notes:
- Group name: `smoke`; test modules/files prefixed with `smoke_test_` under `cli/tests/smoke/`
- Runner: CLI integration runner can filter by group `smoke`; configurable server URL via env (defaults to local)
- Deterministic: unique namespace per run, bounded timeouts for subscriptions, cleanup on failure/success
- Subscriptions are supported for user and stream tables; shared tables do not support subscriptions

---

### User Story 7 - Cache Consolidation for Memory Efficiency (Priority: P1)

**Status**: Identified during Phase 9 completion (2025-11-03)

Developers need a single unified cache for all table-related data instead of maintaining two separate caches that store overlapping information. Currently, TableCache (path resolution) and SchemaCache (schema queries) duplicate ~50% of data, require synchronized updates, and risk consistency bugs. The system must consolidate both caches into a single SchemaCache with CachedTableData that contains all table metadata, schema definitions, and storage path templates.

**Why this priority**: This is critical technical debt that wastes memory, increases code complexity, and creates maintenance burden. Eliminating redundant caching frees memory for actual data, simplifies code (~1,200 lines deleted), and prevents cache synchronization bugs. This directly improves system reliability and resource efficiency.

**Independent Test**: Can be fully tested by running CREATE/ALTER/DROP TABLE operations and verifying single cache serves all use cases (DESCRIBE TABLE, FLUSH TABLE, schema queries) with >99% hit rate and <100μs latency. Memory profiling should show ~50% reduction in cache memory usage compared to dual-cache baseline.

**Acceptance Scenarios**:

1. **Given** a table is created with full schema definition, **When** querying via DESCRIBE TABLE and FLUSH TABLE, **Then** both operations use the same SchemaCache instance (not separate TableCache and SchemaCache)

2. **Given** CachedTableData contains table_id (with namespace + table_name), table_type, storage_id, storage_path_template, and schema (TableDefinition), **When** accessing any table metadata, **Then** all data is retrieved from single DashMap lookup

3. **Given** TableId already contains NamespaceId and TableName internally, **When** table provider is registered with DataFusion, **Then** TableId is created once and stored as Arc<TableId> in table provider struct (not recreated on every query)

4. **Given** table provider stores Arc<TableId>, **When** querying cache for table metadata or storage path, **Then** cache lookup uses Arc<TableId>::clone() (cheap reference count increment, zero allocation)

5. **Given** storage path template is cached with partial resolution ({namespace}/{tableName} substituted, {userId}/{shard} remaining), **When** flush job resolves dynamic placeholders, **Then** template comes from CachedTableData.storage_path_template field (not separate storage_paths map)

6. **Given** ALTER TABLE modifies table schema, **When** invalidating cache, **Then** single invalidate(&table_id) call removes entry from SchemaCache (no need to invalidate both TableCache and SchemaCache)

7. **Given** memory profiling before and after consolidation, **When** measuring cache memory usage, **Then** unified SchemaCache uses ~50% less memory than TableCache + SchemaCache combined

8. **Given** old TableCache (catalog/table_cache.rs, 516 lines) and old SchemaCache (tables/system/schemas/schema_cache.rs, 443 lines) and TableMetadata (catalog/table_metadata.rs, 252 lines), **When** replaced with unified SchemaCache, **Then** ~1,200 lines of duplicate code deleted

9. **Given** concurrent access from multiple threads, **When** reading/writing to SchemaCache, **Then** DashMap provides lock-free concurrent access with no deadlocks

10. **Given** 10,000 queries to same table, **When** measuring TableId allocation count, **Then** only 1 TableId allocated at registration time (remaining 9,999 queries use Arc::clone with reference counting only)

**Architecture Change**:
```rust
// OLD: Two separate caches with TableId recreated on every lookup
TableCache: HashMap<(NamespaceId, TableName), TableMetadata> + storage_path_templates
SchemaCache: DashMap<TableId, Arc<TableDefinition>>
// Every query: TableId::new(namespace, table_name) → allocation

// NEW: Single unified cache with Arc<TableId> stored in providers
SchemaCache: DashMap<TableId, Arc<CachedTableData>>

struct UserTableProvider {
    table_id: Arc<TableId>,  // Created once at registration, reused for all queries
    // ... other fields
}

struct CachedTableData {
    table_id: TableId,                    // Contains (namespace, table_name)
    table_type: TableType,
    storage_id: Option<StorageId>,
    storage_path_template: String,        // Cached partial template
    schema: Arc<TableDefinition>,         // Full schema with columns
    // ... other metadata
}

// Query path (zero allocations after registration):
// 1. Table provider already has Arc<TableId>
// 2. cache.get(&*table_id) → O(1) DashMap lookup, no allocation
// 3. Return Arc<CachedTableData> → cheap Arc::clone()
```

**Benefits**:
- **Memory Efficiency**: ~50% reduction in cache memory usage
- **Code Simplicity**: ~1,200 lines of duplicate code deleted
- **Consistency**: Single source of truth eliminates sync bugs
- **Performance**: Single cache lookup instead of potentially two
- **Maintainability**: One cache implementation to test and evolve

---

### User Story 9 - Unified Job Management System (Priority: P1)

**Status**: Design proposal (2025-11-04)

Developers need a unified, consistent job management system with a single JobManager responsible for ALL background jobs (flush, cleanup, retention, stream eviction, compaction, backup, restore). Currently, job-related code is scattered across multiple files with inconsistent patterns, duplicate state tracking, and ad-hoc implementations. The system must consolidate all job types into a single coherent architecture with comprehensive lifecycle management, dedicated logging, and shortened job IDs with type-specific prefixes.

**Why this priority**: Job management is critical infrastructure that affects system reliability, observability, and maintenance. Current fragmentation causes:
- **Operational Complexity**: 7+ different job implementations with different patterns for starting/stopping/monitoring
- **Poor Observability**: Jobs log to main app.log with no dedicated job tracking, making debugging difficult
- **Inconsistent State**: Some jobs track in system.jobs, others in memory only, leading to lost job state on restart
- **Long Job IDs**: Current UUID-based job IDs (36 chars) waste storage and readability vs typed short IDs
- **Limited Status Tracking**: Only 4 statuses (Running, Completed, Failed, Cancelled) miss important states like Queued and Retrying

**Independent Test**: Can be fully tested by:
1. Creating jobs of each type (flush, cleanup, retention, etc.)
2. Verifying all jobs appear in system.jobs with correct status transitions
3. Checking jobs.log contains job-specific entries with job_id prefixes
4. Validating job ID format (e.g., "FL-abc123" for flush, "CL-def456" for cleanup)
5. Testing crash recovery by restarting server mid-job and verifying resume/fail behavior

**Current Problems**:

1. **Scattered Job Code**: Jobs split across multiple files with different patterns:
   - `backend/crates/kalamdb-core/src/jobs/` (7 files: executor.rs, job_manager.rs, tokio_job_manager.rs, job_cleanup.rs, retention.rs, stream_eviction.rs, stream_eviction_scheduler.rs, user_cleanup.rs)
   - `backend/crates/kalamdb-core/src/scheduler.rs` (flush scheduler logic)
   - Duplicate state tracking in FlushScheduler, StreamEvictionScheduler, JobExecutor

2. **Inconsistent Logging**: All jobs log to main app.log, no dedicated jobs.log, hard to filter job-specific events

3. **Long Job IDs**: Current format uses full UUIDs (e.g., "flush-messages-550e8400-e29b-41d4-a716-446655440000", 36+ chars), wastes storage, unreadable

4. **Limited Status Model**: Only 4 statuses (Running, Completed, Failed, Cancelled) miss:
   - **New**: Job created but not yet queued
   - **Queued**: Job waiting in queue (for rate limiting, backpressure)
   - **Retrying**: Job failed but will retry (exponential backoff)

5. **No Unified JobManager**: JobExecutor mixes executor logic with scheduling, no clear separation of concerns

**Proposed Architecture**:

```rust
// ============================================================================
// JobManager - Central Coordinator
// ============================================================================

/// Unified job manager responsible for ALL background jobs
///
/// Responsibilities:
/// - Job lifecycle: create → queue → execute → complete/fail/retry
/// - State persistence: all jobs tracked in system.jobs
/// - Logging: dedicated jobs.log with job_id prefix on every entry
/// - Crash recovery: resume incomplete jobs on startup
/// - Resource management: concurrency limits, backpressure, rate limiting
pub struct JobManager {
    /// Job queue (priority queue for different job types)
    job_queue: Arc<JobQueue>,
    
    /// Job executor pool (Tokio tasks or thread pool)
    executor_pool: Arc<ExecutorPool>,
    
    /// System.jobs table provider (persistent state)
    jobs_provider: Arc<JobsTableProvider>,
    
    /// Active jobs registry (in-memory tracking)
    active_jobs: Arc<DashMap<JobId, JobHandle>>,
    
    /// Job logger (dedicated jobs.log file)
    job_logger: Arc<JobLogger>,
    
    /// Configuration (max concurrent jobs, retry policy, etc.)
    config: JobManagerConfig,
}

// ============================================================================
// Enhanced JobStatus - 7 States
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub enum JobStatus {
    /// Job created but not yet queued (initial state)
    New,
    
    /// Job waiting in queue (backpressure, rate limiting)
    Queued,
    
    /// Job actively executing
    Running,
    
    /// Job completed successfully
    Completed,
    
    /// Job failed permanently (max retries exhausted)
    Failed,
    
    /// Job failed but will retry (exponential backoff)
    Retrying,
    
    /// Job cancelled by user/admin
    Cancelled,
}

// ============================================================================
// Enhanced JobType - Type-Specific Prefixes
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub enum JobType {
    Flush,          // Prefix: FL
    Compact,        // Prefix: CO
    Cleanup,        // Prefix: CL
    Backup,         // Prefix: BK
    Restore,        // Prefix: RS
    Retention,      // Prefix: RT
    StreamEviction, // Prefix: SE
    UserCleanup,    // Prefix: UC
}

impl JobType {
    /// Get short prefix for job ID generation (2 chars)
    pub fn prefix(&self) -> &'static str {
        match self {
            JobType::Flush => "FL",
            JobType::Compact => "CO",
            JobType::Cleanup => "CL",
            JobType::Backup => "BK",
            JobType::Restore => "RS",
            JobType::Retention => "RT",
            JobType::StreamEviction => "SE",
            JobType::UserCleanup => "UC",
        }
    }
}

// ============================================================================
// JobId - Short, Typed, Readable
// ============================================================================

/// Type-safe job ID with format: {PREFIX}-{SHORT_ID}
///
/// Examples:
/// - FL-abc123 (Flush job)
/// - CL-def456 (Cleanup job)
/// - SE-ghi789 (Stream eviction job)
///
/// Short ID format: 6 chars base62 (62^6 = 56 billion unique IDs)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct JobId(String);

impl JobId {
    /// Generate new job ID for given type
    /// Format: {PREFIX}-{base62(timestamp_ms + random_u16)}
    ///
    /// Examples:
    /// - FL-abc123 (Flush job at timestamp 1730000000000)
    /// - CL-def456 (Cleanup job at timestamp 1730000001000)
    pub fn generate(job_type: JobType) -> Self {
        let prefix = job_type.prefix();
        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let random = rand::random::<u16>() as u64;
        let combined = timestamp.wrapping_add(random);
        let short_id = base62_encode(combined, 6); // 6 chars
        JobId(format!("{}-{}", prefix, short_id))
    }
    
    /// Parse job type from ID prefix
    pub fn job_type(&self) -> Option<JobType> {
        let prefix = self.0.split('-').next()?;
        match prefix {
            "FL" => Some(JobType::Flush),
            "CO" => Some(JobType::Compact),
            "CL" => Some(JobType::Cleanup),
            "BK" => Some(JobType::Backup),
            "RS" => Some(JobType::Restore),
            "RT" => Some(JobType::Retention),
            "SE" => Some(JobType::StreamEviction),
            "UC" => Some(JobType::UserCleanup),
            _ => None,
        }
    }
}

// ============================================================================
// JobLogger - Dedicated Logging
// ============================================================================

/// Dedicated logger for jobs.log file
///
/// Log format: [{job_id}] {level} - {message}
/// Example: [FL-abc123] INFO - Flush started for table users.messages
pub struct JobLogger {
    log_file: Arc<RwLock<File>>,
}

impl JobLogger {
    /// Log job event with job_id prefix
    pub fn log(&self, job_id: &JobId, level: LogLevel, message: &str) {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let entry = format!("[{}] {} {} - {}\n", timestamp, job_id.as_str(), level, message);
        
        if let Ok(mut file) = self.log_file.write() {
            let _ = file.write_all(entry.as_bytes());
            let _ = file.flush();
        }
    }
    
    /// Log with structured data (JSON for complex events)
    pub fn log_structured(&self, job_id: &JobId, level: LogLevel, data: &serde_json::Value) {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let entry = format!("[{}] {} {} - {}\n", timestamp, job_id.as_str(), level, data);
        
        if let Ok(mut file) = self.log_file.write() {
            let _ = file.write_all(entry.as_bytes());
            let _ = file.flush();
        }
    }
}

// ============================================================================
// Enhanced Job Struct - Idempotency & Unified Messaging
// ============================================================================

/// Enhanced Job struct with idempotency, retry logic, and unified messaging
///
/// **Key Changes from Original**:
/// 
/// 1. **Idempotency Key**: Ensures each job runs once only
///    - Format: "flush:{namespace}:{table}:{YYYYMMDD}" for daily flush jobs
///    - Format: "cleanup:{type}:{YYYYMMDDTHH}" for hourly cleanup jobs
///    - Before creating a job, check system.jobs for existing job with same idempotency_key and active status
///    - Active statuses: New, Queued, Running, Retrying
///    - If active job exists, reject new job creation with error "Job already running: {existing_job_id}"
///
/// 2. **Unified Message Field**: Replaces `result` + `error_message`
///    - Single `message` field serves both success and error messages
///    - Completed job: message contains success info (e.g., "Flushed 1,234 rows to Parquet")
///    - Failed job: message contains error summary (e.g., "RocksDB read failed: IO error")
///    - Cleaner API, less confusion about which field to use
///
/// 3. **Exception Trace**: Separate field for detailed stack traces
///    - Old `trace` field renamed to `exception_trace` for clarity
///    - Contains full stack trace for failed jobs (multi-line format)
///    - Only populated on failure (None for successful jobs)
///    - Enables detailed debugging without cluttering message field
///
/// 4. **Retry Logic**: Built-in retry mechanism
///    - `retry_count`: Current retry attempt (0 = first attempt, 1+ = retries)
///    - `max_retries`: Maximum retry attempts (default: 3)
///    - Status transitions: Failed → Retrying → Running → Completed/Failed
///    - Exponential backoff: retry_delay = base_delay * 2^retry_count
///
/// 5. **Parameters Change**: JSON object instead of JSON array
///    - Old: `parameters: Option<String>` // JSON array like ["param1", "param2"]
///    - New: `parameters: Option<String>` // JSON object like {"key": "value", "count": 100}
///    - More flexible, self-documenting, easier to extend
///
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct Job {
    pub job_id: JobId,
    pub job_type: JobType,
    pub namespace_id: NamespaceId,
    pub table_name: Option<TableName>,
    pub status: JobStatus,
    
    /// **NEW**: Idempotency key to prevent duplicate job execution
    /// 
    /// Examples:
    /// - Daily flush: "flush:default:events:20251104"
    /// - Hourly cleanup: "cleanup:retention:20251104T14"
    /// - Compaction: "compact:default:events:20251104T14"
    ///
    /// Before creating a job, query system.jobs:
    /// ```sql
    /// SELECT job_id FROM system.jobs 
    /// WHERE idempotency_key = ? 
    ///   AND status IN ('New', 'Queued', 'Running', 'Retrying')
    /// ```
    /// If result exists, reject job creation.
    pub idempotency_key: Option<String>,
    
    /// **CHANGED**: JSON object (not array) for flexible parameters
    /// 
    /// Examples:
    /// - Flush: {"threshold_bytes": 1048576, "force": false}
    /// - Cleanup: {"retention_days": 30, "batch_size": 1000}
    /// - Retention: {"delete_before": "2025-10-01T00:00:00Z"}
    pub parameters: Option<String>,
    
    /// **NEW**: Unified message field for success OR error messages
    /// 
    /// Success example: "Flushed 1,234 rows (5.2 MB) to Parquet in 342ms"
    /// Error example: "RocksDB read failed: IO error at offset 12345"
    ///
    /// Replaces old `result` and `error_message` fields.
    pub message: Option<String>,
    
    /// **RENAMED**: Full exception trace for failed jobs (was `trace`)
    /// 
    /// Contains multi-line stack trace:
    /// ```
    /// Error: Failed to flush table
    ///   at FlushJob::execute (flush.rs:123)
    ///   at JobExecutor::run (executor.rs:456)
    ///   at tokio::runtime::task (task.rs:789)
    /// ```
    ///
    /// Only populated on failure. Success jobs have None.
    pub exception_trace: Option<String>,
    
    pub memory_used: Option<i64>,  // bytes
    pub cpu_used: Option<i64>,     // microseconds
    pub created_at: i64,           // Unix timestamp in milliseconds
    pub started_at: Option<i64>,   // Unix timestamp in milliseconds
    pub completed_at: Option<i64>, // Unix timestamp in milliseconds
    
    #[bincode(with_serde)]
    pub node_id: NodeId,
    
    /// **NEW**: Retry tracking
    pub retry_count: u32,      // Current retry attempt (0 = first try)
    pub max_retries: u32,      // Maximum retry attempts (default: 3)
}

impl Job {
    /// Create new job with New status (not Running)
    /// 
    /// **Changed**: Initial status is New (not Running) to allow proper queuing
    pub fn new(
        job_id: JobId,
        job_type: JobType,
        namespace_id: NamespaceId,
        node_id: NodeId,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            job_id,
            job_type,
            namespace_id,
            table_name: None,
            status: JobStatus::New,  // Changed from Running
            idempotency_key: None,
            parameters: None,
            message: None,
            exception_trace: None,
            memory_used: None,
            cpu_used: None,
            created_at: now,
            started_at: None,  // Set when job actually starts
            completed_at: None,
            node_id,
            retry_count: 0,
            max_retries: 3,  // Default retry limit
        }
    }
    
    /// Mark job as queued
    pub fn queue(mut self) -> Self {
        self.status = JobStatus::Queued;
        self
    }
    
    /// Mark job as running (transition from Queued)
    pub fn start(mut self) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Running;
        self.started_at = Some(now);
        self
    }
    
    /// Mark job as completed with success message
    pub fn complete(mut self, message: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Completed;
        self.completed_at = Some(now);
        if self.started_at.is_none() {
            self.started_at = Some(self.created_at);
        }
        self.message = message;
        self.exception_trace = None;  // Clear trace on success
        self
    }
    
    /// Mark job as failed with error message and optional exception trace
    pub fn fail(mut self, error_message: String, exception_trace: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Failed;
        self.completed_at = Some(now);
        if self.started_at.is_none() {
            self.started_at = Some(self.created_at);
        }
        self.message = Some(error_message);
        self.exception_trace = exception_trace;
        self
    }
    
    /// Mark job for retry (increment retry_count, set Retrying status)
    pub fn retry(mut self, error_message: String, exception_trace: Option<String>) -> Self {
        self.retry_count += 1;
        self.status = JobStatus::Retrying;
        self.message = Some(error_message);
        self.exception_trace = exception_trace;
        self
    }
    
    /// Check if job can be retried
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }
    
    /// Mark job as cancelled
    pub fn cancel(mut self) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(now);
        self
    }
    
    /// Set idempotency key
    pub fn with_idempotency_key(mut self, key: String) -> Self {
        self.idempotency_key = Some(key);
        self
    }
    
    /// Set table name
    pub fn with_table_name(mut self, table_name: TableName) -> Self {
        self.table_name = Some(table_name);
        self
    }
    
    /// Set parameters (JSON object)
    pub fn with_parameters(mut self, parameters: String) -> Self {
        self.parameters = Some(parameters);
        self
    }
    
    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }
    
    /// Set resource metrics
    pub fn with_metrics(mut self, memory_used: Option<i64>, cpu_used: Option<i64>) -> Self {
        self.memory_used = memory_used;
        self.cpu_used = cpu_used;
        self
    }
    
    /// Helper: Generate daily flush idempotency key
    /// Format: "flush:{namespace}:{table}:{YYYYMMDD}"
    pub fn daily_flush_key(namespace: &NamespaceId, table: &TableName) -> String {
        let today = chrono::Utc::now().format("%Y%m%d");
        format!("flush:{}:{}:{}", namespace.as_str(), table.as_str(), today)
    }
    
    /// Helper: Generate hourly cleanup idempotency key
    /// Format: "cleanup:{type}:{YYYYMMDDTHH}"
    pub fn hourly_cleanup_key(cleanup_type: &str) -> String {
        let hour = chrono::Utc::now().format("%Y%m%dT%H");
        format!("cleanup:{}:{}", cleanup_type, hour)
    }
}

// ============================================================================
// JobManager - Idempotency Enforcement
// ============================================================================

impl JobManager {
    /// Create job with idempotency check
    /// 
    /// **Idempotency Logic**:
    /// 1. If idempotency_key provided, query system.jobs for active jobs with same key
    /// 2. Active statuses: New, Queued, Running, Retrying
    /// 3. If active job found, return error: "Job already running: {existing_job_id}"
    /// 4. If no active job, create new job and persist to system.jobs
    /// 5. If idempotency_key is None, skip check (allow duplicate jobs)
    ///
    /// **Example Usage**:
    /// ```rust
    /// // Daily flush job (idempotent)
    /// let job_id = JobId::generate(JobType::Flush);
    /// let idempotency_key = Job::daily_flush_key(&namespace_id, &table_name);
    /// let job = Job::new(job_id, JobType::Flush, namespace_id, node_id)
    ///     .with_table_name(table_name)
    ///     .with_idempotency_key(idempotency_key);
    ///
    /// match job_manager.create_job(job).await {
    ///     Ok(job_id) => println!("Job created: {}", job_id),
    ///     Err(e) if e.contains("already running") => println!("Job already exists, skip"),
    ///     Err(e) => eprintln!("Error: {}", e),
    /// }
    /// ```
    pub async fn create_job(&self, job: Job) -> Result<JobId, String> {
        // Check idempotency if key provided
        if let Some(ref key) = job.idempotency_key {
            let active_statuses = vec![
                JobStatus::New,
                JobStatus::Queued,
                JobStatus::Running,
                JobStatus::Retrying,
            ];
            
            // Query system.jobs for active jobs with same idempotency key
            for status in active_statuses {
                let existing = self.jobs_provider
                    .find_by_idempotency_key(key, status)
                    .await?;
                
                if let Some(existing_job) = existing {
                    return Err(format!(
                        "Job already running: {} (status: {:?}, created: {})",
                        existing_job.job_id.as_str(),
                        existing_job.status,
                        existing_job.created_at
                    ));
                }
            }
        }
        
        // No active job found, create new one
        let job_id = job.job_id.clone();
        self.jobs_provider.insert(job).await?;
        self.job_queue.enqueue(job_id.clone()).await?;
        
        self.job_logger.log(
            &job_id,
            LogLevel::Info,
            &format!("Job created (type: {:?})", job.job_type)
        );
        
        Ok(job_id)
    }
}

// ============================================================================
// Job Trait - Common Interface for All Jobs
// ============================================================================

#[async_trait]
pub trait Job: Send + Sync {
    /// Job type identifier
    fn job_type(&self) -> JobType;
    
    /// Execute the job (main work)
    async fn execute(&self, ctx: &JobContext) -> Result<String, String>;
    
    /// Cleanup after job completes/fails
    async fn cleanup(&self, ctx: &JobContext) -> Result<(), String> {
        Ok(()) // Default: no cleanup
    }
    
    /// Check if job should retry on failure
    fn should_retry(&self, error: &str) -> bool {
        false // Default: no retry
    }
    
    /// Retry delay (exponential backoff)
    fn retry_delay(&self, attempt: u32) -> Duration {
        Duration::from_secs(2u64.pow(attempt).min(3600)) // Max 1 hour
    }
}

// ============================================================================
// Concrete Job Implementations
// ============================================================================

/// Flush job - flushes RocksDB buffer to Parquet
pub struct FlushJob {
    table_id: TableId,
    flush_policy: FlushPolicy,
}

#[async_trait]
impl Job for FlushJob {
    fn job_type(&self) -> JobType { JobType::Flush }
    
    async fn execute(&self, ctx: &JobContext) -> Result<String, String> {
        ctx.log(LogLevel::Info, &format!("Flushing table {}", self.table_id));
        
        // Actual flush logic (existing code from user_table_flush.rs, shared_table_flush.rs)
        // ...
        
        Ok(format!("Flushed {} rows", 1000))
    }
    
    fn should_retry(&self, error: &str) -> bool {
        // Retry on transient errors (lock conflicts, I/O errors)
        error.contains("lock") || error.contains("I/O")
    }
}

/// Cleanup job - deletes old completed/failed jobs
pub struct CleanupJob {
    retention_config: RetentionConfig,
}

#[async_trait]
impl Job for CleanupJob {
    fn job_type(&self) -> JobType { JobType::Cleanup }
    
    async fn execute(&self, ctx: &JobContext) -> Result<String, String> {
        ctx.log(LogLevel::Info, "Starting job cleanup");
        
        // Existing logic from job_cleanup.rs
        // ...
        
        Ok(format!("Deleted {} old jobs", 50))
    }
}

// Similar for: RetentionJob, StreamEvictionJob, UserCleanupJob, CompactJob, BackupJob, RestoreJob
```

**Acceptance Scenarios**:

1. **Given** JobManager is initialized at startup, **When** creating a new flush job, **Then** job is created with status=New, assigned JobId with FL- prefix (e.g., "FL-abc123"), and persisted to system.jobs

2. **Given** a job is created with status=New, **When** JobManager processes queue, **Then** job transitions to Queued → Running with timestamp updates and logs to jobs.log: "[FL-abc123] INFO - Job queued" then "[FL-abc123] INFO - Job started"

3. **Given** a job is running, **When** job completes successfully, **Then** status transitions to Completed, result is stored, and logs: "[FL-abc123] INFO - Job completed: Flushed 1000 rows"

4. **Given** a job fails with transient error, **When** Job.should_retry() returns true, **Then** status transitions to Retrying, retry_count increments, and logs: "[FL-abc123] WARN - Job failed, retrying in 2s (attempt 1/3)"

5. **Given** a job fails after max retries (3 attempts), **When** final retry fails, **Then** status transitions to Failed, error_message stored, and logs: "[FL-abc123] ERROR - Job failed permanently: {error}"

6. **Given** server restarts with jobs in Running status, **When** JobManager starts, **Then** incomplete jobs are marked as Failed with error "Server restarted during execution" and logged

7. **Given** all job types (Flush, Cleanup, Retention, StreamEviction, UserCleanup, Compact, Backup, Restore), **When** jobs are created, **Then** each has correct prefix (FL-, CL-, RT-, SE-, UC-, CO-, BK-, RS-) and appears in system.jobs

8. **Given** jobs.log file exists, **When** filtering logs by job ID "FL-abc123", **Then** all entries for that specific job are returned (grep "[FL-abc123]" jobs.log)

9. **Given** JobManager with max_concurrent_jobs=5, **When** 10 jobs are submitted, **Then** 5 jobs execute immediately (Running), 5 jobs wait (Queued), and queue drains as jobs complete

10. **Given** a job is cancelled via CANCEL JOB command, **When** job is running, **Then** job receives cancellation signal, performs cleanup, transitions to Cancelled status, and logs: "[FL-abc123] INFO - Job cancelled by user"

**Idempotency Acceptance Scenarios**:

11. **Given** a daily flush job for table "events" on 2025-11-04, **When** JobManager creates job with idempotency key "flush:default:events:20251104", **Then** job is created successfully with New status

12. **Given** job "FL-abc123" is Running with idempotency key "flush:default:events:20251104", **When** another flush job is created with same idempotency key, **Then** creation fails with error: "Job already running: FL-abc123 (status: Running, created: 1730000000000)"

13. **Given** job "FL-abc123" completed on 2025-11-04 with status=Completed, **When** another flush job is created on 2025-11-04 with same idempotency key "flush:default:events:20251104", **Then** job is created successfully (previous job is completed, not active)

14. **Given** job "FL-abc123" failed and transitioned to Retrying with idempotency key "flush:default:events:20251104", **When** another flush job is created with same key, **Then** creation fails with error: "Job already running: FL-abc123 (status: Retrying, ...)"

15. **Given** hourly cleanup job with idempotency key "cleanup:retention:20251104T14", **When** same cleanup runs twice in same hour, **Then** second attempt fails with "Job already running" error (idempotency prevents duplicate cleanup)

16. **Given** job is created without idempotency key (idempotency_key = None), **When** creating multiple jobs of same type, **Then** all jobs are created successfully (no idempotency check when key is absent)

**Unified Message Field Acceptance Scenarios**:

17. **Given** flush job completes successfully, **When** job.complete() is called with message "Flushed 1,234 rows (5.2 MB) to Parquet in 342ms", **Then** job.message contains success message and job.exception_trace is None

18. **Given** cleanup job fails with RocksDB error, **When** job.fail() is called with error_message "RocksDB read failed: IO error at offset 12345", **Then** job.message contains error summary (not "result" or "error_message" field)

19. **Given** retention job completes with message "Deleted 500 old rows before 2025-10-01", **When** querying system.jobs, **Then** message field is populated (old "result" field no longer exists)

20. **Given** compaction job fails with message "Merge failed: insufficient disk space", **When** querying system.jobs, **Then** message field contains error (old "error_message" field no longer exists)

**Exception Trace Acceptance Scenarios**:

21. **Given** flush job fails with panic/exception, **When** job.fail() is called with exception_trace containing multi-line stack trace, **Then** system.jobs stores full trace in exception_trace field

22. **Given** job completes successfully, **When** job.complete() is called, **Then** exception_trace is explicitly set to None (cleared from previous failures)

23. **Given** job fails with short error "IO error", **When** exception_trace is provided with full stack trace (100+ lines), **Then** message contains concise error, exception_trace contains detailed debugging info

24. **Given** job retry after failure, **When** job.retry() is called with error_message and exception_trace, **Then** both message and exception_trace are updated (not cleared until success)

25. **Given** developer queries system.jobs for failed jobs, **When** filtering WHERE status = 'Failed', **Then** results include both message (error summary) and exception_trace (full stack) for debugging

**Retry Logic Acceptance Scenarios**:

26. **Given** job is created, **When** job.new() is called, **Then** retry_count = 0 and max_retries = 3 (default)

27. **Given** job fails with transient error, **When** job.can_retry() returns true and job.retry() is called, **Then** retry_count increments to 1, status = Retrying, message and exception_trace updated

28. **Given** job has retry_count = 2 (2 previous failures), **When** job fails again and max_retries = 3, **Then** job.can_retry() returns true (2 < 3), job transitions to Retrying for 3rd attempt

29. **Given** job has retry_count = 3 and max_retries = 3, **When** job fails, **Then** job.can_retry() returns false (3 >= 3), job transitions to Failed permanently (no more retries)

30. **Given** job is created with custom max_retries, **When** job.with_max_retries(5) is called, **Then** job can retry up to 5 times before permanent failure

**Parameters Change Acceptance Scenarios**:

31. **Given** flush job is created, **When** parameters are set with JSON object {"threshold_bytes": 1048576, "force": false}, **Then** system.jobs stores parameters as JSON object (not array)

32. **Given** cleanup job with parameters {"retention_days": 30, "batch_size": 1000}, **When** job executor parses parameters, **Then** values are extracted by key (retention_days, batch_size) not array index

33. **Given** legacy job created with old parameters format (JSON array), **When** migrating to new format, **Then** migration converts array ["param1", "param2"] to object {"arg0": "param1", "arg1": "param2"} for backward compatibility

**Architecture Benefits**:

- **Unified Management**: Single JobManager handles ALL job types (no more scheduler fragmentation)
- **Better Observability**: Dedicated jobs.log with filterable job IDs makes debugging 10× easier
- **Shorter IDs**: "FL-abc123" (9 chars) vs "flush-messages-550e8400-..." (45+ chars) = 80% storage reduction
- **Rich Status Model**: 7 statuses (New, Queued, Running, Completed, Failed, Retrying, Cancelled) provide fine-grained lifecycle tracking
- **Crash Recovery**: All jobs in system.jobs enable resume/fail on restart
- **Retry Logic**: Built-in exponential backoff for transient failures
- **Resource Control**: Queue-based execution with concurrency limits prevents overload

**Migration Plan**:

1. **Phase 1** (Day 1): Extend JobStatus enum (add New, Queued, Retrying), update JobType with prefixes, implement JobId with short format
2. **Phase 2** (Day 2): Create JobManager core (queue, executor pool, lifecycle management), implement JobLogger
3. **Phase 3** (Day 3): Define Job trait, migrate FlushJob, CleanupJob, RetentionJob to new pattern
4. **Phase 4** (Day 4): Migrate StreamEvictionJob, UserCleanupJob, add CompactJob/BackupJob/RestoreJob stubs
5. **Phase 5** (Day 5): Wire JobManager into AppContext, update lifecycle.rs to start JobManager
6. **Phase 6** (Day 6): Delete old code (scheduler.rs, individual job managers), update all call sites
7. **Phase 7** (Day 7): Integration tests, crash recovery tests, concurrency tests, documentation

**Files to Create**:
- `backend/crates/kalamdb-core/src/jobs/manager.rs` (~800 lines) - JobManager implementation
- `backend/crates/kalamdb-core/src/jobs/logger.rs` (~150 lines) - JobLogger implementation
- `backend/crates/kalamdb-core/src/jobs/queue.rs` (~300 lines) - Priority queue implementation
- `backend/crates/kalamdb-core/src/jobs/trait.rs` (~100 lines) - Job trait definition
- `backend/crates/kalamdb-core/src/jobs/flush_job.rs` (~200 lines) - FlushJob implementation
- `backend/crates/kalamdb-core/src/jobs/cleanup_job.rs` (~150 lines) - CleanupJob implementation
- `backend/crates/kalamdb-core/src/jobs/retention_job.rs` (~150 lines) - RetentionJob implementation
- `backend/crates/kalamdb-core/src/jobs/stream_eviction_job.rs` (~200 lines) - StreamEvictionJob
- `backend/crates/kalamdb-core/src/jobs/user_cleanup_job.rs` (~150 lines) - UserCleanupJob

**Files to Modify**:
- `backend/crates/kalamdb-commons/src/models/job_status.rs` - Add New, Queued, Retrying
- `backend/crates/kalamdb-commons/src/models/job_type.rs` - Add Retention, StreamEviction, UserCleanup; add prefix() method
- `backend/crates/kalamdb-commons/src/models/job_id.rs` - Add generate(), job_type() methods
- `backend/crates/kalamdb-core/src/app_context.rs` - Add job_manager field
- `backend/src/lifecycle.rs` - Start JobManager, migrate flush/eviction schedulers

**Files to Delete**:
- `backend/crates/kalamdb-core/src/scheduler.rs` (~800 lines) - Replaced by JobManager
- `backend/crates/kalamdb-core/src/jobs/tokio_job_manager.rs` (~400 lines) - Merged into JobManager
- `backend/crates/kalamdb-core/src/jobs/stream_eviction_scheduler.rs` (~300 lines) - Replaced by JobManager

**Net Impact**:
- Lines added: ~2,200 (new unified architecture)
- Lines removed: ~1,500 (old fragmented code)
- Net increase: ~700 lines (but massively improved maintainability, observability, reliability)

---

### Edge Cases

- **Schema Evolution**: What happens when querying old schema versions after multiple ALTER TABLE operations? System must support schema_history array in TableDefinition to retrieve historical schemas.

- **Type Conversion Failures**: How does system handle unsupported or corrupted data types during Arrow conversion? Must return simple, clear error messages indicating unsupported type and suggest valid alternatives (e.g., "Unsupported type 'FOO'. Valid types: BOOLEAN, INT, BIGINT, DOUBLE, FLOAT, TEXT, TIMESTAMP, DATE, DATETIME, TIME, JSON, BYTES, EMBEDDING"). Error messages must be single-level (no "server error: query error: type error:" chains).

- **Cache Invalidation Race Conditions**: What happens if schema is modified while being read from cache? Must use atomic cache updates or versioning to prevent returning stale schemas.

- **Cache Failure Handling**: What happens when cache encounters errors (eviction failure, corruption, capacity exceeded)? System must bypass cache and serve directly from EntityStore, log error to stderr, continue serving requests (graceful degradation over failure).

- **Legacy Code Migration**: How does system handle code still using old schema models during refactoring? Must maintain compilation with clear deprecation warnings until all references are migrated.

- **System Table Schema Changes**: What happens if system table schemas (users, jobs, etc.) need to evolve? Must support schema versioning for system tables just like user tables.

- **Null/Default Value Edge Cases**: How does system handle columns with NULL defaults vs. no default vs. function-call defaults? ColumnDefinition.default_value must distinguish between ColumnDefault::None, ColumnDefault::Literal(null), and ColumnDefault::FunctionCall with function name and optional parameters (e.g., NOW(), SNOWFLAKE(), UUID(), SNOWFLAKE(datacenter_id, worker_id)).

- **Partition Key Validation**: What happens if partition key is defined on non-existent column? Must validate partition_key references against actual column names during CREATE TABLE parsing.

- **Wire Format Compatibility**: How does system handle data written with old wire format tags after type system changes? Must maintain backward compatibility with existing Parquet files.

- **Column Ordering After ALTER TABLE**: What happens to ordinal_position when columns are added or dropped? System must assign next available ordinal to new columns and preserve existing ordinals when dropping columns (no re-numbering).

- **SELECT * Column Order**: How does system ensure SELECT * returns columns in consistent order? Must sort ColumnDefinition array by ordinal_position before building Arrow schema.

- **EMBEDDING Dimension Validation**: What happens if EMBEDDING dimension is zero or excessively large? System must validate dimension is between 1 and 8192 during CREATE TABLE parsing, reject with error "EMBEDDING dimension must be between 1 and 8192, got: {dimension}" for out-of-range values.

- **EMBEDDING Type Conversion**: How does KalamDataType::EMBEDDING convert to Arrow? Must map to FixedSizeList<Float32> with specified dimension, ensuring Arrow schema compatibility.

- **Parameterized Type Extensibility**: How does system support future parameterized types (VARCHAR(N), DECIMAL(P,S))? KalamDataType enum must use associated data pattern for type parameters, wire format must encode parameters after tag byte.

- **Schema History Storage Size**: What happens if schema_history array grows very large (100+ versions)? For Alpha, embed all history in TableDefinition for simplicity. Future optimization: move old versions to cold storage if history exceeds 50 versions.

- **Schema History Query Performance**: How to query specific historical schema versions efficiently? TableDefinition.schema_history is Vec<SchemaVersion> sorted by version number, binary search for O(log n) lookup.

## Requirements *(mandatory)*

### Functional Requirements

#### Schema Consolidation (FR-SC)

- **FR-SC-001**: System MUST consolidate all schema models (TableDefinition, ColumnDefinition, SchemaVersion, SystemTable, InformationSchemaTable, UserTableCounter, TableSchema) into \`kalamdb-commons/src/models/schemas/\` directory

- **FR-SC-002**: TableDefinition MUST use type-safe TableOptions enum (not HashMap<String, String>) with variants matching TableType: User→UserTableOptions, Shared→SharedTableOptions, Stream→StreamTableOptions, System→SystemTableOptions

- **FR-SC-002b**: UserTableOptions MUST include: partition_by_user (bool), max_rows_per_user (u64), enable_rls (bool), compression (String: "none"|"snappy"|"lz4"|"zstd")

- **FR-SC-002c**: SharedTableOptions MUST include: access_level (String: "public"|"restricted"), enable_cache (bool), cache_ttl_seconds (u64), compression (String), enable_replication (bool)

- **FR-SC-002d**: StreamTableOptions MUST include: ttl_seconds (u64, REQUIRED), eviction_strategy (String: "time_based"|"size_based"|"hybrid"), max_stream_size_bytes (u64), enable_compaction (bool), watermark_delay_seconds (u64), compression (String)

- **FR-SC-002e**: SystemTableOptions MUST include: read_only (bool, default=true), enable_cache (bool), cache_ttl_seconds (u64), localhost_only (bool)

- **FR-SC-002f**: TableOptions enum MUST use serde tagged representation (#[serde(tag = "table_type")]) for clean JSON serialization with table type discriminator

- **FR-SC-003**: ColumnDefinition MUST include fields: column_name, ordinal_position, data_type (KalamDataType), is_nullable, is_primary_key, is_partition_key, default_value (ColumnDefault enum), column_comment (optional)

- **FR-SC-004**: TableDefinition MUST embed SchemaVersion history as Vec<SchemaVersion> to track ALTER TABLE evolution

- **FR-SC-005**: System MUST remove all duplicate schema model definitions after refactoring (models in kalamdb-sql, kalamdb-api, kalamdb-core must import from commons)

- **FR-SC-006**: System MUST implement schema caching with cache invalidation on schema modifications (ALTER TABLE, DROP TABLE)

- **FR-SC-006b**: Schema cache MUST implement graceful degradation: on cache errors (eviction failure, corruption, capacity exceeded), bypass cache and serve from EntityStore directly, log error to stderr using error! macro

- **FR-SC-007**: All DESCRIBE TABLE, SHOW TABLES, and information_schema queries MUST use consolidated schema models from kalamdb-commons

- **FR-SC-008**: System table schemas (users, jobs, namespaces, storages, live_queries, tables, table_schemas) MUST be defined using consolidated models

- **FR-SC-009**: Schema read operations (DESCRIBE TABLE, information_schema queries) MUST be accessible to all authenticated users (user, service, dba, system roles)

- **FR-SC-010**: Schema modification operations (CREATE TABLE, ALTER TABLE, DROP TABLE) MUST require DBA or system role (reject with error message "Schema modification requires DBA or system role" for user/service roles)

#### Unified Data Type System (FR-DT)

- **FR-DT-001**: System MUST define KalamDataType enum with variants: BOOLEAN, INT (i32), BIGINT (i64), DOUBLE (f64), FLOAT (f32), TEXT (UTF-8), TIMESTAMP (microseconds), DATE (days since epoch), DATETIME (microseconds), TIME (microseconds since midnight), JSON (UTF-8 JSON), BYTES (raw binary), EMBEDDING (f32 vector with fixed length for ML/AI embeddings)

- **FR-DT-002**: Each KalamDataType MUST have associated wire format tag: BOOLEAN=0x01, INT=0x02, BIGINT=0x03, DOUBLE=0x04, FLOAT=0x05, TEXT=0x06, TIMESTAMP=0x07, DATE=0x08, DATETIME=0x09, TIME=0x0A, JSON=0x0B, BYTES=0x0C, EMBEDDING=0x0D

- **FR-DT-002b**: KalamDataType variants with length parameters (EMBEDDING, future TEXT with max_length, etc.) MUST store dimension/length as associated data: EMBEDDING(usize) for vector dimension (e.g., EMBEDDING(1536) for OpenAI embeddings)

- **FR-DT-003**: System MUST provide cached conversion functions from KalamDataType to Arrow DataType (to_arrow_type) and DataFusion types

- **FR-DT-004**: System MUST provide cached conversion functions from Arrow DataType to KalamDataType (from_arrow_type)

- **FR-DT-005**: Type conversions MUST be bidirectional and lossless (KalamDataType → Arrow → KalamDataType yields identical type)

- **FR-DT-006**: System MUST use DashMap or similar lock-free cache for type conversions to enable high-concurrency access

- **FR-DT-007**: ColumnDefinition MUST use KalamDataType for data_type field instead of String representation

- **FR-DT-008**: System MUST remove all other data type enums/representations from codebase (replace with KalamDataType)

- **FR-DT-009**: Wire format encoding MUST use tag byte followed by value bytes as specified: TEXT=[tag][4-byte length][UTF-8 bytes], BIGINT=[tag][8-byte i64 little-endian], EMBEDDING=[tag][4-byte dimension][dimension × 4-byte f32 values]

- **FR-DT-009b**: Wire format for parameterized types MUST encode dimension/length immediately after tag byte: EMBEDDING=[0x0D][4-byte dimension N][N × f32 little-endian], enabling future types like VARCHAR(N)=[tag][4-byte max_length][actual UTF-8 bytes]

- **FR-DT-010**: System MUST use ColumnDefinition.ordinal_position to return columns in correct order for SELECT * queries (sort by ordinal_position ascending)

- **FR-DT-011**: ColumnDefinition.ordinal_position MUST be preserved across schema changes (ALTER TABLE ADD COLUMN assigns next available ordinal, DROP COLUMN preserves existing ordinals)

- **FR-DT-012**: System MUST support EMBEDDING type for vector embeddings with configurable dimensions (common sizes: 384, 768, 1536, 3072 for different embedding models)

- **FR-DT-012b**: System MUST validate EMBEDDING dimension is between 1 and 8192 during CREATE TABLE parsing, reject with error "EMBEDDING dimension must be between 1 and 8192, got: {dimension}" for out-of-range values (hard limit to prevent resource exhaustion)

- **FR-DT-013**: KalamDataType enum architecture MUST support future parameterized types through associated data pattern: enum variants can carry dimension/length parameters (e.g., Embedding(usize), future VarChar(usize), Decimal(u8, u8) for precision/scale)

#### EntityStore Integration (FR-STORE)

- **FR-STORE-001**: TableDefinition MUST be stored in EntityStore using TableId as key type (TableId already contains namespace.tableName, making it globally unique)

- **FR-STORE-002**: System MUST implement EntityStore<TableId, TableDefinition> in backend/crates/kalamdb-core/src/tables/system/schemas for information_schema.tables persistence

- **FR-STORE-003**: SchemaVersion history MUST be stored as embedded Vec<SchemaVersion> within TableDefinition (not separate table)

- **FR-STORE-004**: System MUST provide EntityStore-based retrieval methods for table schemas by TableId (no secondary indexes needed since TableId is unique and contains namespace.tableName)

- **FR-STORE-005**: Schema modifications (CREATE TABLE, ALTER TABLE, DROP TABLE) MUST use EntityStore batch operations for atomic updates

- **FR-STORE-006**: System MUST integrate with existing SystemTableStore<K,V> infrastructure for consistency with other system tables (users, jobs, namespaces)

- **FR-STORE-007**: TableDefinition serialization MUST use bincode for efficient storage (consistent with other EntityStore implementations)

#### Test Suite Completion (FR-TEST)

- **FR-TEST-001**: Backend test suite (\`cargo test\` in backend/) MUST achieve 100% pass rate with 0 failures, 0 panics

- **FR-TEST-002**: CLI test suite (\`cargo test\` in cli/) MUST achieve 100% pass rate including auth, query, and subscription tests

- **FR-TEST-003**: Link/WASM SDK test suite (TypeScript tests in link/sdks/typescript/tests/) MUST achieve 100% pass rate (currently 14 tests)

- **FR-TEST-004**: System MUST fix failing tests by correcting implementation bugs, NOT by modifying tests to match broken behavior

- **FR-TEST-005**: Integration tests MUST verify schema consolidation by querying schemas through multiple code paths (SQL parser, DESCRIBE, information_schema)

- **FR-TEST-006**: Type conversion tests MUST verify all KalamDataType conversions to/from Arrow types are lossless

- **FR-TEST-007**: Tests MUST use consolidated schema models (no legacy model references allowed)

- **FR-TEST-008**: Test coverage MUST include edge cases: schema evolution, cache invalidation, concurrent schema reads, type conversion errors

- **FR-TEST-009**: System MUST provide integration test suite for User Story 1 (Schema Consolidation) verifying schema consistency across DESCRIBE, information_schema, and internal APIs

- **FR-TEST-010**: System MUST provide integration test suite for User Story 2 (Unified Data Types) verifying all KalamDataType conversions and column ordering

- **FR-TEST-011**: System MUST provide integration test suite for User Story 3 (Test Suite Completion) validating end-to-end workflows with consolidated schemas

- **FR-TEST-012**: System MUST provide integration test suite for User Story 4 (Schema Caching) measuring cache hit rates and performance under concurrent load

- **FR-TEST-013**: System MUST provide integration test suite for User Story 7 (Cache Consolidation) verifying single unified cache serves all use cases (DESCRIBE TABLE, FLUSH TABLE, schema queries) with >99% hit rate, <100μs latency, and ~50% memory reduction vs dual-cache baseline

#### Code Refactoring (FR-REFACTOR)

- **FR-REFACTOR-001**: System MUST refactor all code using old schema models to import from \`kalamdb_commons::schemas::*\`

- **FR-REFACTOR-002**: Duplicate schema-related code MUST be extracted into shared utility functions in commons module

- **FR-REFACTOR-003**: System MUST remove old model definitions immediately (no deprecation needed - unreleased version, no backward compatibility required)

- **FR-REFACTOR-004**: All schema serialization/deserialization code MUST be centralized in schema model implementations

- **FR-REFACTOR-005**: System MUST update SQL parser (CREATE TABLE, ALTER TABLE) to populate new schema models

- **FR-REFACTOR-006**: System MUST update DataFusion table providers to consume schema from consolidated models

#### Code Quality & Performance (FR-QUALITY)

- **FR-QUALITY-001**: All new code MUST include comprehensive documentation (module docs, function docs, inline comments for complex logic)

- **FR-QUALITY-002**: System MUST be memory efficient - no memory leaks, minimize allocations, use Arc/Rc for shared data, avoid unnecessary clones

- **FR-QUALITY-003**: Schema models MUST use efficient data structures (Vec for columns sorted by ordinal_position, HashMap for table_options, Arc for schema cache)

- **FR-QUALITY-004**: Type conversion cache MUST use memory-bounded DashMap with maximum size limit (prevent unbounded growth)

- **FR-QUALITY-005**: Schema history MUST use efficient storage - bincode serialization for TableDefinition minimizes overhead compared to JSON

- **FR-QUALITY-006**: All performance-critical paths MUST be profiled to ensure no memory leaks or excessive allocations (use cargo-instruments, valgrind, or heaptrack)

- **FR-QUALITY-007**: Code MUST follow Rust idioms - leverage ownership, avoid unsafe unless necessary, use Result<T,E> for errors, prefer iterators over loops

- **FR-QUALITY-008**: All public APIs MUST have examples in documentation showing correct usage patterns

#### Observability (FR-OBS)

- **FR-OBS-001**: System MUST log all schema operation errors to stderr using Rust `log` crate (error! macro)

- **FR-OBS-002**: Error logs MUST include: operation type (CREATE/ALTER/DROP), table_id, error message, timestamp

- **FR-OBS-003**: System MUST NOT implement metrics or distributed tracing for Alpha release (minimal observability scope)

- **FR-OBS-004**: Cache operations (hits/misses) MUST NOT be logged in production (performance overhead), only critical errors

#### Error Handling (FR-ERR)

- **FR-ERR-001**: All schema operation errors MUST return simple, single-level error messages (plain strings without nested prefixes like "server error: query error:")

- **FR-ERR-002**: Error messages MUST be straightforward and helpful, including context and suggested fixes where applicable (e.g., "Unsupported type 'FOO'. Valid types: BOOLEAN, INT, BIGINT, ...")

- **FR-ERR-003**: Error messages MUST avoid redundant error type prefixes that create message chains (e.g., avoid "SchemaError: TableError: ColumnError: Invalid type")

- **FR-ERR-004**: Type conversion errors MUST list valid type alternatives in the error message

### Key Entities

- **TableDefinition**: Complete table metadata including table_id (TableId type - unique identifier containing namespace.tableName), table_name, namespace_id, table_type (SYSTEM/USER/SHARED/STREAM), created_at, updated_at, schema_version, storage_id, use_user_storage, flush_policy, deleted_retention_hours, ttl_seconds, columns (Vec<ColumnDefinition> sorted by ordinal_position), schema_history (Vec<SchemaVersion> embedded), table_options (HashMap<String, String>). Stored via EntityStore<TableId, TableDefinition> in backend/crates/kalamdb-core/src/tables/system/schemas.

- **ColumnDefinition**: Column metadata including column_name, ordinal_position (1-indexed, immutable), data_type (KalamDataType), is_nullable, is_primary_key, is_partition_key, default_value (ColumnDefault with support for parameterized functions), column_comment. Used to determine SELECT * column order.

- **SchemaVersion**: Schema history entry including version number, created_at timestamp, changes description, arrow_schema_json. Stored as embedded array within TableDefinition (not separate EntityStore).

- **KalamDataType**: Canonical data type enum with 13 variants (BOOLEAN, INT, BIGINT, DOUBLE, FLOAT, TEXT, TIMESTAMP, DATE, DATETIME, TIME, JSON, BYTES, EMBEDDING), wire format tags (0x01-0x0D), Arrow/DataFusion conversion methods with caching. Supports parameterized types via associated data: EMBEDDING(usize) stores dimension, enabling future types like VARCHAR(usize), DECIMAL(u8, u8). Examples: EMBEDDING(384) for sentence embeddings, EMBEDDING(1536) for OpenAI ada-002, EMBEDDING(3072) for GPT-4 embeddings.

- **ColumnDefault**: Enum for default value specifications including None (no default), Literal(Value) for static values, FunctionCall { name: String, args: Vec<String> } for parameterized function defaults (e.g., NOW(), UUID(), SNOWFLAKE(datacenter_id, worker_id)).

- **TableType**: Enum with 4 variants - SYSTEM (system tables like users, jobs), USER (user-created tables), SHARED (cross-user shared tables), STREAM (stream tables with TTL and eviction).

- **SystemTable**: Enum representing system tables (Users, Jobs, Namespaces, Storages, LiveQueries, Tables, TableSchemas) with associated TableDefinition instances. All system tables use EntityStore for persistence.

- **InformationSchemaTable**: SQL standard schema tables (information_schema.tables, information_schema.columns) backed by consolidated schema models retrieved from EntityStore in backend/crates/kalamdb-core/src/tables/system/schemas.

- **SchemaCache**: In-memory cache of TableDefinition objects using DashMap for lock-free concurrent access, with LRU eviction policy. Cache sits in front of EntityStore for frequently accessed schemas.

- **TableSchemaStore**: Concrete implementation of EntityStore<TableId, TableDefinition> following SystemTableStore pattern from Phase 14, located in backend/crates/kalamdb-core/src/tables/system/schemas. No secondary indexes needed since TableId is unique and contains namespace.tableName.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Schema query performance improves by 10× - repeated DESCRIBE TABLE queries complete in under 100 microseconds (currently ~1-2ms due to repeated parsing)

- **SC-002**: Codebase complexity reduces by 30% - eliminate ~1000 lines of duplicate schema-related code across kalamdb-sql, kalamdb-api, kalamdb-core

- **SC-003**: Type conversion overhead reduces by 5× - cached KalamDataType↔Arrow conversions complete in under 10 microseconds (currently ~50μs due to repeated string parsing)

- **SC-004**: Test suite reliability reaches 100% - all tests in backend/, cli/, and link/ pass consistently without flakiness

- **SC-005**: Schema-related bugs reduce to zero - no reports of schema inconsistencies between information_schema queries, DESCRIBE TABLE, and internal schema storage

- **SC-006**: Developer productivity improves - schema-related code changes require updates in only 1 location (kalamdb-commons/src/models/schemas/) instead of 4-6 locations

- **SC-007**: Cache hit rate exceeds 99% - schema cache serves 99%+ of schema queries without database access in production workloads

- **SC-008**: Memory efficiency improves - deduplicated schema models reduce memory usage by 40% for workloads with 1000+ tables (eliminate per-query schema clones)

- **SC-009**: Build time reduces by 20% - fewer model definitions and imports speed up incremental compilation

- **SC-010**: Alpha release readiness achieved - database passes all functional, integration, and performance tests with zero known critical bugs

- **SC-011**: Column ordering consistency - 100% of SELECT * queries return columns in ordinal_position order with zero ordering bugs reported

- **SC-012**: EntityStore integration complete - all table schemas persisted via EntityStore with same performance characteristics as other system tables (sub-millisecond reads/writes)

- **SC-013**: Memory efficiency validated - schema operations show zero memory leaks under valgrind/heaptrack, stable memory usage under sustained load

- **SC-014**: Code quality standards met - all modules have comprehensive documentation, examples in public API docs, no clippy warnings

## Assumptions

### Technical Assumptions

- **No Backward Compatibility Required**: This is an unreleased version - breaking changes to storage format and APIs are acceptable. No migration of existing data needed.

- **Fresh Start**: All existing development databases can be wiped and recreated with new schema models. No preservation of old data required.

- **Schema Versioning**: Current schema_version field in TableDefinition is sufficient for tracking ALTER TABLE changes (no need for complex migration framework in Alpha)

- **Cache Sizing**: Default schema cache size of 1000 entries is sufficient for typical workloads (can be made configurable later)

- **Type Coverage**: 13 data types in KalamDataType cover all currently supported types including vector embeddings (EMBEDDING for ML/AI use cases)

- **Vector Dimension Limits**: EMBEDDING dimensions up to 8192 are sufficient for current embedding models (OpenAI: 1536/3072, sentence-transformers: 384/768/1024, Cohere: 4096)

- **Parameterized Type Pattern**: Using associated data in enum variants (e.g., FloatArray(usize)) provides clean architecture for future parameterized types (VARCHAR, DECIMAL) without API breaking changes

- **Test Environment**: All tests can run in isolated environments without external dependencies (embedded RocksDB, in-memory tables)

- **Ordinal Position Stability**: Column ordinal positions remain stable across schema changes (never recomputed or renumbered)

- **EntityStore Compatibility**: Phase 14 EntityStore infrastructure is complete and stable for TableDefinition storage

- **Schema History Embedded Storage**: Embedding schema_history in TableDefinition is optimal for Alpha because: (1) schema changes are infrequent, (2) most queries need current schema only, (3) simpler atomic updates, (4) can optimize later if history grows large (move to cold storage after 50+ versions)

- **Schema History Size**: Schema history limited to reasonable number of versions (estimate <100 versions per table over lifetime for Alpha workloads)

### Operational Assumptions

- **Performance Impact**: Schema caching provides measurable performance benefit (validated through benchmarking before full rollout)

- **Cache Invalidation**: Schema modifications are infrequent enough that cache invalidation overhead is negligible

- **Lock-Free Safety**: DashMap provides sufficient concurrency guarantees for schema cache without custom locking

- **Test Coverage**: Existing test cases cover major functionality (new tests needed only for edge cases)

## Out of Scope

The following are explicitly **not** included in this feature:

- **Advanced Type Features**: Custom user-defined types, composite types, array types, or nested structures beyond JSON
- **Schema Migration Tools**: Automated ALTER TABLE migration scripts or schema diff utilities
- **Schema Versioning UI**: Visual tools for browsing schema history or rolling back schema changes
- **Distributed Schema Sync**: Cross-node schema synchronization for distributed deployments
- **Schema Permissions**: Fine-grained column-level access control or schema-level permissions
- **Performance Benchmarking Suite**: Comprehensive performance test framework (only basic cache benchmarks included)
- **Documentation Generation**: Auto-generated schema documentation or ER diagrams from TableDefinition models
- **Schema Import/Export**: Tools for importing schemas from other databases or exporting to SQL DDL
- **Type Inference**: Automatic data type detection from sample data or CSV files

These features may be considered for future releases after Alpha.

---

## User Story 8 - Application Context Consolidation (Priority: P0)

**Status**: Critical architectural refactoring (2025-11-03)

Developers need a single source of truth for shared application state instead of passing duplicate Arc-wrapped dependencies through multiple layers. Currently, KalamCore, SqlExecutor, and ApplicationComponents each maintain their own copies of stores, services, and managers, leading to ~70% redundant Arc allocations, complex constructor signatures, and maintenance burden. The system must consolidate all shared resources into a single AppContext singleton that provides global access to stores, services, and managers.

**Why this priority**: This is **foundational infrastructure debt** that blocks efficient development:
- **Memory Waste**: Multiple Arc wrappers for same instances (3× UserTableStore, 3× SharedTableStore, 2× KalamSql per request)
- **API Pollution**: 10+ parameter constructors, 15+ `with_*()` builder methods on SqlExecutor
- **Code Duplication**: Same initialization logic in lifecycle.rs, tests, and examples
- **Testing Pain**: Every test must manually wire 10+ dependencies instead of calling AppContext::init()
- **Maintenance Burden**: Adding a new shared resource requires updating 5+ files

**Independent Test**: Can be fully tested by running existing test suite with AppContext-based architecture and verifying:
1. All tests pass without modification (same behavior)
2. Constructor calls reduced from 10+ params to 0-2 params
3. Memory profiling shows ~70% reduction in Arc allocations
4. Build time improves due to simpler dependency graphs

**Acceptance Scenarios**:

1. **Given** AppContext is initialized once at application startup, **When** any component needs access to stores or services, **Then** it calls AppContext::get() instead of storing its own Arc references

2. **Given** AppContext contains all shared resources (KalamSql, UserTableStore, SharedTableStore, StreamTableStore, StorageBackend, LiveQueryManager, JobManager), **When** a service needs a dependency, **Then** it retrieves it from AppContext without constructor injection

3. **Given** SqlExecutor is refactored to use AppContext, **When** creating a new SqlExecutor, **Then** constructor requires only 2 parameters (NamespaceService, SessionContext) instead of 10+ parameters and 8+ with_*() calls

4. **Given** Services (UserTableService, SharedTableService, StreamTableService) are refactored, **When** creating service instances, **Then** services become stateless singletons with zero-parameter constructors (all state comes from AppContext)

5. **Given** lifecycle.rs initialization, **When** bootstrapping application, **Then** AppContext::init(backend) is called once, and all subsequent components use AppContext::get()

6. **Given** test setup code, **When** creating test fixtures, **Then** single create_test_app_context() call replaces 50+ lines of setup boilerplate

7. **Given** memory profiling before and after AppContext refactoring, **When** measuring Arc allocation count for 1000 requests, **Then** Arc allocations reduce by ~70% (from 11,000 to ~3,300)

8. **Given** ApplicationComponents struct in lifecycle.rs, **When** refactored to use AppContext, **Then** ApplicationComponents only contains HTTP-layer state (JwtAuth, RateLimiter, server handles) not core stores/services

9. **Given** AppContext uses OnceCell for thread-safe singleton initialization, **When** multiple threads call AppContext::get() concurrently, **Then** all threads receive same Arc<AppContext> reference with no race conditions

10. **Given** AppContext exposes getter methods for each resource, **When** components access resources, **Then** getters return Arc<T> clones (cheap reference count increment, zero allocation)

**Architecture Change**:
```rust
// OLD: Multiple layers of Arc wrappers, complex constructors
KalamCore { user_table_store, shared_table_store, stream_table_store }
ApplicationComponents { session_factory, sql_executor, jwt_auth, rate_limiter, flush_scheduler, live_query_manager, stream_eviction_scheduler, rocks_db_adapter }
SqlExecutor::new(namespace_service, session_context, user_table_service, shared_table_service, stream_table_service)
    .with_table_deletion_service(...)
    .with_storage_registry(...)
    .with_job_manager(...)
    .with_live_query_manager(...)
    .with_stores(user_table_store, shared_table_store, stream_table_store, kalam_sql)
    .with_password_complexity(...)
    .with_storage_backend(...)
    .with_schema_store(...)
// Every component stores Arc<Store>, Arc<Service>, Arc<Manager>

// NEW: Single AppContext singleton, minimal constructors
AppContext::init(backend)?;  // Called once at startup
let ctx = AppContext::get(); // Get global singleton anywhere

SqlExecutor::new(namespace_service, session_context) // 2 params only
UserTableService::new() // 0 params, stateless
SharedTableService::new() // 0 params, stateless

// Inside implementation:
impl UserTableService {
    pub fn create_table(&self, stmt: CreateTableStatement) -> Result<()> {
        let ctx = AppContext::get();
        let store = ctx.user_table_store();  // Arc<UserTableStore>
        let kalam_sql = ctx.kalam_sql();     // Arc<KalamSql>
        // Use stores without storing them as fields
    }
}
```

**Visual Architecture**:
```
BEFORE (Current):
┌─────────────────────────────────────────────────────────────┐
│ ApplicationComponents                                       │
├─────────────────────────────────────────────────────────────┤
│ - session_factory: Arc<DataFusionSessionFactory>           │
│ - sql_executor: Arc<SqlExecutor>                           │
│ - jwt_auth: Arc<JwtAuth>                                   │
│ - rate_limiter: Arc<RateLimiter>                           │
│ - flush_scheduler: Arc<FlushScheduler>                     │
│ - live_query_manager: Arc<LiveQueryManager>                │
│ - stream_eviction_scheduler: Arc<StreamEvictionScheduler>  │
│ - rocks_db_adapter: Arc<RocksDbAdapter>                    │
└─────────────────────────────────────────────────────────────┘
           │
           ├──────────────────────────────────────────┐
           ▼                                          ▼
┌─────────────────────────────┐    ┌──────────────────────────────┐
│ SqlExecutor (17 fields!)    │    │ Services (each has stores)   │
├─────────────────────────────┤    ├──────────────────────────────┤
│ - user_table_store          │    │ UserTableService             │
│ - shared_table_store        │    │ - kalam_sql: Arc<...>        │
│ - stream_table_store        │    │ - user_table_store: Arc<...> │
│ - kalam_sql                 │    │                              │
│ - storage_backend           │    │ SharedTableService           │
│ - jobs_table_provider       │    │ - kalam_sql: Arc<...>        │
│ - users_table_provider      │    │ - shared_table_store: Arc<>  │
│ - storage_registry          │    │                              │
│ - job_manager               │    │ StreamTableService           │
│ - live_query_manager        │    │ - kalam_sql: Arc<...>        │
│ - schema_store              │    │ - stream_table_store: Arc<>  │
│ - unified_cache             │    │                              │
│ - table_deletion_service    │    └──────────────────────────────┘
│ - ...                       │
└─────────────────────────────┘

PROBLEM: Same resources (KalamSql, stores) duplicated 3-4 times!
         Each request allocates 11+ Arc instances
         Constructor takes 10+ parameters + 8 .with_*() calls


AFTER (AppContext):
┌─────────────────────────────────────────────────────────────┐
│ AppContext (Global Singleton)                               │
├─────────────────────────────────────────────────────────────┤
│ CORE STORES:                                                │
│ - kalam_sql: Arc<KalamSql>                    ◄────────────┐│
│ - user_table_store: Arc<UserTableStore>       ◄──────────┐ ││
│ - shared_table_store: Arc<SharedTableStore>   ◄────────┐ │ ││
│ - stream_table_store: Arc<StreamTableStore>   ◄──────┐ │ │ ││
│ - storage_backend: Arc<dyn StorageBackend>           │ │ │ ││
│                                                      │ │ │ ││
│ MANAGERS:                                            │ │ │ ││
│ - job_manager: Arc<dyn JobManager>                   │ │ │ ││
│ - live_query_manager: Arc<LiveQueryManager>          │ │ │ ││
│                                                      │ │ │ ││
│ REGISTRIES & CACHES:                                 │ │ │ ││
│ - storage_registry: Arc<StorageRegistry>             │ │ │ ││
│ - unified_cache: Arc<SchemaCache>                    │ │ │ ││
│                                                      │ │ │ ││
│ SYSTEM PROVIDERS:                                    │ │ │ ││
│ - jobs_provider: Arc<JobsTableProvider>              │ │ │ ││
│ - users_provider: Arc<UsersTableProvider>            │ │ │ ││
│ - schema_store: Arc<TableSchemaStore>                │ │ │ ││
└──────────────────────────────────────────────────────┼─┼─┼─┼┘
                                                       │ │ │ │
         ┌─────────────────────────────────────────────┘ │ │ │
         │                         ┌─────────────────────┘ │ │
         │                         │           ┌───────────┘ │
         │                         │           │     ┌───────┘
         ▼                         ▼           ▼     ▼
┌──────────────────┐  ┌──────────────────────────────────────┐
│ SqlExecutor      │  │ Services (Stateless)                 │
├──────────────────┤  ├──────────────────────────────────────┤
│ - namespace      │  │ UserTableService::new()   (0 params) │
│ - session        │  │   fn create_table() {                │
│ (2 fields only!) │  │     let ctx = AppContext::get();     │
└──────────────────┘  │     let store = ctx.user_table_...() │
                      │   }                                  │
┌──────────────────┐  │                                      │
│ ApplicationComp  │  │ SharedTableService::new() (0 params) │
├──────────────────┤  │ StreamTableService::new() (0 params) │
│ - jwt_auth       │  │ TableDeletionService::new() (0 param)│
│ - rate_limiter   │  └──────────────────────────────────────┘
│ - flush_sched    │
│ - stream_evict   │  BENEFIT: Every service/component gets
│ (4 fields only!) │           resources from AppContext
└──────────────────┘           → Zero duplication
                               → Clean constructors
                               → Easy testing
```

**Benefits**:
- **Memory Efficiency**: ~70% reduction in Arc allocations (3× stores → 1× shared)
- **Code Simplicity**: Constructors go from 10+ params to 0-2 params
- **Test Simplicity**: Single create_test_app_context() helper for all tests
- **Maintainability**: Add new resource in 1 place (AppContext), all code gets access
- **Type Safety**: Singleton ensures exactly one instance, compile-time guarantee
- **Thread Safety**: OnceCell provides atomic initialization, no race conditions

**Migration Impact**:
- **Files Modified**: ~40 files (lifecycle.rs, executor.rs, all services, all tests)
- **Breaking Changes**: All service constructors, SqlExecutor API, test helpers
- **Lines Added**: ~400 (AppContext + getters + docs)
- **Lines Removed**: ~1,200 (duplicate Arc fields, builder methods, test boilerplate)
- **Net Reduction**: ~800 lines of code

---

### User Story 8.1 – SchemaRegistry (P0): thin service over SchemaCache

Problem
- Callers need a single entry point to retrieve table schema and derived artifacts (Arrow schema, UserTableShared), with read-through behavior to the authoritative store. The low-level SchemaCache is excellent for concurrency, but it doesn’t orchestrate fetch-on-miss, derivations, or invalidation policy.

Decision
- Keep SchemaCache as the low-level, lock-free cache. Introduce SchemaRegistry as a tiny, memory-efficient facade that coordinates storage, caching, derivations, and invalidation.

Responsibilities (all zero-copy via Arc):
- get_table_data(TableId) -> Arc<CachedTableData>
- get_table_definition(TableId) -> Arc<TableDefinition>
- get_arrow_schema(TableId) -> Arc<datafusion::arrow::datatypes::SchemaRef>
- get_user_table_shared(TableId) -> Arc<UserTableShared> (create once; cache in SchemaCache)
- invalidate(TableId) on ALTER/DROP (evicts data + derived artifacts)
- resolve_storage_path(TableId, Option<UserId>, Option<u32>) -> String (delegates to SchemaCache)

Implementation sketch
```rust
pub struct SchemaRegistry {
    cache: Arc<SchemaCache>,
    system_store: Arc<TableSchemaStore>, // or Arc<dyn SystemTableStore<TableId, TableDefinition>>
}

impl SchemaRegistry {
    pub async fn get_table_data(&self, id: &TableId) -> Result<Arc<CachedTableData>, KalamDbError> { /* read-through */ }
    pub async fn get_table_definition(&self, id: &TableId) -> Result<Arc<TableDefinition>, KalamDbError> { /* Arc clone of inner */ }
    pub async fn get_arrow_schema(&self, id: &TableId) -> Result<Arc<SchemaRef>, KalamDbError> { /* memoize derived */ }
    pub async fn get_user_table_shared(&self, id: &TableId) -> Result<Arc<UserTableShared>, KalamDbError> { /* create-once */ }
    pub fn invalidate(&self, id: &TableId) { self.cache.invalidate(id) }
}
```

Memory/Perf notes
- Single copy per table for heavy artifacts (definition, Arrow schema, UserTableShared). Callers only get Arc clones.
- Derived Arrow schema memoized via OnceCell inside CachedTableData or a tiny DashMap keyed by TableId.
- Hot-path operations remain O(1) DashMap lookups; no blocking locks.

AppContext
- Include Arc<SchemaRegistry> alongside Arc<SchemaCache>. Most callers use SchemaRegistry; internal tight loops can still use SchemaCache directly.

Adoption plan
- Phase 1: Implement SchemaRegistry and wire into AppContext (no call-site changes yet).
- Phase 2: Migrate providers/services/executor to use SchemaRegistry for schema/derived artifacts.
- Phase 3: Route all invalidation through SchemaRegistry; keep SchemaCache API available for advanced internals.

---

### User Story 10 - SQL Executor Modular Architecture (Priority: P0)

**Status**: Design proposal (2025-11-05)

Developers need a high-performance, memory-efficient, and secure SQL executor with modular handler architecture. Currently, `executor.rs` is a monolithic 4,956-line file with all SQL handling logic inline, making it difficult to maintain, test, and optimize. The system must refactor into focused handler modules while optimizing for **single-pass SQL parsing**, **authorization gateway routing**, and **future query caching support** for parameterized queries.

**Why this priority**: SQL execution is the critical hot path for ALL database operations. Current monolithic architecture causes:
- **Performance Bottleneck**: No query caching, no statement reuse, duplicate parsing overhead
- **Memory Inefficiency**: Large monolithic struct with excessive allocations per request
- **Security Risk**: Authorization checks scattered throughout code, no centralized security gateway
- **Maintainability Crisis**: 4,956 lines in one file, difficult to review/modify/test individual features
- **Testing Complexity**: Cannot unit test individual handlers in isolation, only integration tests

**Performance Requirements** (Critical):
- **Fast Execution**: Sub-millisecond latency for simple queries (SELECT/INSERT/UPDATE/DELETE)
- **Low Memory**: Zero unnecessary allocations on hot path, efficient Arc/String reuse
- **Secure**: Authorization checked ONCE before handler routing, SQL injection prevention via DataFusion

**Architecture Principles**:

1. **Single-Pass SQL Parsing**: Parse SQL statement ONCE, classify statement type, route to appropriate handler
2. **Authorization Gateway**: Check authentication/authorization BEFORE routing to handlers (fail-fast security)
3. **Modular Handlers**: Each handler module responsible for one statement category (DDL, DML, query, etc.)
4. **Type-Safe Identifiers**: ALL identifiers use type-safe wrappers (NamespaceId, UserId, TableName, TableId, StorageId) - NO raw strings
5. **Namespace in Statements**: SQL statements contain namespace (e.g., `CREATE TABLE mydb.users`), fallback optional for convenience
6. **Parameter Binding**: execute_with_metadata() accepts `params: Vec<ParamValue>` for prepared statement support
7. **DataFusion Query Caching**: Leverage DataFusion 40.0+ built-in LogicalPlan/PhysicalPlan caching before building custom solution

**Type-Safe Identifier Architecture**:

All handlers MUST use type-safe wrappers defined in `kalamdb-commons`:
- **NamespaceId**: Wraps namespace identifier (e.g., "mydb", "system")
- **UserId**: Wraps user identifier (from ExecutionContext)
- **TableName**: Wraps table name (extracted from SQL statements)
- **TableId**: Composite identifier (namespace + table name, globally unique)
- **StorageId**: Wraps storage backend identifier

```rust
// Example handler signature (type-safe)
impl DdlHandler {
    pub async fn execute_create_table(
        session: &SessionContext,
        namespace_id: NamespaceId,        // Type-safe wrapper
        table_name: TableName,            // Type-safe wrapper
        statement: &Statement,
        execution_context: &ExecutionContext, // Contains UserId
    ) -> Result<ExecutionResult, KalamDbError> {
        // Extract user_id from context (type-safe)
        let user_id: UserId = execution_context.user_id();
        
        // Build table_id from namespace + table (type-safe)
        let table_id = TableId::new(namespace_id, table_name);
        
        // Use type-safe wrappers throughout
        let storage_id: StorageId = get_storage_for_namespace(&namespace_id)?;
        
        // ...
    }
}
```

**Benefits of Type-Safe Wrappers**:
- Compile-time prevention of mixing identifier types (e.g., passing user_id where table_name expected)
- Zero runtime overhead (wrappers optimized away by compiler)
- Clearer function signatures (self-documenting)
- Easier refactoring (compiler catches all usages)

**DataFusion Query Caching Research** (Phase 11 - P1):

Before implementing custom query cache, investigate DataFusion 40.0+ native caching:

1. **SessionContext Caching**: Does DataFusion cache LogicalPlan per session?
2. **Plan Reuse**: Can we reuse plans across different parameter values?
3. **Invalidation**: How does DataFusion invalidate plans when schemas change?
4. **Performance**: What speedup does DataFusion caching provide?
5. **Limitations**: What scenarios require custom caching layer?

If DataFusion caching is insufficient:
- Build normalized SQL cache (literals → placeholders)
- Map normalized SQL to Statement + parameter positions
- Let DataFusion handle LogicalPlan/PhysicalPlan caching
- Only cache SQL normalization layer

**Leveraging kalamdb-sql Crate Infrastructure** (Critical):

The `kalamdb-sql` crate already provides extensive SQL infrastructure:

1. **SqlStatement Classification** (`statement_classifier.rs`):
   - ✅ **Fast statement classification without full parse**
   - ✅ **30+ statement types already defined** (CreateTable, Insert, Select, FlushTable, Subscribe, etc.)
   - ✅ **is_datafusion_statement()** and **is_custom_command()** helpers
   - ✅ **100% test coverage** (30+ tests)
   - **Usage**: `let stmt_type = SqlStatement::classify(sql);`

2. **SQL Parser Infrastructure** (`parser/` module):
   - ✅ **Standard SQL via sqlparser-rs** (SELECT, INSERT, UPDATE, DELETE, CREATE TABLE)
   - ✅ **KalamDB extensions** (CREATE STORAGE, FLUSH TABLE, SUBSCRIBE, etc.)
   - ✅ **PostgreSQL/MySQL compatibility** (multiple syntax variants)
   - ✅ **Type-safe AST** (strongly typed statement representations)
   - **Usage**: `let statement = kalam_sql.parse_statement(sql)?;`

3. **Query Cache** (`query_cache.rs`):
   - ✅ **DashMap-based lock-free cache** (100× less contention than RwLock)
   - ✅ **TTL configuration per query type** (tables: 60s, jobs: 30s, storages: 5min)
   - ✅ **LRU eviction** (default 10,000 entries)
   - ✅ **Zero-copy results** (Arc<[u8]> for shared results)
   - **Usage**: System table queries already cached, can extend to user queries

4. **System Table Operations** (`adapter.rs`):
   - ✅ **CRUD operations** for all 7 system tables (users, jobs, namespaces, storages, live_queries, tables, audit_logs)
   - ✅ **Type-safe identifiers** (NamespaceId, UserId, StorageId, TableName)
   - ✅ **information_schema.tables** integration (TableDefinition CRUD)

**Refactoring Strategy Using kalamdb-sql**:

Instead of building everything from scratch, leverage existing infrastructure:

| Component | Current Location | Move To | Benefit |
|-----------|------------------|---------|---------|
| SqlStatement enum | ✅ kalamdb-sql::statement_classifier | **Keep in kalamdb-sql** | Already complete, tested, 30+ types |
| SQL Parser | ✅ kalamdb-sql::parser | **Keep in kalamdb-sql** | Standard + custom SQL, type-safe AST |
| Query Classification | ✅ kalamdb-sql::SqlStatement::classify() | **Keep in kalamdb-sql** | Fast, no full parse needed |
| Query Cache | ✅ kalamdb-sql::QueryCache | **Extend for user queries** | Lock-free, TTL, LRU already working |
| Authorization Logic | ❌ kalamdb-core::executor | **Move to handlers/authorization.rs** | Centralized security gateway |
| Handler Modules | ❌ kalamdb-core::executor (inline) | **Move to handlers/*.rs** | Modular, testable, maintainable |
| ExecutionContext | ❌ kalamdb-core::executor | **Move to handlers/types.rs** | Shared across all handlers |

**What Stays in kalamdb-sql**:
- SqlStatement classification (already perfect)
- SQL parsing infrastructure (standard + custom)
- Query caching (extend with user query support)
- System table operations (already complete)

**What Moves to kalamdb-core/handlers**:
- Authorization gateway (security-critical, needs RBAC logic)
- Execution handlers (DDL, DML, query, transaction, flush, etc.)
- ExecutionResult/ExecutionContext types (execution-specific)
- Handler orchestration (routing logic)

**Proposed Architecture**:

```rust
// ============================================================================
// SqlExecutor - Thin Coordinator (Stateless)
// ============================================================================

/// SQL execution coordinator with zero stored state
///
/// All dependencies fetched from AppContext on each request.
/// Leverages kalamdb-sql crate for parsing and classification.
/// 
/// Responsibilities:
/// 1. Classify statement type (fast) using kalamdb-sql::SqlStatement
/// 2. Parse SQL statement ONCE (full parse) using kalamdb-sql parser
/// 3. Extract namespace from statement OR use fallback
/// 4. Check authorization via AuthorizationHandler (gateway pattern)
/// 5. Route to appropriate handler module based on classification
/// 6. Return ExecutionResult
pub struct SqlExecutor;

impl SqlExecutor {
    /// Main entry point for SQL execution
    ///
    /// Flow:
    /// 1. Classify → SqlStatement enum (kalamdb-sql::statement_classifier - fast, no full parse)
    /// 2. Parse SQL → Statement (single pass via kalamdb-sql parser - full parse)
    /// 3. Extract namespace from statement OR use fallback (statements contain namespace)
    /// 4. Check auth → ExecutionContext (role, user_id, permissions)
    /// 5. Route → handler based on classification (no re-parsing)
    /// 6. Return → ExecutionResult
    ///
    /// # Parameters
    /// - `session`: DataFusion session context (for query execution)
    /// - `sql`: SQL statement string
    /// - `params`: Parameter values for parameterized queries (e.g., [123, "alice"] for SELECT * FROM users WHERE id = ? AND name = ?)
    /// - `fallback_namespace`: Optional namespace to use if statement doesn't specify one
    /// - `execution_context`: User authentication/authorization context
    pub async fn execute_with_metadata(
        session: &SessionContext,
        sql: &str,
        params: Vec<ParamValue>,
        fallback_namespace: Option<NamespaceId>,
        execution_context: &ExecutionContext,
    ) -> Result<(ExecutionResult, ExecutionMetadata), KalamDbError> {
        // Step 1: Classify statement type (fast, no full parse)
        // Uses kalamdb-sql::statement_classifier::SqlStatement::classify()
        let stmt_type = SqlStatement::classify(sql); // Already exists in kalamdb-sql!
        
        // Step 2: Parse SQL statement ONCE (full parse)
        let ctx = AppContext::get();
        let kalam_sql = ctx.kalam_sql(); // kalamdb-sql::KalamSql instance
        let statement = kalam_sql.parse_statement(sql)?; // PARSE ONCE
        
        // Step 3: Extract namespace from statement (statements contain namespace)
        // e.g., "CREATE TABLE mydb.users (id INT)" → namespace = NamespaceId("mydb")
        //       "SELECT * FROM users" with fallback → use fallback_namespace
        let namespace_id = statement.extract_namespace()
            .or(fallback_namespace)
            .ok_or_else(|| KalamDbError::MissingNamespace("Statement must specify namespace or provide fallback"))?;
        
        // Step 4: Authorization gateway (fail-fast)
        AuthorizationHandler::check_authorization(
            &stmt_type,      // SqlStatement classification
            &statement,      // Parsed statement
            &namespace_id,
            execution_context,
        )?;
        
        // Step 5: Route to handler based on classification (no re-parsing)
        // All handlers use type-safe wrappers (NamespaceId, UserId, TableName, etc.)
        // Uses SqlStatement enum from kalamdb-sql crate
        let result = match stmt_type {
            SqlStatement::CreateTable => {
                // Extract type-safe identifiers from statement
                let table_name = TableName::from_statement(&statement)?;
                DdlHandler::execute_create_table(
                    session, 
                    namespace_id, 
                    table_name,
                    &statement, 
                    execution_context
                ).await?
            }
            SqlStatement::Insert => {
                let table_name = TableName::from_statement(&statement)?;
                DmlHandler::execute_insert(
                    session, 
                    namespace_id,
                    table_name,
                    &statement, 
                    params, // Parameter values for INSERT INTO users VALUES (?, ?)
                    execution_context
                ).await?
            }
            SqlStatement::Select => {
                QueryHandler::execute_query(
                    session, 
                    namespace_id,
                    &statement, 
                    params, // Parameter values for WHERE clauses
                    execution_context
                ).await?
            }
            SqlStatement::BeginTransaction => {
                TransactionHandler::execute_begin(
                    session, 
                    namespace_id,
                    &statement, 
                    execution_context
                ).await?
            }
            // Map all SqlStatement variants to handlers
            SqlStatement::CreateNamespace => NamespaceHandler::execute_create(...).await?,
            SqlStatement::AlterNamespace => NamespaceHandler::execute_alter(...).await?,
            SqlStatement::DropNamespace => NamespaceHandler::execute_drop(...).await?,
            SqlStatement::CreateStorage => StorageHandler::execute_create(...).await?,
            SqlStatement::FlushTable => FlushHandler::execute_flush_table(...).await?,
            SqlStatement::FlushAllTables => FlushHandler::execute_flush_all(...).await?,
            SqlStatement::CreateUser => UserManagementHandler::execute_create_user(...).await?,
            SqlStatement::Subscribe => SubscriptionHandler::execute_subscribe(...).await?,
            // ... all other SqlStatement variants from kalamdb-sql
            _ => {
                return Err(KalamDbError::UnsupportedStatement(stmt_type.name().to_string()));
            }
        };
        
        // Step 6: Return with metadata
        Ok((result, ExecutionMetadata { /* ... */ }))
    }
}

// ============================================================================
// Handler Modules (14 Total)
// ============================================================================

/// handlers/
/// ├── types.rs              - ExecutionResult, ExecutionContext, ExecutionMetadata
/// ├── authorization.rs      - check_authorization() gateway
/// ├── transaction.rs        - BEGIN, COMMIT, ROLLBACK
/// ├── ddl.rs               - CREATE, ALTER, DROP (tables, namespaces, storages)
/// ├── dml.rs               - INSERT, UPDATE, DELETE
/// ├── query.rs             - SELECT, DESCRIBE, SHOW
/// ├── flush.rs             - FLUSH TABLE
/// ├── subscription.rs      - LIVE SELECT
/// ├── user_management.rs   - CREATE USER, ALTER USER, DROP USER
/// ├── table_registry.rs    - REGISTER TABLE, UNREGISTER TABLE
/// ├── system_commands.rs   - VACUUM, OPTIMIZE, ANALYZE
/// ├── audit.rs             - Audit logging helpers
/// ├── helpers.rs           - Common utilities (table resolution, etc.)
/// └── mod.rs               - Module exports

// ============================================================================
// Future: Query Caching Architecture
// ============================================================================

/// Parameterized query cache leveraging DataFusion's built-in caching
///
/// **IMPORTANT**: DataFusion 40.0+ provides query plan caching via SessionContext.
/// We should investigate and use DataFusion's native caching before building custom solution.
///
/// DataFusion Caching Features (v40.0):
/// - LogicalPlan caching: Reuses parsed/optimized logical plans
/// - Physical plan caching: Reuses execution plans for same logical plan
/// - Session-level cache: Plans cached per SessionContext
/// - Automatic invalidation: Plans invalidated when underlying data changes
///
/// Custom Layer (if needed):
/// - SQL normalization: Convert literals to parameters for cache key
/// - Cross-session cache: Share plans across multiple sessions
/// - Schema-aware invalidation: Integrate with SchemaRegistry
///
/// Design (if DataFusion caching insufficient):
/// - Cache key: Normalized SQL with placeholders (e.g., "SELECT * FROM users WHERE id = ?")
/// - Cache value: Parsed Statement + parameter positions
/// - DataFusion handles plan caching internally
/// - Scope: Only DML + queries (NOT DDL, admin commands, transactions)
/// - Invalidation: On ALTER TABLE, DROP TABLE (via SchemaRegistry)
/// - Memory: LRU cache with configurable size (default 1000 queries)
///
/// Example:
/// ```
/// // Original queries
/// SELECT * FROM users WHERE id = 123
/// SELECT * FROM users WHERE id = 456
///
/// // Cached as single entry
/// SELECT * FROM users WHERE id = ?  (with parameter binding)
/// 
/// // DataFusion caches the execution plan
/// // Our layer normalizes SQL + binds parameters
/// ```
pub struct QueryCache {
    /// Normalized SQL → (Statement, Parameter Positions)
    /// DataFusion SessionContext handles plan caching internally
    cache: Arc<DashMap<String, Arc<CachedQuery>>>,
    
    /// LRU eviction policy
    lru: Arc<Mutex<LruCache<String, ()>>>,
    
    /// Max cache size (default 1000)
    max_size: usize,
    
    /// DataFusion session context (handles plan caching)
    /// Plans automatically cached and reused by DataFusion
    session_context: Arc<SessionContext>,
}

impl QueryCache {
    /// Get cached query by normalized SQL
    /// Returns normalized statement + param positions
    /// DataFusion handles plan caching when statement is executed
    pub fn get(&self, normalized_sql: &str) -> Option<Arc<CachedQuery>> {
        self.cache.get(normalized_sql).map(|v| Arc::clone(&v))
    }
    
    /// Insert query into cache (with LRU eviction)
    pub fn insert(&self, normalized_sql: String, query: Arc<CachedQuery>) {
        if self.cache.len() >= self.max_size {
            // Evict LRU entry
            if let Some((evicted_key, _)) = self.lru.lock().unwrap().pop_lru() {
                self.cache.remove(&evicted_key);
            }
        }
        self.cache.insert(normalized_sql.clone(), query);
        self.lru.lock().unwrap().put(normalized_sql, ());
    }
    
    /// Invalidate all queries for a table (on ALTER/DROP)
    /// DataFusion automatically invalidates plans when table schema changes
    pub fn invalidate_table(&self, table_id: &TableId) {
        self.cache.retain(|_, cached| {
            !cached.referenced_tables.contains(table_id)
        });
        // DataFusion's SessionContext invalidates affected plans automatically
    }
}

struct CachedQuery {
    /// Parsed statement with placeholders
    statement: Statement,
    
    /// Tables referenced by this query (for invalidation)
    /// Use type-safe TableId wrappers
    referenced_tables: Vec<TableId>,
    
    /// Parameter positions (for binding values)
    /// Maps placeholder positions to parameter indices
    /// e.g., "SELECT * FROM users WHERE id = ? AND name = ?" → [0, 1]
    parameters: Vec<ParameterPosition>,
    
    /// Note: DataFusion SessionContext caches LogicalPlan and PhysicalPlan internally
    /// We only cache normalized SQL + parameter mapping
}

/// Parameter value types for prepared statements
#[derive(Debug, Clone)]
pub enum ParamValue {
    Int(i32),
    BigInt(i64),
    Float(f32),
    Double(f64),
    Text(String),
    Boolean(bool),
    Null,
    // ... other KalamDataType variants
}

/// Parameter position in normalized SQL
#[derive(Debug, Clone)]
pub struct ParameterPosition {
    /// Placeholder index in SQL (0-based)
    position: usize,
    
    /// Expected data type (for validation)
    expected_type: KalamDataType,
}
```

**Acceptance Scenarios**:

#### Scenario 10.1: Single-Pass SQL Parsing
```
Given the SQL executor receives query: "SELECT * FROM mydb.users WHERE id = 123"
When execute_with_metadata() is called with sql and params
Then SQL is parsed exactly ONCE via kalam_sql.parse_statement()
And the parsed Statement is passed to all downstream handlers
And namespace is extracted from statement: NamespaceId("mydb")
And no handler re-parses the original SQL string
And execution completes in <1ms for simple queries
```

#### Scenario 10.2: Authorization Gateway (Fail-Fast)
```
Given user with role='user' executes "DROP TABLE system.users"
When execute_with_metadata() is called
Then statement is parsed to extract namespace: NamespaceId("system")
And AuthorizationHandler::check_authorization(statement, namespace_id, context) is called FIRST
And authorization fails with "Insufficient permissions" error
And no handler is invoked (fail-fast security)
And DdlHandler::execute_drop_table() is NEVER called
```

#### Scenario 10.3: Modular Handler Routing with Type Safety
```
Given SQL statements: 
  - "CREATE TABLE mydb.test (id INT)" 
  - "INSERT INTO mydb.test VALUES (?)" with params [1]
  - "SELECT * FROM mydb.test WHERE id = ?" with params [1]
When each statement is executed
Then CREATE TABLE extracts NamespaceId("mydb"), TableName("test"), routes to DdlHandler
And INSERT extracts NamespaceId("mydb"), TableName("test"), params [1], routes to DmlHandler
And SELECT extracts NamespaceId("mydb"), params [1], routes to QueryHandler
And all handlers use type-safe wrappers (NamespaceId, TableName, UserId, etc.)
And each handler is tested independently with unit tests
```

#### Scenario 10.4: Parameter Binding
```
Given parameterized query: "SELECT * FROM users WHERE id = ? AND name = ?"
And parameters: [ParamValue::Int(123), ParamValue::Text("alice")]
When execute_with_metadata() is called
Then QueryHandler receives statement + params
And parameters are bound to placeholders in order: [0→123, 1→"alice"]
And query executes with bound values
And same query with different params reuses cached plan (via DataFusion)
```

#### Scenario 10.5: Namespace Extraction from Statement
```
Given SQL: "CREATE TABLE mydb.users (id INT, name TEXT)"
When execute_with_metadata() is called without fallback_namespace
Then statement.extract_namespace() returns Some(NamespaceId("mydb"))
And DdlHandler receives NamespaceId("mydb") and TableName("users")

Given SQL: "SELECT * FROM users" (no explicit namespace)
And fallback_namespace = Some(NamespaceId("default"))
When execute_with_metadata() is called
Then statement.extract_namespace() returns None
And fallback_namespace NamespaceId("default") is used
And QueryHandler receives NamespaceId("default")

Given SQL: "SELECT * FROM users" (no explicit namespace)
And fallback_namespace = None
When execute_with_metadata() is called
Then error "Statement must specify namespace or provide fallback" is returned
```

#### Scenario 10.6: Memory Efficiency (Zero Allocations on Hot Path)
```
Given 1000 concurrent SELECT queries on same table
When all queries execute via QueryHandler
Then Arc<TableDefinition> is cloned (NOT duplicated) via SchemaCache
And Arc<UserTableShared> is reused from cache
And String allocations occur only for result rows (not metadata)
And memory usage remains constant (no leak/growth)
```

#### Scenario 10.7: DataFusion Query Plan Caching (Leveraging Built-in)
```
Given QueryCache is enabled and DataFusion SessionContext has plan caching
When "SELECT * FROM users WHERE id = ?" executes with params [123]
Then query is normalized to "SELECT * FROM users WHERE id = ?"
And DataFusion caches the LogicalPlan and PhysicalPlan internally
When "SELECT * FROM users WHERE id = ?" executes with params [456]
Then normalized SQL cache hit returns same Statement
And parameters [456] are bound to placeholders
And DataFusion reuses cached LogicalPlan (no re-optimization)
And execution is 10-100× faster than cold query (DataFusion's optimization)
```

#### Scenario 10.8: Cache Invalidation on Schema Change
```
Given QueryCache contains 50 cached queries referencing table 'users' (TableId)
When "ALTER TABLE mydb.users ADD COLUMN email VARCHAR" executes
Then SchemaRegistry invalidates TableId for 'users'
And QueryCache.invalidate_table(TableId("mydb.users")) is called
And all 50 cached normalized queries are evicted
And DataFusion SessionContext invalidates affected plans automatically
And next SELECT on 'users' triggers cache miss + re-parse + re-plan
```

#### Scenario 10.9: DDL Commands Bypass Query Cache
```
Given QueryCache is enabled
When "CREATE TABLE mydb.test (id INT)" executes
Then DdlHandler executes normally
And query is NOT added to QueryCache (DDL excluded)
When "FLUSH TABLE users" executes
Then FlushHandler executes normally
And query is NOT cached (admin command excluded)
```

#### Scenario 10.10: Type-Safe Identifiers Throughout
```
Given all handlers use type-safe wrappers
When DdlHandler::execute_create_table() is called
Then namespace parameter is NamespaceId (not &str)
And table_name parameter is TableName (not String)
When DmlHandler::execute_insert() is called
Then user_id in ExecutionContext is UserId (not String)
When QueryHandler resolves table reference
Then table_id is TableId (not String)
And all identifier types provide compile-time safety
```

**Implementation Plan**:

**Phase 1: Directory Structure & Type Extraction (P0)**
- Create `backend/crates/kalamdb-core/src/sql/executor/` directory
- Move `executor.rs` → `executor/mod.rs` (no logic changes)
- Extract to `executor/handlers/types.rs`: ExecutionResult, ExecutionContext, ExecutionMetadata, ParamValue enum
- Extract to `executor/handlers/mod.rs`: Module exports
- **Import SqlStatement** from kalamdb-sql crate: `use kalamdb_sql::statement_classifier::SqlStatement;`
- Update execute_with_metadata() signature: Add `params: Vec<ParamValue>`, change `namespace: &str` → `fallback_namespace: Option<NamespaceId>`
- **Validation**: Workspace compiles, all tests pass (no behavior change)

**Phase 2: Statement Classification Integration (P0)**
- Replace manual statement type detection with `SqlStatement::classify(sql)`
- Update routing logic to use SqlStatement enum (already has 30+ variants!)
- Remove duplicate classification code from executor
- **Validation**: All existing tests pass, classification correct

**Phase 3: Authorization Gateway (P0)**
- Extract to `executor/handlers/authorization.rs`: check_authorization(stmt_type, statement, namespace_id, context) function
- Modify `executor/mod.rs`: Call AuthorizationHandler::check_authorization() BEFORE routing
- Update authorization to receive SqlStatement + NamespaceId (type-safe wrappers)
- Add authorization tests: role permissions, system table access, fail-fast behavior
- **Validation**: All existing tests pass + 10 new authorization tests

**Phase 4: Statement Namespace Extraction (P0)**
- Add `extract_namespace()` method to parsed Statement
- Implement namespace extraction for all statement types (CREATE TABLE mydb.users → NamespaceId("mydb"))
- Update execute_with_metadata() to extract namespace from statement
- Fallback to fallback_namespace if statement doesn't specify
- Return error if both are None
- **Validation**: Namespace extraction tests pass for all statement types

**Phase 4: Transaction Handler (P0)**
- Extract to `executor/handlers/transaction.rs`: BEGIN, COMMIT, ROLLBACK handlers
- Update handler signatures: Use NamespaceId instead of &str
- Update `executor/mod.rs`: Route transaction statements to TransactionHandler
- Add transaction tests: isolation levels, rollback, error handling
- **Validation**: All transaction tests pass (integration + unit)

**Phase 5: DDL Handler (P0)**
- Extract to `executor/handlers/ddl.rs`: CREATE/ALTER/DROP handlers (tables, namespaces, storages)
- Update handler signatures: Use NamespaceId, TableName, UserId (type-safe wrappers)
- Update `executor/mod.rs`: Route DDL statements to DdlHandler
- Add DDL tests: schema creation, modification, deletion
- **Validation**: All DDL tests pass, schema operations correct

**Phase 6: DML Handler (P0)**
- Extract to `executor/handlers/dml.rs`: INSERT, UPDATE, DELETE handlers
- Update handler signatures: Use NamespaceId, TableName, params: Vec<ParamValue>
- Implement parameter binding in INSERT/UPDATE/DELETE handlers
- Update `executor/mod.rs`: Route DML statements to DmlHandler with params
- Add DML tests: data manipulation, parameter binding, defaults, constraints
- **Validation**: All DML tests pass, data integrity maintained

**Phase 7: Query Handler (P0)**
- Extract to `executor/handlers/query.rs`: SELECT, DESCRIBE, SHOW handlers
- Update handler signatures: Use NamespaceId, params: Vec<ParamValue>
- Implement parameter binding in WHERE clauses via DataFusion
- Update `executor/mod.rs`: Route query statements to QueryHandler with params
- Add query tests: filtering, joins, aggregations, parameter binding
- **Validation**: All query tests pass, results correct

**Phase 7: Query Handler (P0)**
- Extract to `executor/handlers/query.rs`: SELECT, DESCRIBE, SHOW handlers
- Update handler signatures: Use NamespaceId, params: Vec<ParamValue>
- Implement parameter binding in WHERE clauses via DataFusion
- Update `executor/mod.rs`: Route query statements to QueryHandler with params
- Add query tests: filtering, joins, aggregations, parameter binding
- **Validation**: All query tests pass, results correct

**Phase 8: Remaining Handlers (P1)**
- Extract to `executor/handlers/flush.rs`: FLUSH TABLE handler (uses TableName)
- Extract to `executor/handlers/subscription.rs`: LIVE SELECT handler (uses NamespaceId, params)
- Extract to `executor/handlers/user_management.rs`: CREATE/ALTER/DROP USER handlers (uses UserId)
- Extract to `executor/handlers/table_registry.rs`: REGISTER/UNREGISTER TABLE handlers (uses TableName, NamespaceId)
- Extract to `executor/handlers/system_commands.rs`: VACUUM, OPTIMIZE, ANALYZE handlers
- Extract to `executor/handlers/helpers.rs`: Common utilities (table resolution with TableId)
- Extract to `executor/handlers/audit.rs`: Audit logging helpers (uses UserId, TableId)
- **Validation**: All handlers tested independently, integration tests pass

**Phase 9: Single-Pass Parsing Optimization (P0)**
- Audit all handler methods: Ensure Statement is passed (not &str)
- Remove any redundant parse_statement() calls in handlers
- Verify namespace extraction happens once in execute_with_metadata()
- Add performance test: Verify single parse per query
- **Validation**: Performance test confirms 1 parse per execution

**Phase 10: Memory Profiling (P0)**
- Add memory benchmarks: Measure allocations in QueryHandler hot path
- Optimize Arc cloning: Use Arc::clone() instead of new allocations
- Verify String interning: Ensure system columns use interned strings
- Verify type-safe wrappers (NamespaceId, TableId, etc.) have zero overhead
- **Validation**: Benchmarks show <100 bytes allocated per simple query

**Phase 11: DataFusion Query Caching Investigation (P1)**
- Research DataFusion 40.0+ SessionContext query caching capabilities
- Test DataFusion's LogicalPlan and PhysicalPlan caching behavior
- Measure cache hit rates and performance improvements
- Document DataFusion caching features and limitations
- **Validation**: DataFusion caching documented, performance measured

**Phase 12: Query Cache Design (P2 - Future, if DataFusion insufficient)**
- Create `executor/query_cache.rs`: QueryCache struct with LRU eviction
- Design normalized SQL format: Replace literals with placeholders
- Implement parameter mapping: Track placeholder positions
- Integrate with DataFusion SessionContext for plan caching
- Implement cache invalidation: Integrate with SchemaRegistry
- Add configuration: query_cache_size, query_cache_enabled
- **Validation**: Cache hit rate >80% in benchmark, invalidation correct

**Testing Strategy**:

1. **Unit Tests**: Each handler module has dedicated test file
   - `handlers/tests/authorization_tests.rs`: 20+ authorization scenarios
   - `handlers/tests/transaction_tests.rs`: 15+ transaction scenarios
   - `handlers/tests/ddl_tests.rs`: 30+ DDL scenarios
   - `handlers/tests/dml_tests.rs`: 25+ DML scenarios
   - `handlers/tests/query_tests.rs`: 40+ query scenarios

2. **Integration Tests**: End-to-end SQL execution
   - `tests/sql_executor_integration_tests.rs`: Multi-statement workflows
   - `tests/sql_executor_performance_tests.rs`: Latency/memory benchmarks

3. **Performance Benchmarks**: Criterion.rs benchmarks
   - `benches/sql_parsing_bench.rs`: Verify single-pass parsing (target: <50μs)
   - `benches/query_execution_bench.rs`: Measure query latency (target: <1ms)
   - `benches/memory_usage_bench.rs`: Measure allocations (target: <100 bytes)

4. **Security Tests**: Authorization enforcement
   - `tests/sql_security_tests.rs`: Role-based access control
   - `tests/sql_injection_tests.rs`: SQL injection prevention (DataFusion)

**Success Metrics**:

- **Code Organization**: 4,956 lines in 1 file → ~300-500 lines per handler file (14 files)
- **Test Coverage**: 90%+ line coverage for all handler modules
- **Performance**: SELECT latency <1ms, INSERT latency <5ms (simple queries)
- **Memory**: <100 bytes allocated per simple query (Arc reuse)
- **Security**: 100% of authorization tests pass (fail-fast gateway)
- **Maintainability**: Each handler can be modified/tested independently
- **Future-Ready**: Query cache architecture designed (implementation in P2)

**Migration Impact**:

- **Files Modified**: 1 file deleted (`executor.rs`), 15+ files created (executor/ directory structure)
- **Breaking Changes**: None (internal refactoring only, public API unchanged)
- **Test Changes**: Existing tests remain unchanged, new handler unit tests added
- **Documentation**: Update architecture docs with handler module diagram

**Dependencies**:

- **User Story 8**: SchemaRegistry required for cache invalidation integration
- **AppContext**: All handlers use AppContext::get() for dependency injection
- **KalamSQL**: Parser must return Statement for single-pass parsing
- **DataFusion**: Query execution requires LogicalPlan for future caching

**Future Enhancements** (Post-P0):

1. **Query Cache (P2)**: Implement parameterized query caching for 10-100× speedup on repeated queries
2. **Prepared Statements (P2)**: Add SQL PREPARE/EXECUTE support for client-side caching
3. **Query Plan Cache (P2)**: Cache DataFusion LogicalPlan for complex queries (joins, aggregations)
4. **Parallel Execution (P3)**: Execute independent statements concurrently (batch inserts)
5. **Query Profiling (P3)**: Add EXPLAIN ANALYZE for execution plan analysis

---

## Dependencies

### Internal Dependencies

- **kalamdb-commons**: Central location for consolidated schema models, must be updated before other crates
- **kalamdb-store**: Must implement EntityStore<TableId, TableDefinition> and SystemTableStore infrastructure for table schema persistence
- **kalamdb-sql**: SQL parser must populate new schema models, ensure ordinal_position is assigned correctly, depends on commons updates
- **kalamdb-api**: REST API schema endpoints must return consolidated models from EntityStore
- **DataFusion Integration**: Table providers must consume KalamDataType, convert to Arrow schemas, and sort columns by ordinal_position for SELECT *

### External Dependencies

- **Arrow/Parquet Compatibility**: Type conversions must remain compatible with Apache Arrow 52.0 and Parquet 52.0
- **Serde Serialization**: Schema models must support serde serialization for storage and API responses
- **Bincode Serialization**: TableDefinition must support bincode for efficient EntityStore persistence
- **DashMap Crate**: Lock-free HashMap for schema caching (already in dependencies)
- **Once Cell Crate**: Thread-safe singleton initialization for AppContext (already in dependencies via tokio)

### Refactoring Sequence

**Note**: This is an unreleased version - breaking changes are acceptable, no backward compatibility needed. Clean slate approach.

1. **Phase 1**: Create new schema models in \`kalamdb-commons/src/models/schemas/\` (KalamDataType, TableDefinition, ColumnDefinition) with bincode/serde derives, comprehensive documentation
2. **Phase 2**: Create AppContext singleton in \`kalamdb-core/src/app_context.rs\` with all shared resources (KalamSql, stores, managers, services) and introduce \`SchemaRegistry\` layered over \`SchemaCache\`
3. **Phase 3**: Implement type conversion functions and caching infrastructure (memory-bounded DashMap), profile for memory efficiency

4. **Phase 4**: Refactor kalamdb-sql parser to use new models and assign ordinal_position correctly, write integration tests for Story 2

5. **Phase 5**: Update DataFusion table providers to sort columns by ordinal_position for SELECT *, verify column ordering tests

6. **Phase 6**: Refactor services to use AppContext instead of constructor injection (UserTableService, SharedTableService, StreamTableService, TableDeletionService)

7. **Phase 7**: Refactor SqlExecutor to use AppContext, eliminate builder pattern with 8+ with_*() methods

8. **Phase 8**: Refactor lifecycle.rs and ApplicationComponents to use AppContext, remove redundant Arc fields

9. **Phase 9**: Update all tests to use create_test_app_context() helper, remove duplicate test setup code

10. **Phase 10**: Fix all failing tests using new infrastructure, write integration tests for Stories 2-3

11. **Phase 11**: Implement schema caching with performance validation, write integration tests for Story 4

12. **Phase 12**: Memory profiling and leak detection (valgrind/heaptrack), verify ~70% Arc allocation reduction, documentation review, final cleanup

## Implementation Plan: User Story 8 - AppContext Consolidation

### Phase 1: AppContext Core Structure (Day 1 - 2 hours)

**Goal**: Create AppContext singleton with all shared resources

**Files to Create**:
- `backend/crates/kalamdb-core/src/app_context.rs` (~300 lines)

**Implementation**:
```rust
use std::sync::Arc;
use once_cell::sync::OnceCell;

/// Global application context (singleton)
///
/// Central repository for ALL shared application resources:
/// - Stores (user tables, shared tables, stream tables)
/// - Services (namespace, user table, shared table, stream table, deletion, backup, restore)
/// - Managers (jobs, live queries)
/// - Providers (all system table providers)
/// - Registries (storage)
/// - Caches (unified schema cache)
/// - Factories (DataFusion session factory)
/// - Schedulers (flush, stream eviction)
pub struct AppContext {
    // ============================================================================
    // CORE DATABASE INFRASTRUCTURE
    // ============================================================================
    
    /// KalamSQL adapter for system table access
    pub(crate) kalam_sql: Arc<KalamSql>,
    
    /// Generic storage backend (RocksDB implementation)
    pub(crate) storage_backend: Arc<dyn StorageBackend>,
    
    // ============================================================================
    // TABLE STORES (EntityStore-based)
    // ============================================================================
    
    /// User table store (per-user data isolation)
    pub(crate) user_table_store: Arc<UserTableStore>,
    
    /// Shared table store (global data across namespace)
    pub(crate) shared_table_store: Arc<SharedTableStore>,
    
    /// Stream table store (ephemeral event storage)
    pub(crate) stream_table_store: Arc<StreamTableStore>,
    
    // ============================================================================
    // DATAFUSION SESSION MANAGEMENT
    // ============================================================================
    
    /// DataFusion session factory (creates SessionContext with custom functions)
    pub(crate) session_factory: Arc<DataFusionSessionFactory>,
    
    /// Global base SessionContext with system schema registered
    /// Used as template for creating per-request sessions
    pub(crate) base_session_context: Arc<SessionContext>,
    
    // ============================================================================
    // SYSTEM TABLE PROVIDERS (EntityStore v2)
    // ============================================================================
    
    /// Users table provider (system.users)
    pub(crate) users_provider: Arc<UsersTableProvider>,
    
    /// Jobs table provider (system.jobs)
    pub(crate) jobs_provider: Arc<JobsTableProvider>,
    
    /// Namespaces table provider (system.namespaces)
    pub(crate) namespaces_provider: Arc<NamespacesTableProvider>,
    
    /// Storages table provider (system.storages)
    pub(crate) storages_provider: Arc<StoragesTableProvider>,
    
    /// Live queries table provider (system.live_queries)
    pub(crate) live_queries_provider: Arc<LiveQueriesTableProvider>,
    
    /// Tables table provider (system.tables)
    pub(crate) tables_provider: Arc<TablesTableProvider>,
    
    /// Audit logs table provider (system.audit_logs)
    pub(crate) audit_logs_provider: Arc<AuditLogsTableProvider>,
    
    /// Stats virtual table provider (system.stats)
    pub(crate) stats_provider: Arc<StatsTableProvider>,
    
    /// Information schema tables provider (information_schema.tables)
    pub(crate) info_schema_tables_provider: Arc<InformationSchemaTablesProvider>,
    
    /// Information schema columns provider (information_schema.columns)
    pub(crate) info_schema_columns_provider: Arc<InformationSchemaColumnsProvider>,
    
    // ============================================================================
    // SCHEMA MANAGEMENT
    // ============================================================================
    
    /// Table schema store (information_schema.tables EntityStore)
    pub(crate) schema_store: Arc<TableSchemaStore>,
    
    /// Unified schema cache (Phase 10: replaces TableCache + SchemaCache)
    pub(crate) unified_cache: Arc<SchemaCache>,
    
    // ============================================================================
    // MANAGERS
    // ============================================================================
    
    /// Job manager (background task execution)
    pub(crate) job_manager: Arc<dyn JobManager>,
    
    /// Live query manager (WebSocket subscriptions)
    pub(crate) live_query_manager: Arc<LiveQueryManager>,
    
    // ============================================================================
    // REGISTRIES
    // ============================================================================
    
    /// Storage registry (storage path template resolution)
    pub(crate) storage_registry: Arc<StorageRegistry>,
    
    // ============================================================================
    // SCHEDULERS (HTTP layer, not core DB)
    // ============================================================================
    // Note: FlushScheduler and StreamEvictionScheduler stay in ApplicationComponents
    // because they need to be stopped during shutdown (HTTP lifecycle management)
    
    // ============================================================================
    // CONFIGURATION
    // ============================================================================
    
    /// Node ID for distributed coordination
    pub(crate) node_id: NodeId,
    
    /// Default storage path
    pub(crate) default_storage_path: String,
}

static APP_CONTEXT: OnceCell<Arc<AppContext>> = OnceCell::new();

impl AppContext {
    /// Initialize global context (call once at startup)
    pub fn init(
        backend: Arc<dyn StorageBackend>,
        node_id: NodeId,
        config: AppContextConfig,
    ) -> Result<Arc<Self>, KalamDbError> {
        // Initialize KalamSql
        let kalam_sql = Arc::new(KalamSql::new(backend.clone())?);
        
        // Initialize stores (using KalamCore pattern)
        let core = KalamCore::new(backend.clone())?;
        
        // Initialize DataFusion session factory and base context
        let session_factory = Arc::new(DataFusionSessionFactory::new()?);
        let base_session = session_factory.create_session();
        
        // Register system schema with base context (used as template)
        let catalog = base_session.catalog("kalam")
            .ok_or_else(|| KalamDbError::Other("kalam catalog not found".into()))?;
        catalog.register_schema("system", config.system_schema.clone())?;
        
        // Initialize managers
        let job_manager = Arc::new(TokioJobManager::new());
        let live_query_manager = Arc::new(LiveQueryManager::new(
            kalam_sql.clone(),
            node_id.clone(),
            Some(core.user_table_store.clone()),
            Some(core.shared_table_store.clone()),
            Some(core.stream_table_store.clone()),
        ));
        
        // Initialize registry and cache
        let storage_registry = Arc::new(StorageRegistry::new(
            kalam_sql.clone(),
            config.default_storage_path.clone(),
        ));
        let unified_cache = Arc::new(SchemaCache::new(
            config.cache_size,
            Some(storage_registry.clone()),
        ));
        
        // Register ALL system tables and get providers
        let (
            users_provider,
            jobs_provider,
            namespaces_provider,
            storages_provider,
            live_queries_provider,
            tables_provider,
            audit_logs_provider,
            stats_provider,
            info_schema_tables_provider,
            info_schema_columns_provider,
            schema_store,
        ) = crate::system_table_registration::register_all_system_tables(
            &config.system_schema,
            backend.clone(),
        )?;
        
        let ctx = Arc::new(AppContext {
            // Core infrastructure
            kalam_sql,
            storage_backend: backend,
            
            // Table stores
            user_table_store: core.user_table_store,
            shared_table_store: core.shared_table_store,
            stream_table_store: core.stream_table_store,
            
            // DataFusion
            session_factory,
            base_session_context: Arc::new(base_session),
            
            // System table providers
            users_provider,
            jobs_provider,
            namespaces_provider,
            storages_provider,
            live_queries_provider,
            tables_provider,
            audit_logs_provider,
            stats_provider,
            info_schema_tables_provider,
            info_schema_columns_provider,
            
            // Schema management
            schema_store,
            unified_cache,
            
            // Managers
            job_manager,
            live_query_manager,
            
            // Registries
            storage_registry,
            
            // Configuration
            node_id,
            default_storage_path: config.default_storage_path,
        });
        
        APP_CONTEXT.set(ctx.clone())
            .map_err(|_| KalamDbError::Other("AppContext already initialized".into()))?;
        
        Ok(ctx)
    }
    
    /// Get global context (panics if not initialized)
    pub fn get() -> Arc<Self> {
        APP_CONTEXT.get()
            .expect("AppContext not initialized - call AppContext::init() first")
            .clone()
    }
    
    /// Try to get global context (returns None if not initialized)
    pub fn try_get() -> Option<Arc<Self>> {
        APP_CONTEXT.get().cloned()
    }
    
    // Getter methods (return cheap Arc clones)
    
    // Core infrastructure
    pub fn kalam_sql(&self) -> Arc<KalamSql> { self.kalam_sql.clone() }
    pub fn storage_backend(&self) -> Arc<dyn StorageBackend> { self.storage_backend.clone() }
    
    // Table stores
    pub fn user_table_store(&self) -> Arc<UserTableStore> { self.user_table_store.clone() }
    pub fn shared_table_store(&self) -> Arc<SharedTableStore> { self.shared_table_store.clone() }
    pub fn stream_table_store(&self) -> Arc<StreamTableStore> { self.stream_table_store.clone() }
    
    // DataFusion
    pub fn session_factory(&self) -> Arc<DataFusionSessionFactory> { self.session_factory.clone() }
    pub fn base_session_context(&self) -> Arc<SessionContext> { self.base_session_context.clone() }
    
    /// Create a new per-request SessionContext (clones base context with fresh runtime config)
    pub fn create_session(&self) -> Arc<SessionContext> {
        Arc::new(self.session_factory.create_session())
    }
    
    /// Create a session with user context (registers CURRENT_USER() function)
    pub fn create_session_for_user(&self, user_id: UserId, namespace_id: NamespaceId) -> (Arc<SessionContext>, KalamSessionState) {
        let (ctx, state) = self.session_factory.create_session_for_user(user_id, namespace_id);
        (Arc::new(ctx), state)
    }
    
    // System table providers
    pub fn users_provider(&self) -> Arc<UsersTableProvider> { self.users_provider.clone() }
    pub fn jobs_provider(&self) -> Arc<JobsTableProvider> { self.jobs_provider.clone() }
    pub fn namespaces_provider(&self) -> Arc<NamespacesTableProvider> { self.namespaces_provider.clone() }
    pub fn storages_provider(&self) -> Arc<StoragesTableProvider> { self.storages_provider.clone() }
    pub fn live_queries_provider(&self) -> Arc<LiveQueriesTableProvider> { self.live_queries_provider.clone() }
    pub fn tables_provider(&self) -> Arc<TablesTableProvider> { self.tables_provider.clone() }
    pub fn audit_logs_provider(&self) -> Arc<AuditLogsTableProvider> { self.audit_logs_provider.clone() }
    pub fn stats_provider(&self) -> Arc<StatsTableProvider> { self.stats_provider.clone() }
    pub fn info_schema_tables_provider(&self) -> Arc<InformationSchemaTablesProvider> { self.info_schema_tables_provider.clone() }
    pub fn info_schema_columns_provider(&self) -> Arc<InformationSchemaColumnsProvider> { self.info_schema_columns_provider.clone() }
    
    // Schema management
    pub fn schema_store(&self) -> Arc<TableSchemaStore> { self.schema_store.clone() }
    pub fn unified_cache(&self) -> Arc<SchemaCache> { self.unified_cache.clone() }
    
    // Managers
    pub fn job_manager(&self) -> Arc<dyn JobManager> { self.job_manager.clone() }
    pub fn live_query_manager(&self) -> Arc<LiveQueryManager> { self.live_query_manager.clone() }
    
    // Registries
    pub fn storage_registry(&self) -> Arc<StorageRegistry> { self.storage_registry.clone() }
    
    // Configuration
    pub fn node_id(&self) -> &NodeId { &self.node_id }
    pub fn default_storage_path(&self) -> &str { &self.default_storage_path }
}

/// Configuration for AppContext initialization
pub struct AppContextConfig {
    pub default_storage_path: String,
    pub cache_size: usize,
    pub system_schema: Arc<MemorySchemaProvider>,
}
```

**Tests**:
- `test_app_context_init_once()` - Verify singleton initialization
- `test_app_context_get_panics_if_not_init()` - Verify panic behavior
- `test_app_context_getters()` - Verify all getters return valid Arc
- `test_app_context_thread_safe()` - Verify concurrent access

**Deliverable**: AppContext compiles, tests pass

---

### Phase 2: Refactor Services (Day 1 - 3 hours)

**Goal**: Make services stateless, use AppContext for dependencies

**Files to Modify**:
- `backend/crates/kalamdb-core/src/services/user_table_service.rs`
- `backend/crates/kalamdb-core/src/services/shared_table_service.rs`
- `backend/crates/kalamdb-core/src/services/stream_table_service.rs`
- `backend/crates/kalamdb-core/src/services/table_deletion_service.rs`
- `backend/crates/kalamdb-core/src/services/namespace_service.rs`

**Before**:
```rust
pub struct UserTableService {
    kalam_sql: Arc<KalamSql>,
    user_table_store: Arc<UserTableStore>,
}

impl UserTableService {
    pub fn new(kalam_sql: Arc<KalamSql>, user_table_store: Arc<UserTableStore>) -> Self {
        Self { kalam_sql, user_table_store }
    }
}
```

**After**:
```rust
pub struct UserTableService;

impl UserTableService {
    pub fn new() -> Self {
        Self
    }
    
    pub fn create_table(&self, stmt: CreateTableStatement) -> Result<(), KalamDbError> {
        let ctx = AppContext::get();
        let kalam_sql = ctx.kalam_sql();
        let user_table_store = ctx.user_table_store();
        
        // Use kalam_sql and user_table_store without storing them
        // ... existing logic unchanged ...
    }
}
```

**Pattern**: For each service:
1. Remove all Arc<Store> and Arc<Manager> fields
2. Make `new()` take zero parameters
3. Call `AppContext::get()` at start of each method
4. Extract needed resources via getters

**Tests**: Update service tests to use AppContext:
```rust
fn create_test_service() -> UserTableService {
    UserTableService::new()  // No params needed!
}
```

**Deliverable**: All 5 services refactored, tests pass

---

### Phase 3: Refactor SqlExecutor (Day 2 - 4 hours)

**Goal**: Simplify SqlExecutor constructor, eliminate builder pattern

**Files to Modify**:
- `backend/crates/kalamdb-core/src/sql/executor.rs` (~5000 lines)

**Before** (current nightmare):
```rust
pub struct SqlExecutor {
    namespace_service: Arc<NamespaceService>,
    session_context: Arc<SessionContext>,
    user_table_service: Arc<UserTableService>,
    shared_table_service: Arc<SharedTableService>,
    stream_table_service: Arc<StreamTableService>,
    table_deletion_service: Option<Arc<TableDeletionService>>,
    user_table_store: Option<Arc<UserTableStore>>,
    shared_table_store: Option<Arc<SharedTableStore>>,
    stream_table_store: Option<Arc<StreamTableStore>>,
    kalam_sql: Option<Arc<KalamSql>>,
    storage_backend: Option<Arc<dyn StorageBackend>>,
    jobs_table_provider: Option<Arc<JobsTableProvider>>,
    users_table_provider: Option<Arc<UsersTableProvider>>,
    storage_registry: Option<Arc<StorageRegistry>>,
    job_manager: Option<Arc<dyn JobManager>>,
    live_query_manager: Option<Arc<LiveQueryManager>>,
    schema_store: Option<Arc<TableSchemaStore>>,
    unified_cache: Option<Arc<SchemaCache>>,
    // ... 17 fields total!
}
```

**After** (clean):
```rust
pub struct SqlExecutor {
    namespace_service: Arc<NamespaceService>,
    session_context: Arc<SessionContext>,
    session_factory: DataFusionSessionFactory,
    enforce_password_complexity: bool,  // Config-only field
}

impl SqlExecutor {
    pub fn new(
        namespace_service: Arc<NamespaceService>,
        session_context: Arc<SessionContext>,
    ) -> Self {
        Self {
            namespace_service,
            session_context,
            session_factory: DataFusionSessionFactory::default(),
            enforce_password_complexity: false,
        }
    }
    
    pub fn with_password_complexity(mut self, enforce: bool) -> Self {
        self.enforce_password_complexity = enforce;
        self
    }
    
    // All 8 other with_*() methods DELETED
    
    pub async fn execute(&self, sql: &str, exec_ctx: ExecutionContext) -> Result<ExecutionResult> {
        let ctx = AppContext::get();
        
        // Get resources as needed
        let kalam_sql = ctx.kalam_sql();
        let user_table_store = ctx.user_table_store();
        let job_manager = ctx.job_manager();
        // ... etc
        
        // Existing logic unchanged
    }
}
```

**Changes**:
1. Remove 13 Option<Arc<T>> fields
2. Remove 8 with_*() builder methods
3. Add AppContext::get() calls in execute() and other methods
4. Keep only NamespaceService and SessionContext (not available globally)

**Tests**: Update executor tests:
```rust
fn create_test_executor() -> SqlExecutor {
    let ns_service = Arc::new(NamespaceService::new(AppContext::get().kalam_sql()));
    let session = Arc::new(create_session());
    SqlExecutor::new(ns_service, session)  // Clean!
}
```

**Deliverable**: SqlExecutor simplified, 477 tests still pass

---

### Phase 3A: Stateless SqlExecutor (memory-focused revision)

To further reduce per-request memory and ensure no accidental session retention inside the executor, adopt a stateless executor pattern.

Key changes:
- Remove SessionContext field from SqlExecutor; do not store per-request state inside the executor.
- All methods accept a reference to the caller-provided SessionContext.
- Keep SqlExecutor as a lightweight, shareable service (one Arc in AppContext or constructed on-demand).

Revised API (illustrative):
```rust
pub struct SqlExecutor;

impl SqlExecutor {
    pub fn new() -> Self { SqlExecutor }

    pub async fn execute(
        &self,
        session: &SessionContext,
        sql: &str,
        exec_ctx: ExecutionContext,
    ) -> Result<ExecutionResult> {
        let ctx = AppContext::get();
        // Fetch shared dependencies on-demand
        let _kalam_sql = ctx.kalam_sql();
        let _user_store = ctx.user_table_store();
        let _job_mgr = ctx.job_manager();
        // ... existing logic unchanged ...
        todo!()
    }
}
```

Per-request footprint:
- Only the SessionContext and minimal per-request provider wrappers (UserTableAccess) are allocated.
- SqlExecutor is shared and stateless → zero growth per request.

Rationale:
- We already require a per-request SessionContext for CURRENT_USER-bound UDFs and RLS.
- Stateless executor prevents cross-request state leaks and cuts memory.

Adoption:
- Route handlers/tests create a per-request SessionContext via SessionFactory and pass it to SqlExecutor::execute().

---

### Phase 4: Refactor lifecycle.rs (Day 2 - 2 hours)

**Goal**: Use AppContext in bootstrap, simplify ApplicationComponents

**Files to Modify**:
- `backend/src/lifecycle.rs` (~500 lines)

**Before**:
```rust
pub struct ApplicationComponents {
    pub session_factory: Arc<DataFusionSessionFactory>,
    pub sql_executor: Arc<SqlExecutor>,
    pub jwt_auth: Arc<JwtAuth>,
    pub rate_limiter: Arc<RateLimiter>,
    pub flush_scheduler: Arc<FlushScheduler>,
    pub live_query_manager: Arc<LiveQueryManager>,
    pub stream_eviction_scheduler: Arc<StreamEvictionScheduler>,
    pub rocks_db_adapter: Arc<RocksDbAdapter>,
}

pub async fn bootstrap(config: &ServerConfig) -> Result<ApplicationComponents> {
    // 200+ lines of initialization
    let kalam_sql = Arc::new(KalamSql::new(backend.clone())?);
    let core = KalamCore::new(backend.clone())?;
    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    // ... 50 more lines ...
}
```

**After**:
```rust
pub struct ApplicationComponents {
    // Only HTTP-layer state (not core database state)
    pub jwt_auth: Arc<JwtAuth>,
    pub rate_limiter: Arc<RateLimiter>,
    pub flush_scheduler: Arc<FlushScheduler>,
    pub stream_eviction_scheduler: Arc<StreamEvictionScheduler>,
}

pub async fn bootstrap(config: &ServerConfig) -> Result<ApplicationComponents> {
    // Initialize RocksDB
    let db = init_rocksdb(&config)?;
    let backend = Arc::new(RocksDBBackend::new(db));
    
    // Create DataFusion session for system table registration
    let session_factory = DataFusionSessionFactory::new()?;
    let session = session_factory.create_session();
    let system_schema = Arc::new(MemorySchemaProvider::new());
    
    // Register system schema with DataFusion
    let catalog = session.catalog("kalam").expect("kalam catalog");
    catalog.register_schema("system", system_schema.clone())?;
    
    // Initialize AppContext (ONE call, all resources)
    let node_id = NodeId::new(config.server.node_id.clone());
    let ctx = AppContext::init(
        backend,
        node_id,
        AppContextConfig {
            default_storage_path: config.storage.default_storage_path.clone(),
            cache_size: 10000,
            system_schema,
        },
    )?;
    
    // Seed default storage (uses AppContext)
    seed_default_storage(config)?;
    
    // Create HTTP-layer components
    let jwt_auth = Arc::new(JwtAuth::new(config.auth.jwt_secret.clone(), Algorithm::HS256));
    let rate_limiter = Arc::new(RateLimiter::with_config(/* ... */));
    let flush_scheduler = Arc::new(FlushScheduler::new(
        ctx.job_manager(),
        Duration::from_secs(5),
    ).with_jobs_provider(ctx.jobs_provider()));
    
    let stream_eviction_scheduler = Arc::new(StreamEvictionScheduler::new(/* ... */));
    
    // Start schedulers
    flush_scheduler.start().await?;
    stream_eviction_scheduler.start().await?;
    
    Ok(ApplicationComponents {
        jwt_auth,
        rate_limiter,
        flush_scheduler,
        stream_eviction_scheduler,
    })
}
```

**Changes**:
1. Remove session_factory, sql_executor, live_query_manager, rocks_db_adapter from ApplicationComponents
2. Initialize AppContext with single call
3. Create SqlExecutor in route handlers on-demand (it's now cheap!)
4. Simplify bootstrap by ~100 lines

**Deliverable**: Server starts, all HTTP endpoints work

---

### Phase 5: Refactor Tests (Day 3 - 3 hours)

**Goal**: Create test utility, update all tests

**Files to Create**:
- `backend/crates/kalamdb-core/src/test_utils.rs` (~150 lines)

**Implementation**:
```rust
use once_cell::sync::OnceCell;

static TEST_CONTEXT: OnceCell<Arc<AppContext>> = OnceCell::new();

/// Create or get test AppContext (singleton per test process)
pub fn create_test_app_context() -> Arc<AppContext> {
    TEST_CONTEXT.get_or_init(|| {
        let test_db = TestDb::new(&[
            "system_users",
            "system_tables",
            "information_schema_tables",
            "user_tables",
            "shared_tables",
            "stream_tables",
        ]).expect("Failed to create test DB");
        
        let backend = Arc::new(RocksDBBackend::new(test_db.db));
        let session_factory = DataFusionSessionFactory::new().unwrap();
        let session = session_factory.create_session();
        let system_schema = Arc::new(MemorySchemaProvider::new());
        
        session.catalog("kalam").unwrap()
            .register_schema("system", system_schema.clone()).unwrap();
        
        AppContext::init(
            backend,
            NodeId::new("test-node"),
            AppContextConfig {
                default_storage_path: "/tmp/test".into(),
                cache_size: 1000,
                system_schema,
            },
        ).expect("Failed to init test AppContext")
    }).clone()
}

/// Reset test context (for tests that need clean state)
pub fn reset_test_context() {
    // Note: Can't actually reset OnceCell, so tests must be idempotent
    // or use unique identifiers (namespaces, table names)
}
```

**Files to Update** (~30 test files):
- `backend/crates/kalamdb-core/src/services/user_table_service.rs`
- `backend/crates/kalamdb-core/src/services/shared_table_service.rs`
- `backend/crates/kalamdb-core/src/services/stream_table_service.rs`
- `backend/crates/kalamdb-core/src/sql/executor.rs`
- `backend/tests/*.rs` (all integration tests)

**Pattern**:
```rust
// Before (50 lines):
fn create_test_service() -> (UserTableService, TestDb) {
    let test_db = TestDb::new(&["system_tables", "user_tables"]).unwrap();
    let backend = Arc::new(RocksDBBackend::new(test_db.db.clone()));
    let kalam_sql = Arc::new(KalamSql::new(backend.clone()).unwrap());
    let user_table_store = Arc::new(UserTableStore::new(backend, "user_tables"));
    let service = UserTableService::new(kalam_sql, user_table_store);
    (service, test_db)
}

// After (2 lines):
fn create_test_service() -> UserTableService {
    create_test_app_context();  // Ensure initialized
    UserTableService::new()
}
```

**Deliverable**: All tests updated, pass with new pattern

---

### Phase 6: Update Route Handlers (Day 3 - 2 hours)

**Goal**: Simplify route handler initialization

**Files to Modify**:
- `backend/src/routes.rs`
- `backend/crates/kalamdb-api/src/routes/sql.rs`
- `backend/crates/kalamdb-api/src/routes/ws.rs`

**Before**:
```rust
async fn execute_sql(
    sql_executor: web::Data<Arc<SqlExecutor>>,
    // ... other params
) -> Result<HttpResponse> {
    // Use injected sql_executor
}
```

**After**:
```rust
async fn execute_sql(
    // No sql_executor injection needed!
    // ... other params
) -> Result<HttpResponse> {
    let ctx = AppContext::get();
    
    // Create SqlExecutor on demand (cheap now!)
    let ns_service = Arc::new(NamespaceService::new(ctx.kalam_sql()));
    let session_factory = DataFusionSessionFactory::new()?;
    let session = Arc::new(session_factory.create_session());
    let sql_executor = SqlExecutor::new(ns_service, session);
    
    // Or even better: make SqlExecutor a method on AppContext
    let sql_executor = ctx.create_sql_executor()?;
}
```

**Alternative** (add to AppContext):
```rust
impl AppContext {
    pub fn create_sql_executor(&self) -> Result<SqlExecutor> {
        let ns_service = Arc::new(NamespaceService::new(self.kalam_sql()));
        let session_factory = DataFusionSessionFactory::new()?;
        let session = Arc::new(session_factory.create_session());
        Ok(SqlExecutor::new(ns_service, session))
    }
}
```

**Deliverable**: Route handlers simplified, server works

---

### Phase 7: Documentation & Cleanup (Day 4 - 2 hours)

**Goal**: Document pattern, clean up old code

**Tasks**:
1. Add comprehensive docs to `app_context.rs` (examples, thread safety notes)
2. Update AGENTS.md with AppContext pattern
3. Delete KalamCore struct (merged into AppContext)
4. Delete ApplicationComponents old fields
5. Run clippy, fix all warnings
6. Run memory profiler, verify Arc reduction

**Documentation**:
```rust
/// # AppContext - Global Application State Singleton
///
/// AppContext provides centralized access to all shared application resources:
/// - Database stores (user tables, shared tables, stream tables)
/// - Managers (jobs, live queries)
/// - Caches (unified schema cache)
/// - System table providers
///
/// # Thread Safety
/// AppContext uses OnceCell for thread-safe singleton initialization.
/// Multiple threads can call `get()` concurrently - all receive the same instance.
///
/// # Usage Pattern
/// ```
/// // In main.rs or lifecycle.rs (called once):
/// let ctx = AppContext::init(backend, node_id, config)?;
///
/// // Anywhere else in the code:
/// let ctx = AppContext::get();
/// let store = ctx.user_table_store();  // Arc<UserTableStore>
/// ```
///
/// # Memory Efficiency
/// Before AppContext: 3× Arc<UserTableStore> per request (SqlExecutor + Service + Handler)
/// After AppContext: 1× Arc<UserTableStore> shared globally (70% reduction)
```

**Deliverable**: Docs complete, old code removed

---

### Testing Strategy

**Unit Tests** (per phase):
- Phase 1: AppContext initialization, getters, thread safety
- Phase 2: Services work with AppContext
- Phase 3: SqlExecutor simplified constructor
- Phase 5: Test utilities work correctly

**Integration Tests** (end-to-end):
- All existing backend tests pass (477 tests)
- All CLI tests pass
- All WASM SDK tests pass
- Server starts and accepts requests
- Concurrent request handling works

**Performance Tests**:
- Memory profiling: Verify ~70% Arc reduction
- Benchmark: AppContext::get() latency (<1μs)
- Load test: 1000 concurrent requests, stable memory

**Rollback Plan**:
If critical bugs found:
1. Each phase is a separate commit
2. Can revert to previous phase
3. AppContext is additive - doesn't break existing code until services are refactored

---

### Success Metrics

**Code Metrics**:
- [ ] Lines removed: ~1,200 (builder methods, Arc fields, test boilerplate)
- [ ] Lines added: ~400 (AppContext + docs)
- [ ] Net reduction: ~800 lines
- [ ] Files modified: ~40
- [ ] Constructor params reduced: 10+ params → 0-2 params (>80% reduction)

**Performance Metrics**:
- [ ] Arc allocations: 70% reduction (11,000 → 3,300 for 1000 requests)
- [ ] AppContext::get() latency: <1μs
- [ ] Memory usage: Stable under load (no leaks)
- [ ] Test suite time: <5% slower (AppContext initialization overhead)

**Quality Metrics**:
- [ ] All 477 kalamdb-core tests pass
- [ ] All CLI tests pass
- [ ] All WASM SDK tests pass
- [ ] Zero clippy warnings
- [ ] Zero unsafe code in AppContext

---

---

### Recommended Execution Order

Given the interdependencies, here's the optimal execution sequence:

**Week 1: Foundation**
- Day 1-2: **User Story 8 Phase 1-2** (AppContext + Services + SchemaRegistry) - Foundation for everything
- Day 3-4: **User Story 8 Phase 3-4** (SqlExecutor + lifecycle.rs) - Core architecture

**Week 2: Integration & Testing**  
- Day 1-2: **User Story 8 Phase 5-7** (Tests + Documentation) - Stabilize AppContext
- Day 3-4: **User Story 1** (Schema Consolidation) - Build on stable foundation
- Day 5: **User Story 2** (Unified Data Types) - Parallel with US1

**Week 3: Quality & Performance**
- Day 1-2: **User Story 3** (Test Suite Completion) - Fix all tests
- Day 3: **User Story 4** (Schema Caching) - Performance optimization
- Day 4: **User Story 7** (Cache Consolidation) - Final memory optimization
- Day 5: **User Story 5** (P0 Datatypes) - Critical missing types

**Week 4: Validation**
- Day 1-2: **User Story 6** (CLI Smoke Tests) - End-to-end validation
- Day 3-4: Integration testing, performance profiling, documentation
- Day 5: Alpha release preparation

**Why This Order**:
1. AppContext first eliminates the "wiring hell" that would make subsequent refactoring painful
2. Schema consolidation builds on stable AppContext foundation
3. Data types integrate with consolidated schemas
4. Tests validate everything works together
5. Caching optimizes the validated system
6. Smoke tests prove production readiness

**Parallel Work Opportunities**:
- User Story 2 (Data Types) can be developed in parallel with User Story 1 (Schemas)
- User Story 5 (P0 Datatypes) can be prototyped while waiting for schema consolidation
- Documentation can be written incrementally throughout

---

## Notes

### Implementation Priorities

The three main stories have overlapping P1 priority because they are interdependent:
- **Schema Consolidation** provides the foundation for all schema-related code
- **Unified Data Types** eliminates type conversion bugs that affect tests
- **Test Fixing** validates that consolidation and type system work correctly

Recommended implementation order:
1. Start with Schema Consolidation (Story 1) to establish new models with ordinal_position field
2. Implement EntityStore<TableId, TableDefinition> for schema persistence (Phase 14 pattern)
3. Implement Unified Data Type System (Story 2) in parallel since ColumnDefinition depends on KalamDataType
4. Update SQL parser and DataFusion providers to use ordinal_position for SELECT * column ordering
5. Incrementally fix tests (Story 3) as each refactoring phase completes
6. Add performance caching (Story 4) after core functionality is stable

### Design Decisions

**Schema History Embedded in TableDefinition - Why This is Optimal:**

The decision to embed `Vec<SchemaVersion>` in `TableDefinition` rather than use separate storage is the best approach for Alpha because:

1. **Simplicity**: Atomic updates - updating schema and history happens in single EntityStore write, no multi-table coordination
2. **Performance**: Most queries need current schema only, not history. Loading history on-demand from separate storage adds latency for rare use case
3. **Consistency**: Schema and history always in sync, no possibility of orphaned history records or missing versions
4. **Storage Efficiency**: Bincode serialization keeps overhead minimal. For typical workloads (<50 schema versions), embedded storage is negligible
5. **Query Pattern**: Schema history queries are rare (debugging, auditing), optimizing for common case (current schema) is correct
6. **Future Flexibility**: If history grows large (>50 versions), can implement cold storage migration without changing API surface

**Alternative Considered**: Separate `EntityStore<SchemaVersionId, SchemaVersion>` table
- **Rejected because**: Adds complexity (2 tables to update), worse performance for common case (current schema), potential consistency issues
- **When it might be better**: If querying historical schemas becomes frequent, or if history size exceeds 100 versions per table regularly

**Migration Strategy**: Not applicable - unreleased version, no backward compatibility needed. Clean break from old code.

### Risk Mitigation

- **Breaking Changes**: Clean slate approach - no backward compatibility needed, wipe dev databases and start fresh
- **Test Failures**: Fix tests incrementally per crate (commons → store → sql → core → api) to isolate issues
- **Performance Regression**: Benchmark schema queries before/after caching to validate performance improvements
- **Memory Leaks**: Run valgrind/heaptrack on test suite and long-running scenarios, ensure stable memory usage
- **Cache Bugs**: Implement cache with disabled mode (passthrough) to quickly diagnose cache-related issues
- **Column Ordering Bugs**: Add integration tests that verify SELECT * column order matches ordinal_position order
- **Schema History Growth**: Monitor schema_history size in tests, add warning if exceeds 50 versions (consider cold storage)
- **EMBEDDING Dimension Limits**: Enforce reasonable maximum dimension (8192) to prevent memory exhaustion attacks
- **Vector Embedding Memory**: Monitor memory usage for tables with EMBEDDING columns, large dimensions (3072+) can consume significant space

### Use Cases

**Vector Embeddings for ML/AI Workloads:**

EMBEDDING type enables KalamDB to store and query vector embeddings for semantic search, recommendation systems, and AI applications:

- **Sentence Embeddings**: EMBEDDING(384) or EMBEDDING(768) for sentence-transformers models (all-MiniLM-L6-v2, all-mpnet-base-v2)
- **OpenAI Embeddings**: EMBEDDING(1536) for text-embedding-ada-002, EMBEDDING(3072) for text-embedding-3-large
- **Document Search**: Store document embeddings alongside metadata for semantic search without external vector database
- **Image Embeddings**: EMBEDDING(512) for CLIP or ResNet embeddings
- **Product Recommendations**: Store item embeddings for similarity-based recommendations

Example schema:
```sql
CREATE TABLE documents (
  id INT PRIMARY KEY,
  title TEXT,
  content TEXT,
  embedding EMBEDDING(1536),  -- OpenAI ada-002 embeddings
  created_at TIMESTAMP
);
```

This positions KalamDB as a unified database for structured data + vector embeddings, eliminating need for separate vector databases (Pinecone, Weaviate, etc.) in many use cases.

### Success Validation

Alpha release readiness checklist:

**User Story 1 - Schema Consolidation:**
- [ ] Integration tests verify identical schemas from DESCRIBE, information_schema.columns, and internal APIs
- [ ] All system table schemas defined in kalamdb-commons/src/models/schemas/
- [ ] Schema cache performance: sub-100μs repeated lookups
- [ ] Schema history preserved across ALTER TABLE operations

**User Story 2 - Unified Data Types:**
- [ ] Integration tests verify all 13 KalamDataType conversions to/from Arrow are lossless (including FLOAT, EMBEDDING)
- [ ] EMBEDDING type correctly maps to Arrow FixedSizeList<Float32> with dimension parameter
- [ ] Integration tests validate EMBEDDING with common dimensions (384, 768, 1536, 3072)
- [ ] Type conversion cache shows <10μs lookup times
- [ ] SELECT * queries return columns in ordinal_position order (integration test validates)
- [ ] All codebase type conversions use KalamDataType (verified via grep)
- [ ] Parameterized type pattern (associated data in enum) documented with examples for future extensions

**User Story 3 - Test Suite Completion:**
- [ ] All backend tests pass (cargo test in backend/)
- [ ] All CLI tests pass (cargo test in cli/)
- [ ] All SDK tests pass (link/sdks/typescript/tests/)
- [ ] Integration tests exist for each user story

**User Story 4 - Schema Caching:**
- [ ] Cache hit rate >99% in benchmark (10,000 schema queries)
- [ ] Cache invalidation works correctly on ALTER TABLE
- [ ] Concurrent schema reads show no contention

**Code Quality:**
- [ ] Zero clippy warnings in new code
- [ ] All public APIs have documentation with examples
- [ ] Memory profiling shows zero leaks (valgrind/heaptrack clean)
- [ ] Stable memory usage under sustained load (1 hour test)

**General:**
- [ ] Schema queries use only consolidated models (verified via code search)
- [ ] All old model code removed (no deprecation warnings, clean break)
- [ ] All table schemas persisted via EntityStore (verified via storage inspection)
- [ ] Schema history embedded in TableDefinition (no separate storage)
- [ ] Performance benchmarks show 10× improvement in schema queries
