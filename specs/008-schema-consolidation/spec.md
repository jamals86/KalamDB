# Feature Specification: Schema Consolidation & Unified Data Type System

**Feature Branch**: `008-schema-consolidation`  
**Created**: November 1, 2025  
**Status**: Draft  
**Input**: User description: "Schema consolidation and unified data type system with comprehensive test fixing for Alpha release readiness"

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

### Refactoring Sequence

**Note**: This is an unreleased version - breaking changes are acceptable, no backward compatibility needed. Clean slate approach.

1. **Phase 1**: Create new schema models in \`kalamdb-commons/src/models/schemas/\` (KalamDataType, TableDefinition, ColumnDefinition) with bincode/serde derives, comprehensive documentation
3. **Phase 3**: Implement type conversion functions and caching infrastructure (memory-bounded DashMap), profile for memory efficiency

4. **Phase 4**: Refactor kalamdb-sql parser to use new models and assign ordinal_position correctly, write integration tests for Story 2

5. **Phase 5**: Update DataFusion table providers to sort columns by ordinal_position for SELECT *, verify column ordering tests

6. **Phase 6**: Refactor kalamdb-core and kalamdb-api to consume schemas from EntityStore, remove ALL old model code immediately (no gradual deprecation)

7. **Phase 7**: Fix all failing tests using new infrastructure, write integration tests for Story 3

8. **Phase 8**: Implement schema caching with performance validation, write integration tests for Story 4

9. **Phase 9**: Memory profiling and leak detection (valgrind/heaptrack), documentation review, final cleanup

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
