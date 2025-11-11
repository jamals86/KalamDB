# Feature Specification: Full DML Support (MVCC Architecture)

**Feature Branch**: `012-full-dml-support`  
**Created**: 2025-11-10  
**Updated**: 2025-11-11 (MVCC Architecture Redesign)  
**Status**: Draft  

## ⚠️ CRITICAL ARCHITECTURE CHANGE (2025-11-11)

**Original Design**: Append-only storage with `_updated` timestamps and storage layer prioritization (fast storage > Parquet)

**New Design (MVCC)**: Multi-Version Concurrency Control with `_seq` Snowflake IDs and unified DML functions

### Key Changes:

1. **Mandatory Primary Key**: Users MUST supply a primary key column (any name: `id`, `email`, `order_id`, etc.) when creating tables
2. **SeqId Type**: New `SeqId` wrapper around Snowflake ID with timestamp extraction capabilities (replaces raw i64 `_seq`)
3. **System Columns**: KalamDB auto-adds `_seq: SeqId` (Snowflake ID for versioning) and `_deleted: bool` (soft delete flag)
4. **Storage Key Formats**:
   - **UserTableRowId**: `{user_id}:{_seq}` (no table_id - already in column family name)
   - **SharedTableRowId**: Just `{_seq}` (SeqId directly, no wrapper needed)
5. **Row Structures**:
   - **UserTableRow**: `user_id: UserId`, `_seq: SeqId`, `_deleted: bool`, `fields: JsonValue` (user PK + data in fields)
   - **SharedTableRow**: `_seq: SeqId`, `_deleted: bool`, `fields: JsonValue` (no access_level per row - cached in schema)
6. **Append-Only Writes**: INSERT/UPDATE/DELETE ALL append new versions with incremented `_seq` - ZERO in-place updates in hot storage
7. **Version Resolution**: SELECT queries use MAX(`_seq`) per PK with `_deleted = false` filtering
8. **Snapshot Deduplication**: FLUSH operations write only latest versions (MAX(`_seq`) per PK) to Parquet files
9. **Unified DML**: User tables and shared tables use IDENTICAL insertion/update/deletion functions (50%+ code reduction, zero duplicate logic)
10. **Incremental Sync**: `SELECT * WHERE _seq > {last_synced}` retrieves all changes after specified version threshold

### Benefits:

- **Zero-locking concurrency**: Writers never block readers (true append-only architecture)
- **Simplified flush logic**: Snapshot deduplication uses simple MAX(`_seq`) grouping
- **Code consolidation**: User/shared tables share all DML functions (eliminates duplicate code paths)
- **Point-in-time queries**: Foundation for future time-travel queries via `_seq` ranges
- **Efficient sync**: Incremental sync via `_seq > X` without complex timestamp handling
- **Type-safe SeqId**: Wrapper prevents mixing raw i64s with sequence IDs, provides timestamp extraction
- **Efficient scans**: RocksDB prefix scanning by `user_id:` or range scanning by `_seq` threshold
- **Minimal row overhead**: UserTableRow/SharedTableRow contain only essential fields (_seq, _deleted, fields)

### Migration Impact:

- **Existing tables**: Migration strategy required (add `_seq: SeqId` column, rewrite storage keys to `{user_id}:{_seq}` format)
- **Existing code**: UserTableRow/SharedTableRow structures must change:
  - Remove: `row_id: String`, `user_id: String`, `_updated: String`, `access_level: TableAccess`
  - Add: `_seq: SeqId`, `_deleted: bool`, `fields: JsonValue` (minimal structure)
- **Storage keys**: UserTableRowId becomes `{user_id}:{_seq}`, SharedTableRowId becomes just `SeqId` (no wrapper)
- **Phase 2 tasks**: SeqId type creation, unified DML module, storage key refactoring, query resolution changes

**Input**: User description: "Support Update/Delete of flushed tables with manifest files, AS USER support for DML statements, centralized config usage via AppContext, and generic JobExecutor parameters" + **MVCC architecture redesign with `_seq` versioning and unified DML functions**

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Update and Delete Persisted Table Records (Priority: P1)

A database administrator needs to update or delete specific records from a user table, including records that have been persisted to long-term storage. The system uses an append-only architecture where updates create new versions and queries automatically retrieve the latest version using timestamp-based resolution.

**Why this priority**: Core DML functionality - without UPDATE/DELETE on persisted tables, users cannot modify historical data, making the database read-only for persisted records. This is essential for data correction, GDPR compliance (right to erasure), and normal database operations.

**Independent Test**: Can be fully tested by creating a user table, inserting records, persisting to long-term storage, then executing UPDATE/DELETE statements. Success is verified by querying the table and confirming only the latest version of each record is returned based on timestamp ordering.

**Acceptance Scenarios**:

1. **Given** a record exists only in fast storage, **When** user executes `UPDATE table SET column = value WHERE id = X`, **Then** the system updates the record in fast storage and sets `_updated` to current timestamp with nanosecond precision
2. **Given** a record has been persisted to long-term storage, **When** user executes `UPDATE table SET column = value WHERE id = X`, **Then** the system creates a new version in fast storage with updated `_updated` timestamp, leaving the old version in long-term storage unchanged
3. **Given** a record has been updated 3 times (original + 2 updates all persisted), **When** user queries the record, **Then** the system returns only the version with MAX(_updated) timestamp by joining all storage layers
4. **Given** a persisted record, **When** user executes `DELETE FROM table WHERE id = X`, **Then** the system sets `_deleted = true` and updates `_updated` timestamp in fast storage, and subsequent queries exclude this record
5. **Given** a deleted record (`_deleted = true`), **When** user queries the table, **Then** the record is filtered out (WHERE _deleted = false) even though historical versions exist in long-term storage

---

### User Story 2 - Manifest Files for Query Optimization (Priority: P2)

System administrators need efficient query execution on tables with many persisted storage files. A manifest file per table tracks batch file metadata including min/max values for all columns (especially `_updated`), enabling the query planner to skip unnecessary file scans based on timestamp ranges and column predicates.

**Why this priority**: Performance optimization for large tables - without manifest files, queries must scan all persisted batch files even when data is concentrated in specific time windows or value ranges. This becomes critical as tables grow beyond 100+ batch files. Secondary to core UPDATE/DELETE functionality.

**Independent Test**: Can be fully tested by creating a table, flushing data to multiple batch files, then querying with selective WHERE clauses. Success is verified by observing that the query planner only reads relevant batch files based on manifest metadata (min/max _updated, min/max column values).

**Acceptance Scenarios**:

1. **Given** a table with data flushed to 5 batch files (batch-0001.parquet to batch-0005.parquet), **When** query includes `WHERE _updated >= '2025-11-10T00:00:00Z'`, **Then** the system uses manifest to identify only batch files with max_updated overlapping that range and skips older batches
2. **Given** a manifest tracking min/max values per column per batch file, **When** query includes `WHERE id = 12345`, **Then** the system scans only batch files where 12345 falls within the indexed ID range
3. **Given** a query targeting only fast storage data, **When** manifest indicates no relevant data in long-term storage based on _updated ranges, **Then** the system skips all batch file reads
4. **Given** a flush operation completing, **When** new batch file is written, **Then** the system reads current manifest, increments max_batch counter, writes batch-{max_batch+1}.parquet, and updates manifest with new batch metadata
5. **Given** a manifest file with batch entries, **When** query execution reads manifest, **Then** the manifest provides file path, min/max values for all columns including _updated (nanosecond precision), row count, byte size, and schema version

---

### User Story 3 - Bloom Filter Optimization for Batch Files (Priority: P2)

Query performance on large batch files needs row-level filtering before reading full column data. Bloom filters embedded in Parquet files enable efficient point lookups by quickly eliminating batch files that definitely don't contain a specific ID or timestamp value.

**Why this priority**: Critical performance optimization for point queries (e.g., `WHERE id = X`) - without Bloom filters, the system must read and decompress full columns from every batch file. Bloom filters provide probabilistic set membership testing with minimal space overhead (<1% of file size). Secondary to basic manifest-based file skipping.

**Independent Test**: Can be fully tested by creating a table with 100K records flushed across 10 batch files, then executing `WHERE id = specific_value` queries. Success is verified by observing that only 1-2 batch files are scanned (those where Bloom filter returns "maybe") instead of all 10 files.

**Acceptance Scenarios**:

1. **Given** a flush operation writing a new batch file, **When** Parquet file is created, **Then** the system automatically generates Bloom filters for `_id` column and `_updated` column by default
2. **Given** a table with indexed columns defined in schema, **When** batch file is written, **Then** the system creates Bloom filters for all indexed columns in addition to default `_id` and `_updated` filters
3. **Given** a batch file with Bloom filters, **When** query includes `WHERE _id = 12345`, **Then** the system tests Bloom filter before reading column data, skipping files where filter returns "definitely not present"
4. **Given** a query with `WHERE indexed_column = value`, **When** indexed_column has a Bloom filter, **Then** the system uses the filter to eliminate batch files, reducing I/O by 90%+ for point lookups
5. **Given** Bloom filter false positive rate configuration, **When** batch file is written, **Then** the system tunes filter size to achieve target false positive rate (default 1%) balancing space vs accuracy

---

### User Story 4 - Execute DML as Different User (AS USER) (Priority: P2)

A service account or admin needs to insert, update, or delete records on behalf of a specific user without switching authentication context. This enables system operations like message routing, AI-generated content, and cross-user notifications.

**Why this priority**: Critical for multi-tenant systems where services need to act on behalf of users (e.g., chat systems, notification services, AI assistants). Without this, services would need separate authentication per user, creating security and performance overhead.

**Independent Test**: Can be fully tested by authenticating as a service/admin user, executing `INSERT INTO table AS USER 'user123' VALUES (...)`, then verifying the record is owned by user123 (not the service account). Works independently for both User and Stream tables.

**Acceptance Scenarios**:

1. **Given** authenticated as service role, **When** executing `INSERT INTO namespace.user_table AS USER 'user123' VALUES (...)`, **Then** the record is inserted with user123 as the owner, visible only to user123 (respecting RLS)
2. **Given** authenticated as admin role, **When** executing `UPDATE namespace.stream_table AS USER 'user456' SET column = value WHERE id = X`, **Then** the record is updated as if user456 performed the action
3. **Given** authenticated as regular user role, **When** attempting `DELETE FROM user_table AS USER 'other_user' WHERE condition`, **Then** the system rejects the operation with "Permission denied: AS USER requires service/admin role"
5. **Given** authenticated as service role, **When** attempting `INSERT INTO shared_table AS USER 'user123' VALUES (...)`, **Then** the system rejects the operation with "AS USER clause not supported for Shared tables"

---

### User Story 5 - MVCC Storage Architecture with System Columns (Priority: P1)

Developers and database administrators need a clean Multi-Version Concurrency Control (MVCC) architecture where:
1. Users must supply a primary key (any column name) when creating user/shared tables
2. KalamDB automatically adds two system columns: `_seq` (Snowflake ID versioning) and `_deleted` (soft delete flag)
3. The Arrow schema is cached with `_seq` and `_deleted` to avoid repeated schema manipulation
4. INSERT operations store rows with key format: `{table_id}:{user_id}:{_seq}` where `_seq` is a unique Snowflake ID per version
5. UPDATE/DELETE operations append new versions to the same store (never in-place updates) with incremented `_seq`
6. Queries use MAX(`_seq`) with `_deleted = false` filtering to retrieve the latest visible version
7. All DML operations (user/shared tables) use unified, reusable functions - no separate logic paths

**Why this priority**: CRITICAL for data integrity, query correctness, and code maintainability. MVCC enables:
- **Zero-locking concurrency**: Writers never block readers (append-only storage)
- **Point-in-time queries**: Retrieve data as of specific `_seq` (future feature)
- **Simplified flush logic**: Snapshot uses MAX(`_seq`) per row to get latest version for Parquet
- **Unified codebase**: User and Shared tables use identical storage patterns (eliminates 50%+ duplicate code)

Without this foundation, UPDATE/DELETE implementations would be scattered, inconsistent, and impossible to optimize for high-concurrency workloads.

**Independent Test**: Create table with user-defined PK, insert rows (auto-generate `_seq`), update/delete (append new versions), query (verify MAX(`_seq`) resolution), flush (verify Parquet contains deduplicated latest versions). Verify user/shared tables use same code paths.

**Acceptance Scenarios**:

1. **Given** a user creates a table, **When** CREATE TABLE executes, **Then** user MUST supply a primary key column (any name like `id`, `email`, `order_id`) and system auto-adds `_seq BIGINT` and `_deleted BOOLEAN` columns
2. **Given** table creation, **When** Arrow schema is constructed, **Then** `_seq` and `_deleted` are cached in CachedTableData.arrow_schema (never recomputed per-query)
3. **Given** an INSERT operation to user table, **When** user provides PK value (or system generates UUID), **Then** system generates SeqId for `_seq`, sets `_deleted = false`, stores user_id in UserTableRow.user_id field, stores user PK + data in `fields` JSON, and uses storage key `{user_id}:{_seq}` via UserTableRowId.storage_key() method
4. **Given** an UPDATE operation, **When** user modifies a row, **Then** system appends a NEW version with new SeqId `_seq` and updated `fields` JSON (original version unchanged in hot storage)
5. **Given** a DELETE operation, **When** user deletes a row, **Then** system appends a NEW version with `_deleted = true` and new SeqId `_seq` (original version unchanged)
6. **Given** a SELECT query, **When** scanning data, **Then** DataFusion applies MAX(`_seq`) grouping per PK and filters WHERE `_deleted = false` to return only latest visible versions
7. **Given** a FLUSH operation, **When** writing to Parquet, **Then** system snapshots data, deduplicates using MAX(`_seq`) per PK, and writes only latest versions to disk (hot storage keeps all versions until next flush)
8. **Given** hot storage (RocksDB), **When** any DML operation executes, **Then** system ALWAYS appends (never updates keys in-place) ensuring lock-free writes
9. **Given** user tables and shared tables, **When** any DML operation executes, **Then** BOTH table types use the same unified insertion/update/deletion functions (zero duplicate logic)
10. **Given** sync operations (SELECT since `_seq > last_synced`), **When** client requests changes, **Then** system efficiently returns all versions after specified SeqId threshold for incremental sync
11. **Given** shared table creation, **When** table is created, **Then** SharedTableRow contains only `_seq`, `_deleted`, and `fields` (no access_level per row - cached in schema definition)
12. **Given** user table scan by user, **When** querying specific user's data, **Then** RocksDB uses prefix scan on `{user_id}:` to efficiently retrieve only that user's rows
13. **Given** incremental sync query, **When** scanning for `_seq > threshold`, **Then** RocksDB range scan efficiently skips older versions using SeqId ordering

---

### User Story 6 - Manifest Cache Lifecycle (Priority: P1)

Database administrators and query planners need to minimize repeated reads of `manifest.json` from S3/local storage while ensuring cache and source remain in full sync, leveraging KalamDB's existing SchemaRegistry and EntityStore patterns.

**Why this priority**: Performance optimization for manifest access - without caching, every query must read manifest.json from S3/local storage (10-50ms latency), multiplied across thousands of queries/sec. Manifest caching reduces this to sub-millisecond memory access while maintaining strict consistency. Essential for production workloads with high query rates on tables with many batch files.

**Independent Test**: Can be fully tested by executing queries on a table with manifest.json in S3, measuring cache hit/miss rates, forcing flush operations, and verifying cache/storage synchronization. Success is verified by: (1) first query reads from S3, (2) subsequent queries use cache, (3) flush updates both cache and S3 atomically, (4) eviction removes stale entries based on TTL.

**Acceptance Scenarios**:

#### Manifest Read during Query (Priority: P1)

1. **Given** a query targets a user table, **When** system checks ManifestCache CF via SchemaRegistry, **Then** if manifest entry exists and is fresh, it is used directly without S3 read
2. **Given** no cache entry is found for a table, **When** query executes, **Then** manifest is read once from S3/local storage, then stored in both ManifestCache CF (RocksDB) and in-memory hot cache (for TTL-based reuse)
3. **Given** a cached manifest exists, **When** query accesses it, **Then** queries after that reuse the manifest until TTL expiration or explicit invalidation
4. **Given** cache and S3 manifest have mismatched ETags or modified times, **When** stale detection occurs, **Then** manifest is re-fetched from S3 and overwritten in cache
5. **Given** a manifest is accessed from cache, **When** query completes, **Then** `last_accessed` timestamp is updated in-memory only (lightweight DashMap, not RocksDB) for eviction scheduling

#### Manifest Write during Flush (Priority: P1)

1. **Given** a flush operation completes successfully, **When** manifest update is triggered, **Then** system writes updated manifest.json to S3/local path (`/data/{namespace}/{table}/user_{id}/manifest.json`), ManifestCache CF (persistent cache), and in-memory cache entry (if present)
2. **Given** flush writes manifest to cache and S3, **When** operation completes, **Then** system logs `manifest_cache_sync_success` or `manifest_cache_sync_failure` (on mismatch or write failure)
3. **Given** flush updates manifest, **When** cache synchronization completes, **Then** cache and persisted manifest are guaranteed to represent the same file list and version post-flush

#### Manifest Cache Management & Eviction (Priority: P2)

1. **Given** an administrator needs to monitor cached manifests, **When** executing `SHOW MANIFEST CACHE;` SQL command, **Then** system returns columns: `namespace`, `table`, `user_id`, `etag`, `last_refreshed`, `last_accessed`, `ttl`, `source`, `sync_state`
2. **Given** ManifestEvictionJob runs periodically, **When** eviction executes, **Then** system removes stale or unused manifest entries based on `ttl_seconds` and `last_accessed` (configurable in system.jobs)
3. **Given** manifest cache eviction runs, **When** determining candidates, **Then** `last_accessed` values are stored in lightweight in-memory map (not RocksDB) ensuring small footprint and fast cleanup
4. **Given** manifest cache configuration in config.toml, **When** server starts, **Then** system respects `ttl_seconds`, `eviction_interval_seconds`, `max_entries`, and `last_accessed_memory_window` settings

**Configuration Example**:
```toml
[manifest_cache]
ttl_seconds = 600                    # 10 minutes cache TTL
eviction_interval_seconds = 300      # Evict every 5 minutes
max_entries = 50000                  # Max cached manifests
last_accessed_memory_window = 600    # Keep access timestamps for 10 minutes
```

---

### User Story 7 - Centralized Configuration Access (Priority: P3)

Developers need a single consistent way to access all application configuration across the codebase, eliminating duplicate config models and file I/O scattered throughout modules.

**Why this priority**: Reduces code duplication, prevents configuration drift between components, and improves performance by eliminating redundant file reads. Essential for maintainability as configuration grows.

**Independent Test**: Can be fully tested by searching the codebase for direct config file reads (e.g., `fs::read_to_string("config.toml")`), migrating them to `AppContext.config()`, removing duplicate DTOs, and verifying all components still function correctly.

**Acceptance Scenarios**:

1. **Given** a module previously reading config from file directly, **When** refactored to use `AppContext.config().section_name`, **Then** the module accesses the same configuration values without file I/O
2. **Given** multiple modules using different config DTOs for the same settings, **When** consolidated to use AppContext config structs, **Then** all modules reference a single source of truth
3. **Given** server startup, **When** AppContext is initialized, **Then** configuration is loaded once and available to all components via shared reference
4. **Given** a configuration change requiring restart, **When** server restarts, **Then** all components automatically use updated config without code changes

---

### User Story 8 - Type-Safe Job Executor Parameters (Priority: P3)

Developers implementing job executors need type-safe parameter handling instead of manual JSON parsing, reducing boilerplate and preventing runtime errors from malformed parameters.

**Why this priority**: Improves developer experience and code quality. While not user-facing, this prevents production bugs from parameter parsing failures and makes executor code more maintainable. Lower priority because existing JSON approach works, just less elegantly.

**Independent Test**: Can be fully tested by refactoring one executor (e.g., FlushExecutor) to use generic typed parameters, verifying parameter validation at compile time, and confirming all existing job execution tests still pass.

**Acceptance Scenarios**:

1. **Given** a FlushExecutor implementation, **When** refactored to use `impl JobExecutor<FlushParams>` with generic type parameter, **Then** parameters are automatically deserialized to FlushParams struct with compile-time type safety
2. **Given** a job created with invalid parameters, **When** executor attempts to deserialize parameters, **Then** the error is caught during parameter parsing with clear type mismatch message
3. **Given** multiple executor types (Flush, Cleanup, Retention), **When** each defines its own parameter struct, **Then** each executor has type-specific parameter validation without shared JSON parsing code
4. **Given** a job executor execution, **When** parameters include nested structures or arrays, **Then** the generic deserializer handles complex types correctly without manual parsing logic

---

### Edge Cases

- What happens when UPDATE creates a new version while the original is being persisted to long-term storage?
- How does the system handle concurrent updates to the same record from different sessions?
- What happens if `_updated` timestamps are identical for two versions of the same record (nanosecond precision collision)?
- How does the system handle querying across many versions (e.g., 100+ updates to the same record)?
- What happens when a deletion (`_deleted = true`) is created while old versions are being queried?
- What happens if manifest.json becomes corrupted or out of sync with actual batch files?
- How does the system handle manifest rebuild when batch files exist but manifest is missing?
- What happens when flush operation fails after writing batch file but before updating manifest?
- How does the system ensure atomic manifest updates (read max_batch → write new file → update manifest)?
- What happens when Bloom filter indicates "maybe present" but record is not in the batch file (false positive)?
- How does AS USER validation behave when the target user_id doesn't exist in the system?
- What happens when a service account uses AS USER with a soft-deleted user account?
- How does the system handle AS USER syntax in prepared statements or batch operations?
- What happens when central configuration initialization fails during server startup due to missing settings?
- How does the system handle configuration updates - is hot-reload supported or restart required?
- What happens when a job receives parameters that fail validation against the expected structure?
- How does the system handle backward compatibility when changing job parameter schemas?
- What happens when AS USER is attempted on a Shared table (should be rejected)?
- What happens when user attempts to manually set `_id` value in INSERT statement?
- How does Snowflake ID generation handle clock skew or node_id collisions across distributed nodes?
- What happens when existing tables (created before this feature) are migrated to include `_id` column?
- How does the system handle user-defined `id` column that conflicts with system `_id` column naming?
- What happens if SystemColumnsService is bypassed and system columns are modified directly?

## Requirements *(mandatory)*

### Functional Requirements

#### MVCC Storage Architecture (FR-001 to FR-025)

- **FR-001**: System MUST require users to supply a primary key column (any name) when creating user/shared tables via CREATE TABLE statement
- **FR-002**: System MUST create SeqId type wrapping Snowflake ID (i64) with timestamp extraction methods
- **FR-003**: System MUST automatically add `_seq: SeqId` column to all user and shared tables for MVCC version tracking
- **FR-004**: System MUST automatically add `_deleted: bool` column to all user and shared tables for soft deletion tracking
- **FR-005**: System MUST cache Arrow schema with `_seq` and `_deleted` columns in CachedTableData.arrow_schema during table creation (zero per-query recomputation)
- **FR-006**: System MUST generate unique SeqId (via Snowflake ID) for `_seq` on every INSERT/UPDATE/DELETE operation (timestamp + node_id + sequence)
**FR-007**: System MUST use storage key format `{user_id}:{_seq}` for UserTableRowId (composite key with UserId and SeqId components, implements StorageKey trait like TableId)
**FR-008**: System MUST use SeqId directly as storage key for SharedTableRowId (no wrapper struct needed)
- **FR-009**: System MUST set `_deleted = false` for INSERT operations by default
- **FR-010**: System MUST store user_id in UserTableRow.user_id field to identify which user owns the row (since all users stored in same RocksDB store)
- **FR-011**: System MUST store user-provided PK value + all user columns in `fields: JsonValue` within UserTableRow
- **FR-011**: System MUST store all shared table columns in `fields: JsonValue` within SharedTableRow (no access_level per row)
- **FR-012**: System MUST append new versions with new SeqId for UPDATE operations (NEVER modify existing keys in hot storage)
- **FR-013**: System MUST append new versions with `_deleted = true` and new SeqId for DELETE operations (NEVER modify existing keys)
- **FR-014**: System MUST maintain all historical versions in hot storage (RocksDB) until flush operation executes
- **FR-015**: System MUST apply MAX(`_seq`) grouping per primary key during SELECT queries to retrieve latest version of each row
- **FR-016**: System MUST filter WHERE `_deleted = false` during SELECT queries to exclude soft-deleted rows
- **FR-017**: System MUST use the same unified DML functions for both user tables and shared tables (zero duplicate code paths)
- **FR-018**: System MUST support incremental sync queries via `SELECT * WHERE _seq > {last_synced}` to retrieve all changes after specified SeqId
- **FR-019**: System MUST deduplicate rows using MAX(`_seq`) per PK during FLUSH operations before writing to Parquet (only latest version persisted)
- **FR-020**: System MUST keep all versions in hot storage after flush (flush creates Parquet snapshot, does NOT delete hot storage versions)
- **FR-021**: System MUST ensure SeqId values are globally unique and monotonically increasing across all operations (Snowflake ID guarantees)
- **FR-022**: System MUST validate that user-provided PK values are unique per table (reject duplicate PK inserts with error)
- **FR-023**: System MUST allow NULL PK values only if PK column is nullable in schema (enforce NOT NULL PK constraint otherwise)
- **FR-024**: System MUST support RocksDB prefix scans using `{user_id}:` for efficient user-specific queries in UserTableStore
- **FR-025**: System MUST support RocksDB range scans using SeqId ordering for efficient `WHERE _seq > threshold` queries

#### Manifest File for Query Optimization (FR-016 to FR-030)

- **FR-016**: System MUST create a manifest.json file per user table (per user directory) tracking batch file metadata
- **FR-017**: System MUST create a manifest.json file per shared table tracking batch file metadata
- **FR-018**: System MUST store manifest files in same directory as batch files (user_id/ for user tables, shared/ for shared tables)
- **FR-019**: System MUST include version number, generation timestamp, and max_batch counter in manifest metadata
- **FR-020**: System MUST track batch file entries with: file path (batch-{number}.parquet), min/max values for all columns, row count, byte size, schema version, status
- **FR-021**: System MUST record min_updated and max_updated (with nanosecond precision) for each batch file to enable timestamp-based filtering
- **FR-022**: System MUST read current manifest before flush operation to determine next batch number
- **FR-023**: System MUST write new batch file using naming pattern: batch-{max_batch+1}.parquet where max_batch comes from manifest
- **FR-024**: System MUST update manifest.json atomically after flush completes with new batch entry and incremented max_batch counter
- **FR-025**: System MUST use manifest during query planning to determine which batch files overlap query predicates
- **FR-026**: System MUST skip batch files when manifest proves they cannot contain matching records (e.g., _updated range outside query window)
- **FR-027**: System MUST validate manifest integrity on table access and rebuild if inconsistencies detected
- **FR-028**: System MUST handle queries when manifest is unavailable by falling back to scanning all batch files
- **FR-029**: System MUST support manifest schema evolution (version field) for future enhancements
- **FR-030**: System MUST mark batch files with status field in manifest (active, compacting, archived) for lifecycle management

#### Manifest Cache Lifecycle (FR-031 to FR-046)

- **FR-031**: System MUST create a dedicated RocksDB column family `manifest_cache` for cached manifest entries
- **FR-032**: System MUST register ManifestCache CF as new EntityStore in SchemaRegistry (following existing patterns for schemas, users, tables, jobs)
- **FR-033**: System MUST implement ManifestCacheStore with CRUD operations (get/put/delete) for cache entries
- **FR-034**: System MUST implement ManifestCacheService with in-memory DashMap for hot cache + RocksDB fallback for persistence
- **FR-035**: System MUST use cache key format `{namespace}:{table}:{user_id}` for manifest cache entries
- **FR-036**: System MUST serialize ManifestCacheEntry as JSON (serde_json) for storage in RocksDB to keep entries human-readable and backward compatible
- **FR-037**: System MUST check ManifestCache CF via SchemaRegistry during query planning before reading manifest from S3/local storage
- **FR-038**: System MUST use cached manifest directly when entry exists and is fresh (within TTL)
- **FR-039**: System MUST read manifest from S3/local storage on cache miss, then store in both ManifestCache CF and in-memory hot cache
- **FR-040**: System MUST validate cache freshness using ETag or modified time comparison between cache and S3 source
- **FR-041**: System MUST re-fetch manifest from S3 and overwrite cache when stale detection occurs (ETag/modified time mismatch)
- **FR-042**: System MUST update `last_accessed` timestamp in-memory only (lightweight DashMap, not RocksDB) for eviction scheduling
- **FR-043**: System MUST write updated manifest.json to both S3/local path AND ManifestCache CF when flush operation completes
- **FR-044**: System MUST log `manifest_cache_sync_success` or `manifest_cache_sync_failure` after flush manifest update
- **FR-045**: System MUST guarantee cache and S3 manifest represent same file list and version post-flush (atomic synchronization)
- **FR-046**: System MUST support `SHOW MANIFEST CACHE;` SQL command returning: namespace, table, user_id, etag, last_refreshed, last_accessed, ttl, source, sync_state

#### Manifest Cache Eviction (FR-047 to FR-053)

- **FR-047**: System MUST implement ManifestEvictionJob as background job tracked in system.jobs
- **FR-048**: System MUST run ManifestEvictionJob periodically based on `eviction_interval_seconds` configuration
- **FR-049**: System MUST remove stale manifest entries based on `ttl_seconds` and `last_accessed` timestamp
- **FR-050**: System MUST store `last_accessed` values in lightweight in-memory map (not RocksDB) for fast eviction decisions
- **FR-051**: System MUST support manifest_cache configuration in config.toml with fields: ttl_seconds, eviction_interval_seconds, max_entries, last_accessed_memory_window
- **FR-052**: System MUST restore manifest cache from RocksDB CF on server restart, revalidating TTL via stored `last_refreshed` timestamps before serving cached manifests
- **FR-053**: System MUST repopulate in-memory hot cache on first query after server restart

#### Bloom Filter Optimization (FR-054 to FR-061)

- **FR-054**: System MUST generate Bloom filters for `_id` and `_updated` columns by default when writing batch files
- **FR-055**: System MUST generate Bloom filters for all indexed columns defined in table schema
- **FR-056**: System MUST embed Bloom filters in Parquet file metadata for efficient access without reading column data
- **FR-057**: System MUST test Bloom filters during query execution before reading column data from batch files
- **FR-058**: System MUST skip batch files where Bloom filter returns "definitely not present" for equality predicates
- **FR-059**: System MUST configure Bloom filter false positive rate (default 1%) balancing space overhead vs accuracy
- **FR-060**: System MUST handle Bloom filter false positives gracefully by reading actual column data for verification
- **FR-061**: System MUST support disabling Bloom filters per column via table configuration for space-constrained scenarios

#### AS USER Syntax (FR-062 to FR-069)

- **FR-062**: System MUST support `AS USER 'user_id'` clause in INSERT statements for User and Stream tables
- **FR-063**: System MUST support `AS USER 'user_id'` clause in UPDATE statements for User and Stream tables
- **FR-064**: System MUST support `AS USER 'user_id'` clause in DELETE statements for User and Stream tables
- **FR-065**: System MUST restrict AS USER clause to service and admin roles only
- **FR-066**: System MUST validate that the target user_id exists before executing AS USER operations
- **FR-067**: System MUST apply Row-Level Security (RLS) policies as if the specified user executed the operation
- **FR-068**: System MUST audit AS USER operations with both the authenticated user (actor) and target user (subject)
- **FR-069**: System MUST reject AS USER operations on Shared tables with clear error message

#### Unified DML Functions (FR-070 to FR-077)

- **FR-070**: System MUST create a centralized DML module with unified functions for INSERT/UPDATE/DELETE operations
- **FR-071**: System MUST implement unified `append_version()` function used by INSERT, UPDATE, and DELETE handlers for both user/shared tables
- **FR-072**: System MUST implement unified `resolve_latest_version()` function used by query planning for both user/shared tables
- **FR-073**: System MUST implement unified `validate_primary_key()` function enforcing uniqueness and NOT NULL constraints for both user/shared tables
- **FR-074**: System MUST implement unified `generate_storage_key()` function creating `{table_id}:{user_id}:{_seq}` keys for both user/shared tables
- **FR-075**: System MUST eliminate duplicate INSERT/UPDATE/DELETE logic between user_table_*.rs and shared_table_*.rs files (target: 50%+ code reduction)
- **FR-076**: System MUST use same Arrow schema manipulation functions for both user/shared tables (zero per-table-type schema logic)
- **FR-077**: System MUST validate via code analysis that user/shared DML handlers call identical unified functions (zero divergence)

#### Centralized Configuration (FR-082 to FR-087)

- **FR-082**: System MUST provide all configuration through AppContext.config() instead of direct file reads
- **FR-083**: System MUST eliminate duplicate config DTOs and use single source of truth from AppContext
- **FR-084**: System MUST load configuration once during AppContext initialization and share via Arc reference
- **FR-085**: System MUST provide type-safe access to configuration sections (database, server, jobs, auth, etc.)
- **FR-086**: System MUST validate configuration completeness at startup and fail fast with clear errors
- **FR-087**: System MUST document all configuration migration points in code comments for future reference

#### Generic Job Executor (FR-088 to FR-093)

- **FR-088**: Job execution framework MUST support type-specific parameter definitions for each job type
- **FR-089**: System MUST automatically validate and convert job parameters to job-specific structures at execution time
- **FR-090**: System MUST provide early validation of job parameter correctness before job execution
- **FR-091**: System MUST maintain compatibility with existing job storage format (JSON serialization with serde)
- **FR-092**: System MUST serialize job parameters as JSON strings in RocksDB (backward compatible with existing Job table schema)
- **FR-093**: System MUST deserialize JSON parameters to typed structs (e.g., FlushParams, CleanupParams) using serde at execution time
- **FR-094**: System MUST support parameter schema evolution via `Option<T>` fields and serde default attributes for backward compatibility
- **FR-095**: System MUST handle parameter validation errors with clear messages identifying expected parameter structure

#### Test Coverage Requirements (FR-096 to FR-101)

- **FR-096**: System MUST include tests for updating records after they have been flushed to long-term storage
- **FR-097**: System MUST include tests for records updated 3 times (original + 2 updates, all flushed)
- **FR-098**: System MUST include tests verifying deleted records (_deleted = true) are not returned in query results
- **FR-099**: System MUST include tests verifying MAX(_updated) correctly resolves latest version across storage tiers
- **FR-100**: System MUST include tests for concurrent updates to the same record
- **FR-101**: System MUST include tests for querying records with _deleted flag at different storage layers

#### Performance Regression Tests (FR-102 to FR-107)

- **FR-102**: System MUST include performance regression tests measuring query latency with increasing version counts (baseline: 1 version, test: 10 versions, 100 versions)
- **FR-103**: System MUST enforce that query latency MUST NOT exceed 2x baseline when resolving 10+ versions of the same record across storage layers
- **FR-104**: System MUST include performance tests measuring manifest-based file skipping efficiency (target: <5ms to eliminate 90% of batch files)
- **FR-105**: System MUST include performance tests measuring Bloom filter lookup overhead (target: <1ms per batch file for point queries)
- **FR-106**: System MUST include performance tests measuring UPDATE operation latency on persisted records (target: <10ms for append-only write to fast storage)
- **FR-107**: System MUST include performance tests measuring concurrent UPDATE throughput degradation (target: <20% reduction with 10 concurrent updates to same record)

### Key Entities

- **SeqId**: Type-safe wrapper around Snowflake ID (i64) with timestamp extraction, ordering, and serialization capabilities
- **RecordVersion**: A versioned instance of a record containing `user_id: UserId`, `_seq: SeqId`, `_deleted: bool`, and `fields: JsonValue` (user PK + data) for UserTableRow
- **VersionResolution**: Query-time logic that groups rows by PK and selects version with MAX(`_seq`), filtering WHERE `_deleted = false`
- **UserTableRowId**: Composite key struct containing `user_id: UserId` and `_seq: SeqId`, implements StorageKey trait with storage_key() method returning `{user_id}:{_seq}` bytes (similar to TableId pattern)
- **SharedTableRowId**: Just `SeqId` directly (no wrapper struct, simplest possible key)
- **AppendOnlyStorage**: Architecture where UPDATE/DELETE operations append new versions with new SeqId instead of modifying existing keys
- **SnapshotDeduplication**: Flush operation logic that applies MAX(`_seq`) per PK to write only latest versions to Parquet files
- **IncrementalSync**: Query pattern `SELECT * WHERE _seq > {last_synced}` retrieving all row versions after specified SeqId threshold
- **ManifestFile**: JSON file (manifest.json) per table tracking batch file metadata including max_batch counter, file paths, min/max column values, row counts, sizes, and schema versions
- **BatchFileEntry**: Manifest entry for a single batch file containing: file path (batch-{number}.parquet), min_updated/max_updated timestamps, min/max values for all columns, row_count, size_bytes, schema_version, status
- **ManifestCache CF**: Dedicated RocksDB column family storing cached manifest entries with key format `{namespace}:{table}:{user_id}`
- **ManifestCacheEntry**: Cached manifest data including: manifest content (JSON), ETag, last_refreshed timestamp, source path, sync_state (in_sync/stale/error)
- **ManifestCacheStore**: EntityStore implementation providing CRUD operations for manifest cache (get/put/delete) using RocksDB persistence
- **ManifestCacheService**: High-level service component with dual-layer caching: in-memory DashMap for hot cache + RocksDB ManifestCacheStore for persistent cache
- **ManifestEvictionJob**: Background job that periodically removes stale or unused manifest entries based on ttl_seconds and last_accessed timestamps
- **BloomFilter**: Probabilistic data structure embedded in Parquet file metadata for `id`, `_updated`, and indexed columns enabling efficient point lookup elimination
- **ImpersonationContext**: Execution context holding both authenticated user (actor) and target user (subject) for audit trail and authorization enforcement in AS USER operations (User/Stream tables only)
- **SystemColumnsService**: Centralized service managing all system column operations (`_id` generation with Snowflake IDs, `_updated` timestamp management, `_deleted` flag handling) ensuring single source of truth and preventing scattered logic
- **SnowflakeId**: 64-bit unique identifier combining timestamp (41 bits), node_id (10 bits), and sequence (12 bits) for globally unique, monotonically increasing IDs
- **UnifiedDMLModule**: Centralized module containing reusable INSERT/UPDATE/DELETE/query functions eliminating duplicate logic between user/shared tables
- **JsonValue**: serde_json::Value storing all user-defined columns (PK + data) within UserTableRow/SharedTableRow.fields
- **ManifestService**: Centralized service component responsible for all manifest file operations (create, update, rebuild, validate) ensuring single source of truth for batch file metadata management
- **TypedJobParameters**: Structured parameter container enabling validation and type checking for job-specific configurations

## Clarifications

### Session 2025-11-10

- Q: How are concurrent updates handled when a record is being flushed to long-term storage? → A: Flush operations work on a snapshot of fast storage while live fast storage continues accepting writes
- Q: What happens if two versions of the same record have identical `_updated` timestamps (nanosecond precision collision)? → A: Use storage layer priority tie-breaker (fast storage > long-term storage) plus +1ns increment prevention
- Q: How does the system recover from manifest.json corruption or inconsistencies with batch files? → A: Centralized ManifestService performs scan-and-rebuild with degraded mode fallback
- Q: Should AS USER operations work with soft-deleted user accounts? → A: Rejected as non-existent with generic error message

### Session 2025-11-11

- Q: How are node_id values assigned to database instances for Snowflake ID generation? → A: Static string configuration from config.toml, hashed to 10-bit integer (0-1023) for Snowflake ID generation

### Concurrent Update Conflict Resolution (added 2025-11-10)

**Question**: How are concurrent updates handled when a record is being flushed to long-term storage?

**Answer**: Concurrent updates during flush are safe because flush operations work on a snapshot of fast storage while the live fast storage continues accepting writes. Conflict resolution:

1. **Flush Process**: Creates snapshot of fast storage at T0, writes snapshot to batch file, continues normal operation
2. **Concurrent Updates**: Any updates after T0 write to live fast storage (not the snapshot), creating new versions with current `_updated` timestamp
3. **Version Ordering**: Query-time resolution uses MAX(_updated) across all layers - newer updates (after snapshot) automatically supersede flushed versions
4. **No Locking Required**: Fast storage remains fully available during flush - updates are never blocked or delayed

Example timeline:
- T0: Flush begins, snapshots record {id:123, status:'pending', _updated:T0}
- T1: User updates to status='active', creates version with _updated:T1 in live fast storage
- T2: Flush completes, writes snapshot to batch-0005.parquet
- Query at T3: Returns status='active' (MAX(_updated) = T1 > T0)

### Nanosecond Timestamp Collision Handling (added 2025-11-10)

**Question**: What happens if two versions of the same record have identical `_updated` timestamps (nanosecond precision collision)?

**Answer**: Timestamp collisions are handled with a secondary tie-breaker:

1. **Primary Ordering**: MAX(_updated) timestamp (nanosecond precision)
2. **Collision Tie-Breaker**: If multiple versions have identical _updated timestamps, use storage layer priority:
   - Fast storage version > Long-term storage version (newer always wins)
   - Within same layer: undefined behavior (system assumes nanosecond precision prevents this)
3. **Prevention**: Fast storage UPDATE operations add 1 nanosecond to previous MAX(_updated) if current_time() returns same value
4. **Guarantee**: Each UPDATE increments _updated by at least 1ns, ensuring strict ordering even at microsecond update rates

Example:
- Version A: _updated = 1699564800.123456789 (in batch file)
- Version B: _updated = 1699564800.123456789 (in fast storage, collision detected)
- Version B adjusted to: 1699564800.123456790 (guaranteed unique)

### Manifest Corruption Recovery Strategy (added 2025-11-10)

**Question**: How does the system recover from manifest.json corruption or inconsistencies with batch files?

**Answer**: Manifest corruption recovery follows a scan-and-rebuild approach with centralized management:

1. **Centralized Manifest Service**: Single ManifestService component within flush operations handles ALL manifest operations (create, update, rebuild, validate) - no scattered manifest logic across codebase
2. **Detection**: On table access, validate manifest exists, has valid JSON, and max_batch matches highest batch-{N}.parquet file number
3. **Rebuild Trigger**: If validation fails, initiate background rebuild while serving queries in degraded mode
4. **Degraded Mode**: Queries fallback to scanning all batch files directly (no manifest optimization) while rebuild runs
5. **Rebuild Process** (executed by ManifestService):
   - Scan directory for all batch-*.parquet files, extract max N → new max_batch
   - For each batch file: read Parquet metadata (footer only, not full data) to extract min/max values, row counts, schema
   - Generate new manifest.json with reconstructed metadata
   - Atomic rename: manifest.json.tmp → manifest.json
6. **Flush Job Integration**: Each flush operation completion triggers ManifestService to:
   - Validate current manifest exists (rebuild if missing/corrupt)
   - Update manifest with new batch entry
   - Ensure manifest is consistent before flush job completes
7. **Availability**: Table remains readable during entire rebuild (degraded performance, not downtime)
8. **Duration**: Manifest rebuild completes in O(number_of_batch_files × parquet_footer_read_time), typically <1 second per 100 files

**Architecture Constraint**: ManifestService is the ONLY component allowed to read/write manifest files - enforces single source of truth for manifest management.

### AS USER Validation with Soft-Deleted Users (added 2025-11-10)

**Question**: Should AS USER operations work with soft-deleted user accounts?

**Answer**: AS USER validation treats soft-deleted users as non-existent:

1. **Validation Logic**: Check both user_id existence AND active status (deleted_at IS NULL)
2. **Soft-Deleted Users**: Rejected with same error as non-existent users: 'Invalid user_id for AS USER operation'
3. **Rationale**: Soft-deleted users should be treated as if they don't exist - prevents services from creating data in deleted user contexts
4. **Security**: Generic error message prevents information leakage about which user_ids exist in the system
5. **Exception**: System-level jobs (user cleanup, data migration) bypass AS USER and operate directly on user stores

Example rejections:
- `INSERT INTO app.messages AS USER 'deleted_user123'` → Error: 'Invalid user_id for AS USER operation'
- `INSERT INTO app.messages AS USER 'never_existed'` → Error: 'Invalid user_id for AS USER operation' (same message)

### Service-Level Subscription Design (added 2025-11-10)

**Question**: How should service-level subscriptions handle backpressure and event volume at scale?

**Status**: DEFERRED - Requires deeper design exploration

**Context**: Initial design proposed `SUBSCRIBE TO ALL` syntax for service-level monitoring of all user changes. However, this approach has significant backpressure challenges when subscribers can't keep up with event rates (10K events/sec generated, 1K/sec consumed).

**Alternative Approach Under Consideration**: 
Instead of real-time subscription connections, use materialized stream tables:
- `CREATE STREAM TABLE admin.all_messages AS SELECT * FROM user_tables.messages` 
- Stream table acts as durable event log with built-in backpressure via storage
- Services read from stream table at their own pace (pull model vs push)
- Enables batch processing, replay, and better resource isolation

**Next Steps**: 
1. Design stream table federation/aggregation semantics
2. Define materialization triggers and update propagation
3. Specify query syntax for cross-user table aggregation
4. Determine resource limits and quotas

**Impact**: User Story 5 (Service-Level Subscriptions) removed from this spec pending design completion. Will be addressed in separate feature specification.

### Snowflake ID Node Assignment Strategy (added 2025-11-11)

**Question**: How are node_id values assigned to database instances for Snowflake ID generation?

**Answer**: Static configuration from config.toml with startup validation (Option A):

1. **Configuration**: Each database instance has `node_id` field in config.toml as a string value (e.g., "node-001", "prod-db-42")
2. **Hash to 10-bit Integer**: System hashes the node_id string to derive 10-bit integer (0-1023) for Snowflake ID generation
3. **Validation**: On startup, AppContext initialization validates node_id string exists and is non-empty
4. **Collision Risk**: Hash collisions possible but rare (1024 possible values); operator responsibility to choose distinct node_id strings in cluster deployments
5. **Integration**: Reuses existing AppContext.node_id pattern (already loaded once from config.toml per Phase 10 architecture)

Example config.toml:
```toml
[server]
node_id = "prod-db-01"  # String identifier for this instance, hashed to 10-bit value for Snowflake IDs
```

**Hashing Strategy**: Use stable hash function (e.g., CRC32 or FNV-1a) and take modulo 1024 to derive 10-bit integer consistently from same string.

**Rationale**: Aligns with existing KalamDB architecture where NodeId is string type allocated once from config.toml (SC-000, SC-007 from AGENTS.md). Simple, explicit, no external coordinator dependencies.

### Job Parameter Serialization Format (added 2025-11-11)

**Question**: How are typed job parameters serialized for persistence while maintaining backward compatibility?

**Answer**: JSON serialization with serde, bridging type safety and persistence:

1. **Storage Format**: Job parameters stored as JSON strings in RocksDB `system.jobs` table (existing schema, no migration needed)
2. **Serialization**: When creating job, executor-specific params struct (e.g., `FlushParams`, `CleanupParams`) serialized to JSON via `serde_json::to_string()`
3. **Deserialization**: When executing job, JSON string deserialized to typed struct via `serde_json::from_str::<FlushParams>()`
4. **Type Safety**: Each executor defines its own params struct with `#[derive(Serialize, Deserialize)]`
5. **Backward Compatibility**: 
   - Old jobs (created before type-safe params) continue working via fallback to generic JSON parsing
   - New fields added via `Option<T>` with `#[serde(default)]` attribute (missing fields deserialize to None)
   - Schema evolution: `FlushParamsV2` can deserialize `FlushParamsV1` JSON via serde's flexible parsing
6. **Validation**: Serde deserialization errors caught early with clear messages like "Missing required field 'namespace_id' in FlushParams"

Example parameter evolution:
```rust
// Version 1 (initial)
#[derive(Serialize, Deserialize)]
struct FlushParams {
    namespace_id: String,
    table_name: String,
}

// Version 2 (added optional batch_size)
#[derive(Serialize, Deserialize)]
struct FlushParams {
    namespace_id: String,
    table_name: String,
    #[serde(default)]  // Old jobs without this field deserialize to None
    batch_size: Option<usize>,
}
```

**Performance**: JSON deserialization overhead <1ms per job execution (negligible compared to job execution time of seconds/minutes).

**Alternative Considered**: Binary formats (bincode, protobuf) rejected due to schema evolution complexity and loss of human-readable job debugging.

### Manifest Cache Architecture Integration (added 2025-11-11)

**Question**: How does the manifest cache integrate with existing KalamDB architecture (SchemaRegistry, EntityStore patterns, Column Family separation)?

**Answer**: Manifest cache follows established KalamDB patterns for consistency and maintainability:

**Architecture Components**:

| Component | Role | Pattern |
|-----------|------|---------|
| `manifest_cache` CF | Dedicated RocksDB column family for cached manifests | Follows CF separation pattern (same as `schemas`, `users`, `tables`, `jobs`) |
| ManifestCacheStore | EntityStore implementation with CRUD operations (get/put/delete) | Implements same trait pattern as UserStore, JobStore, etc. |
| SchemaRegistry | Registers ManifestCache as new EntityStore in global registry | Same registration pattern as existing system tables |
| ManifestCacheService | High-level service with in-memory DashMap + RocksDB fallback | Dual-layer caching: hot (memory) + cold (RocksDB) |
| FlushExecutor | Writes manifest to both cache and S3/local storage | Write-through pattern: update cache + source atomically |
| QueryPlanner | Reads manifest via ManifestCacheService (cache-first) | Read-through pattern: check cache → fallback to S3 → populate cache |
| ManifestEvictionJob | Background job for periodic eviction of stale entries | Same job framework as FlushJob, CleanupJob (UnifiedJobManager) |

**Synchronization Rules**:

| Operation | Cache Action | S3/Local Action | Consistency Guarantee |
|-----------|--------------|-----------------|------------------------|
| Query (cache miss) | Read from S3 → store in CF + memory | Read manifest.json | Cache populated for next query |
| Query (cache hit) | Use cached manifest, update last_accessed | None | Sub-millisecond access |
| Flush | Write to CF + memory (if present) | Write manifest.json | Atomic sync: both updated or flush fails |
| Eviction Job | Remove expired entries from CF + memory | None | TTL-based cleanup |
| Server Restart | Restore from CF → repopulate memory on first query | None | Persistent cache survives restarts |

**Cache Key Format**: `{namespace}:{table}:{user_id}` (consistent with existing RocksDB key patterns)

**Cache Entry Schema** (ManifestCacheEntry):
```rust
struct ManifestCacheEntry {
    manifest_content: String,       // JSON manifest data
    etag: Option<String>,            // S3 ETag for staleness detection
    last_refreshed: i64,             // Unix timestamp (seconds)
    source_path: String,             // /data/{namespace}/{table}/user_{id}/manifest.json
    sync_state: SyncState,           // InSync | Stale | Error
}

enum SyncState {
    InSync,    // Cache matches S3 source
    Stale,     // ETag mismatch, needs refresh
    Error,     // Sync failed, needs rebuild
}
```

`last_refreshed` is persisted so that, on restart, ManifestCacheService can reapply TTL rules before serving cached manifests (entries older than `ttl_seconds` trigger an immediate refresh instead of returning stale data).

**SchemaRegistry Integration**:
```rust
// AppContext initialization (following Phase 5 pattern)
let manifest_cache_store = Arc::new(ManifestCacheStore::new(db.clone()));
schema_registry.register_entity_store("manifest_cache", manifest_cache_store);

// Usage in query planner
let manifest_cache = app_context.schema_registry().get_manifest_cache();
let manifest = manifest_cache.get_or_load(namespace, table, user_id).await?;
```

**Benefits of This Architecture**:
- **Consistency**: Same patterns as existing system tables (zero new concepts)
- **Performance**: Dual-layer cache (memory + RocksDB) balances speed and persistence
- **Reliability**: Write-through ensures cache never diverges from source
- **Observability**: `SHOW MANIFEST CACHE;` command provides visibility into cache state
- **Maintainability**: Centralized in SchemaRegistry (single source of truth for all metadata)

---

## Assumptions *(optional)*

- **Append-Only Architecture**: System uses append-only writes where updates create new versions rather than modifying existing records in-place
- **System Columns**: All user and shared tables already include `_updated` (timestamp with nanosecond precision) and `_deleted` (boolean) columns
- **Storage Tiers**: System uses two-tier storage (fast storage for recent writes + long-term storage for persisted batch files)
- **Flush Process**: Records are periodically moved from fast storage to long-term storage, with new versions flushed to batch-{number}.parquet files
- **Batch Numbering**: Batch files use sequential numbering controlled by manifest max_batch counter (not timestamps)
- **User Isolation**: User tables are stored in per-user directories (/data/{namespace}/{table}/user_{id}/) enabling per-user manifest files
- **Shared Table Layout**: Shared tables use single shared/ directory (/data/{namespace}/{table}/shared/) with single manifest file
- **Manifest Cache Scope**: Manifest cache persistence (RocksDB + hot cache) is node-local; multi-node clusters rely on shared manifest.json sync via S3/local storage updates
- **Role-Based Authorization**: System has existing role hierarchy (user < service < admin < system) for permission checks
- **Configuration Format**: Single configuration file loaded at startup containing all system settings
- **Job Framework**: Existing job execution system supports parameter passing and error handling
- **Audit Requirements**: All impersonated operations require full audit trail for compliance
- **Parquet Support**: Parquet library supports Bloom filter metadata embedding and column statistics extraction

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: INSERT operations append new versions to hot storage with unique SeqId completing in <5ms (append-only write, zero reads required)
- **SC-002**: UPDATE operations append new versions without reading Parquet files (append-only write to hot storage with new SeqId)
- **SC-003**: DELETE operations execute in under 10ms by appending new version with `_deleted = true` and new SeqId (append-only write, zero destructive operations)
- **SC-004**: Queries returning latest versions correctly resolve MAX(`_seq`) per PK across hot storage within 2x baseline query time
- **SC-005**: User tables and shared tables use identical DML functions with zero duplicate code paths (verified by code analysis showing 50%+ code reduction)
- **SC-006**: Arrow schemas with `_seq: SeqId` and `_deleted: bool` are cached in CachedTableData, never recomputed per-query (verified by profiling showing zero to_arrow_schema() calls in hot path)
- **SC-005**: Manifest-based query optimization reduces unnecessary batch file scans by 80%+ for queries with timestamp range predicates
- **SC-006**: Bloom filter optimization reduces I/O by 90%+ for point queries (WHERE id = X) on tables with 100+ batch files
- **SC-007**: FLUSH operations deduplicate using MAX(`_seq`) per PK and write only latest versions to Parquet (verified by comparing hot storage row count vs Parquet row count after flush)
- **SC-008**: Incremental sync queries `WHERE _seq > X` return all row versions after SeqId threshold completing in <50ms for 10K changed rows
- **SC-009**: Primary key uniqueness is enforced with clear error messages rejecting duplicate PK inserts within 5ms (index lookup + validation)
- **SC-010**: UserTableRowId implements StorageKey trait with storage_key() method returning `{user_id}:{_seq}` bytes, following same pattern as TableId (verified by code inspection showing struct with user_id and _seq fields)
- **SC-011**: SharedTableRowId uses SeqId directly (no wrapper overhead) with zero serialization overhead (verified by profiling)
- **SC-012**: SeqId timestamp extraction works correctly returning milliseconds since epoch (verified by unit tests with known Snowflake IDs)
- **SC-013**: UserTableRow contains user_id field to identify row ownership plus essential fields (_seq, _deleted, fields) with zero redundant per-row overhead
- **SC-014**: SharedTableRow contains only essential fields (_seq, _deleted, fields) with zero per-row overhead (no access_level, verified by struct inspection)
- **SC-011**: Centralized configuration access eliminates all direct configuration file reads outside initialization code
- **SC-012**: Job parameter validation failures provide actionable error messages identifying expected structure within 100ms
- **SC-013**: Type-safe job parameter implementation reduces parameter handling code by 50%+ lines per job type
- **SC-014**: Test suite achieves 100% coverage for MVCC append-only writes, MAX(`_seq`) resolution, and unified DML functions
- **SC-015**: Performance regression tests confirm SeqId-based version resolution remains within 2x baseline when resolving 10+ versions per PK
- **SC-016**: Code analysis confirms user/shared table DML handlers call identical unified functions (zero divergence, 50%+ code reduction)
- **SC-017**: Snowflake ID generation (via SeqId) produces globally unique, monotonically increasing values with 1M+ IDs/sec throughput per node
- **SC-018**: Hot storage (RocksDB) performs zero in-place key updates (100% append-only writes verified by operation logging)
- **SC-019**: RocksDB prefix scans using `{user_id}:` efficiently retrieve user-specific data completing in <10ms for 10K user rows
- **SC-020**: RocksDB range scans using SeqId ordering efficiently implement `WHERE _seq > threshold` completing in <20ms for 100K total rows
- **SC-019**: Manifest-based file skipping achieves <5ms evaluation time per query and eliminates 90%+ of irrelevant batch file scans
- **SC-020**: Bloom filter optimization achieves <1ms overhead per batch file and reduces I/O by 90%+ for point queries on large tables (100+ batch files)
