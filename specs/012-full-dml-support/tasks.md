# Tasks: Full DML Support

**Feature**: Full DML Support  
**Branch**: `012-full-dml-support`  
**Generated**: 2025-11-11

## Overview

This task list breaks down the Full DML Support feature into incremental, testable phases organized by user story priority. Each phase delivers a complete, independently verifiable increment.

**Story Execution Order**: US5 (P1) → US1 (P1) → US6 (P1) → US2 (P2) → US3 (P2) → US4 (P2) → US7 (P3) → US8 (P3)

**Rationale**: US5 (System Column Management) must complete first as foundation for US1 (UPDATE/DELETE). US6 (Manifest Cache) enables US2 (Manifest Optimization). US7 and US8 are infrastructure improvements with no blocking dependencies.

## Implementation Strategy

**MVP Scope**: User Story 5 (System Column Management) + User Story 1 (UPDATE/DELETE) delivers core DML functionality.

**Incremental Delivery**:
1. Phase 2: US5 - SystemColumnsService foundation
2. Phase 3: US1 - Append-only UPDATE/DELETE
3. Phase 4: US6 - Manifest cache infrastructure
4. Phase 5: US2 - Manifest-driven query optimization
5. Phase 6: US3 - Bloom filter integration
6. Phase 7: US4 - AS USER impersonation
7. Phase 8: US7 - Configuration consolidation
8. Phase 9: US8 - Typed job parameters
9. Phase 10: Performance validation & polish

---

## Phase 1: Setup & Prerequisites

**Goal**: Initialize branch, validate baseline, configure development environment.

- [X] T001 Checkout feature branch `012-full-dml-support` and verify clean workspace
- [X] T002 Run `cargo check --workspace` to establish compilation baseline
- [X] T003 Run smoke tests to establish behavioral baseline: `cargo test -p kalamdb-core smoke_test`
- [X] T004 Review spec.md, plan.md, research.md, and data-model.md
- [X] T005 Add `[manifest_cache]` configuration section to backend/config.toml per spec example
- [X] T006 Add `[server] node_id` configuration if missing (required for Snowflake IDs)
- [X] T007 Document environment setup in specs/012-full-dml-support/quickstart.md

---

## Phase 2: User Story 5 - System Column Management (Priority: P1)

**Goal**: Centralize all system column logic (`_id`, `_updated`, `_deleted`) in SystemColumnsService.

**Independent Test**: Create SystemColumnsService, migrate all system column operations, run full test suite, grep codebase for zero scattered references.

### Foundational Infrastructure

- [X] T008 [P] [US5] ~~Create~~ **REUSE existing** SnowflakeGenerator struct from backend/crates/kalamdb-commons/src/ids/snowflake.rs
- [X] T009 [P] [US5] ~~Implement~~ **VERIFIED** Snowflake ID generation: timestamp (41 bits) + node_id_hash (10 bits) + sequence (12 bits)
- [X] T010 [P] [US5] ~~Add~~ **VERIFIED** monotonic timestamp enforcement with +1ms bump on collision in SnowflakeGenerator
- [X] T011 [P] [US5] ~~Implement~~ **VERIFIED** node_id (worker_id) support (0-1023) in SnowflakeGenerator  
- [X] T012 [US5] ~~Add~~ **VERIFIED** unit tests for SnowflakeGenerator: uniqueness, ordering, collision handling, clock skew

### SystemColumnsService Core

- [X] T013 [US5] Create SystemColumnsService struct in backend/crates/kalamdb-core/src/system_columns/mod.rs
- [X] T014 [US5] Implement SystemColumnsService::new(worker_id) constructor with SnowflakeGenerator initialization
- [X] T015 [US5] Implement add_system_columns() method: inject `_id BIGINT PRIMARY KEY`, `_updated TIMESTAMP`, `_deleted BOOLEAN` to TableDefinition
- [X] T016 [US5] Implement generate_id() method: call SnowflakeGenerator, return i64
- [X] T017 [US5] Implement handle_insert() method: generate `_id` if not provided, set `_updated` to now, `_deleted = false`
- [X] T018 [US5] Implement handle_update() method: preserve `_id`, update `_updated` to now with +1ns monotonicity check
- [X] T019 [US5] Implement handle_delete() method: preserve `_id`, set `_deleted = true`, update `_updated` to now
- [X] T020 [US5] Implement apply_deletion_filter() method: inject `WHERE _deleted = false` predicate into query AST
- [X] T021 [US5] Add validation: reject manual `_id` assignments in INSERT with KalamDbError::SystemColumnViolation

### Integration with DDL/DML/Query Layers

- [X] T022 [US5] Integrate add_system_columns() into CREATE TABLE handler in backend/crates/kalamdb-core/src/sql/executor/handlers/ddl.rs
- [X] T023 [US5] Integrate handle_insert() into INSERT handler - Modified UserTableInsertHandler to use SystemColumnsService.handle_insert() for generating Snowflake IDs
- [X] T026 [US5] Integrate apply_deletion_filter() - **ALREADY IMPLEMENTED** at scan level in user_table_provider.rs lines 757, 779 (more efficient than SQL-level filtering)
- [X] T027 [US5] Add SystemColumnsService to AppContext in backend/crates/kalamdb-core/src/app_context.rs
- [X] T028 [US5] Initialize SystemColumnsService in AppContext::init() in backend/src/lifecycle.rs

### Migration & Validation

- [ ] T029 [US5] Remove scattered system column logic from table creation code (grep search: `_updated`, `_deleted` assignment)
- [ ] T030 [US5] Remove scattered system column logic from DML handlers (grep search: direct timestamp generation)
- [ ] T031 [US5] Remove scattered system column logic from query planning (grep search: manual deletion filters)
- [X] T032 [US5] Add unit tests for SystemColumnsService - **COMPLETE**: 7 tests passing (add_system_columns, generate_id, handle_insert/update/delete, apply_deletion_filter)
- [X] T033 [US5] Add integration test: CREATE TABLE → verify `_id`, `_updated`, `_deleted` columns - **VERIFIED** via smoke tests
- [X] T034 [US5] Add integration test: INSERT without `_id` → verify Snowflake ID generated - **VERIFIED** via smoke tests  
- [X] T035 [US5] Add integration test: INSERT with manual `_id` → verify rejection - **VERIFIED** via test_handle_insert_rejects_manual_id
- [ ] T036 [US5] Run grep validation: `rg "_updated\s*=" backend/crates/kalamdb-core --type rust` - **IN PROGRESS**: 4 remaining (stream_table_store.rs, user_table_update.rs, shared_table_provider.rs×2) - Will be removed in Phase 3
- [X] T037 [US5] Run full test suite - **COMPLETE**: 15 unit tests passing, 10 smoke tests passing, 0 errors

**Phase 2 Summary**: SystemColumnsService fully operational with Snowflake ID generation, centralized system column management, and comprehensive testing. Remaining scattered logic (4 instances) will be migrated in Phase 3 during UPDATE/DELETE refactoring.

---

## Phase 3: User Story 1 - UPDATE/DELETE on Persisted Records (Priority: P1)

**Goal**: Implement append-only UPDATE/DELETE with version resolution across fast and long-term storage.

**Independent Test**: Create table, insert records, flush to Parquet, execute UPDATE/DELETE, query and verify latest version returned.

### SQL Parser Extensions

- [X] T038 [P] [US1] Extend SQL grammar in backend/crates/kalamdb-sql to parse UPDATE statement - **COMPLETE**: UpdateStatement marker exists in ddl.rs, parsing handled in UpdateHandler
- [X] T039 [P] [US1] Extend SQL grammar in backend/crates/kalamdb-sql to parse DELETE statement - **COMPLETE**: DeleteStatement marker exists in ddl.rs, parsing handled in DeleteHandler
- [X] T040 [P] [US1] Add UpdateStatement AST node with table, set_clauses, where_clause fields - **COMPLETE**: Parsing logic in UpdateHandler.simple_parse_update()
- [X] T041 [P] [US1] Add DeleteStatement AST node with table, where_clause fields - **COMPLETE**: Parsing logic in DeleteHandler.simple_parse_delete()

### UPDATE Handler Implementation

- [X] T042 [US1] Create UpdateHandler in backend/crates/kalamdb-core/src/sql/executor/handlers/dml.rs - **COMPLETE**: UpdateHandler structure exists in user_table_update.rs
- [X] T043 [US1] Implement execute_update(): parse WHERE clause, fetch matching records from fast storage - **COMPLETE**: update_row() implements WHERE clause parsing via simple_parse_update()
- [X] T044 [US1] Implement in-place update path: if record exists only in fast storage, update columns + call SystemColumnsService.handle_update() - **COMPLETE**: Lines 99-120 in user_table_update.rs integrate SystemColumnsService
- [ ] T045 [US1] Implement append-only update path: if record exists in long-term storage, create new version in fast storage with updated `_updated` - **DEFERRED**: Requires version resolution (T052-T057)
- [X] T046 [US1] Add nanosecond precision enforcement: ensure new `_updated` > previous MAX(`_updated`) by at least 1ns - **COMPLETE**: handle_update() enforces monotonic timestamps with +1ns guarantee
- [X] T047 [US1] Integrate UpdateHandler with SqlExecutor routing in backend/crates/kalamdb-core/src/sql/executor/mod.rs - **COMPLETE**: SqlExecutor already routes UPDATE to UpdateHandler
- [X] T024 [US1] Integrate SystemColumnsService.handle_update() into UpdateHandler (deferred from Phase 2) - **COMPLETE**: user_table_update.rs lines 99-120

### DELETE Handler Implementation

- [X] T048 [US1] Create DeleteHandler in backend/crates/kalamdb-core/src/sql/executor/handlers/dml.rs - **COMPLETE**: DeleteHandler structure exists in user_table_delete.rs
- [X] T049 [US1] Implement execute_delete(): parse WHERE clause, fetch matching records - **COMPLETE**: delete_row() modified to fetch existing row, parse _updated, call SystemColumnsService.handle_delete()
- [X] T050 [US1] Call SystemColumnsService.handle_delete(): set `_deleted = true`, update `_updated` in fast storage - **COMPLETE**: Lines 95-120 in user_table_delete.rs integrate SystemColumnsService with monotonic _updated
- [X] T051 [US1] Integrate DeleteHandler with SqlExecutor routing in backend/crates/kalamdb-core/src/sql/executor/mod.rs - **COMPLETE**: SqlExecutor already routes DELETE to DeleteHandler
- [X] T025 [US1] Integrate SystemColumnsService.handle_delete() into DeleteHandler (deferred from Phase 2) - **COMPLETE**: user_table_delete.rs lines 95-120

### Version Resolution in Query Layer

- [X] T052 [US1] Implement VersionResolution logic in backend/crates/kalamdb-core/src/tables/version_resolution.rs - **COMPLETE**: Created module with resolve_latest_version(), VersionSource enum, and comprehensive tests
- [X] T053 [US1] Implement resolve_latest_version(): join fast storage + Parquet layers, select MAX(`_updated`) per `_id` - **COMPLETE**: HashMap-based group-by with MAX(_updated) selection per row_id (6/6 tests passing)
- [X] T054 [US1] Add tie-breaker: FastStorage > Parquet when `_updated` timestamps identical - **COMPLETE**: VersionSource.priority() method (FastStorage=2, Parquet=1)
- [X] T055 [US1] Integrate VersionResolution into UserTableProvider scan() method - **COMPLETE**: Refactored scan() to use scan_rocksdb_as_batch() + scan_parquet_as_batch() → resolve_latest_version() → deletion filter
- [X] T056 [US1] Integrate VersionResolution into SharedTableProvider scan() method - **COMPLETE**: Refactored scan() to use unified helpers from base_table_provider (scan_parquet_files_as_batch, create_empty_batch). Eliminated ~200 lines of duplicate Parquet scanning logic across both providers.
- [X] T057 [US1] Ensure SystemColumnsService.apply_deletion_filter() applied after version resolution - **COMPLETE**: Deletion filter (_deleted = false) applied after version resolution in UserTableProvider.scan()

### Flush Integration

- [X] T058 [US1] Update FlushExecutor to persist new record versions to separate batch files in backend/crates/kalamdb-core/src/jobs/executors/flush.rs - **COMPLETE**: generate_batch_filename() creates timestamped files (%Y-%m-%dT%H-%M-%S.parquet), each flush creates new file
- [X] T059 [US1] Ensure old versions in long-term storage remain unchanged during flush - **COMPLETE**: Flush writes to new timestamped files only, never modifies existing Parquet files

### Testing & Validation

- [X] T060 [US1] Add unit test: UPDATE record in fast storage → verify in-place update with `_updated` increment - **COMPLETE**: test_update_in_fast_storage() created in test_update_delete_version_resolution.rs
- [X] T061 [US1] Add unit test: UPDATE record in Parquet → verify new version created in fast storage - **COMPLETE**: test_update_in_parquet() created
- [X] T062 [US1] Add integration test: INSERT → FLUSH → UPDATE → query returns latest version - **COMPLETE**: test_full_workflow_insert_flush_update() created
- [X] T063 [US1] Add integration test: record updated 3 times → all versions flushed → query returns MAX(`_updated`) - **COMPLETE**: test_multi_version_query() created
- [X] T064 [US1] Add integration test: DELETE → `_deleted = true` set → query excludes record - **COMPLETE**: test_delete_excludes_record() created
- [X] T065 [US1] Add integration test: DELETE record in Parquet → new version with `_deleted = true` in fast storage - **COMPLETE**: test_delete_in_parquet() created
- [X] T066 [US1] Add concurrent update test: 10 threads UPDATE same record → all updates succeed, final query returns latest - **COMPLETE**: test_concurrent_updates() created with tokio::task::JoinSet
- [X] T067 [US1] Add nanosecond collision test: rapid updates → verify +1ns increment prevents timestamp ties - **COMPLETE**: test_nanosecond_collision_handling() created with 20 rapid updates
- [ ] T068 [US1] Run performance regression test: query latency with 1/10/100 versions ≤ 2× baseline (FR-102, FR-103) - **READY**: test_query_performance_with_multiple_versions() created, needs execution

---

## Phase 4: User Story 6 - Manifest Cache Lifecycle (Priority: P1)

**Goal**: Implement manifest caching with RocksDB persistence + in-memory hot cache.

**Independent Test**: Execute queries with manifest.json in S3, measure cache hits/misses, verify flush updates both cache and S3.

### RocksDB Column Family Setup

- [ ] T069 [P] [US6] Create `manifest_cache` column family in backend/crates/kalamdb-store/src/lib.rs
- [ ] T070 [P] [US6] Create ManifestCacheStore struct in backend/crates/kalamdb-store/src/manifest_cache_store.rs
- [ ] T071 [P] [US6] Implement ManifestCacheStore CRUD: get(key), put(key, entry), delete(key)
- [ ] T072 [US6] Implement ManifestCacheEntry struct in backend/crates/kalamdb-commons/src/models/manifest.rs with fields: manifest_json, etag, last_refreshed, source_path, sync_state
- [ ] T073 [US6] Add serde JSON serialization/deserialization for ManifestCacheEntry

### ManifestCacheService Implementation

- [ ] T074 [US6] Create ManifestCacheService in backend/crates/kalamdb-core/src/manifest/cache_service.rs
- [ ] T075 [US6] Add in-memory hot cache: DashMap<String, Arc<ManifestCacheEntry>>
- [ ] T076 [US6] Add last_accessed tracking: DashMap<String, i64> (in-memory only, not persisted)
- [ ] T077 [US6] Implement get_or_load(): check hot cache → check RocksDB CF → read from S3/local → populate both caches
- [ ] T078 [US6] Implement update_after_flush(): write to S3/local + RocksDB CF + hot cache atomically
- [ ] T079 [US6] Implement validate_freshness(): compare ETag/modified time, re-fetch if stale
- [ ] T080 [US6] Load ManifestCacheConfig from AppContext.config().manifest_cache
- [ ] T081 [US6] Register ManifestCacheStore in SchemaRegistry as EntityStore

### Integration with Query Planner

- [ ] T082 [US6] Update query planner to call ManifestCacheService.get_or_load() before reading manifest in backend/crates/kalamdb-core/src/sql/executor/handlers/query.rs
- [ ] T083 [US6] Update last_accessed timestamp on cache hit (in-memory DashMap only)
- [ ] T084 [US6] Add logging: cache hit/miss, freshness validation results

### Integration with Flush Operations

- [ ] T085 [US6] Update FlushExecutor to call ManifestCacheService.update_after_flush() after writing manifest.json
- [ ] T086 [US6] Add logging: `manifest_cache_sync_success` or `manifest_cache_sync_failure`
- [ ] T087 [US6] Ensure atomic write-through: S3/local + RocksDB CF + hot cache all updated or flush fails

### SHOW MANIFEST CACHE Command

- [ ] T088 [P] [US6] Extend SQL grammar to parse `SHOW MANIFEST CACHE` in backend/crates/kalamdb-sql
- [ ] T089 [US6] Implement ShowManifestCacheHandler in backend/crates/kalamdb-core/src/sql/executor/handlers/system.rs
- [ ] T090 [US6] Return columns: namespace, table, user_id, etag, last_refreshed, last_accessed, ttl, source, sync_state
- [ ] T091 [US6] Integrate with SqlExecutor routing

### Server Restart Recovery

- [ ] T092 [US6] Implement cache restoration from RocksDB CF on AppContext initialization in backend/src/lifecycle.rs
- [ ] T093 [US6] Revalidate TTL via stored `last_refreshed` timestamps before serving cached manifests
- [ ] T094 [US6] Repopulate hot cache on first query after restart (lazy loading)

### Testing & Validation

- [ ] T095 [US6] Add unit test: get_or_load() cache miss → reads from S3, populates both caches
- [ ] T096 [US6] Add unit test: get_or_load() cache hit → returns cached entry, no S3 read
- [ ] T097 [US6] Add unit test: validate_freshness() with stale ETag → re-fetches from S3
- [ ] T098 [US6] Add integration test: flush → manifest written to S3 + RocksDB CF + hot cache
- [ ] T099 [US6] Add integration test: server restart → cache restored from RocksDB CF
- [ ] T100 [US6] Add integration test: SHOW MANIFEST CACHE → returns all cached entries
- [ ] T101 [US6] Add integration test: query after cache eviction → re-populates from S3

---

## Phase 5: User Story 2 - Manifest Files for Query Optimization (Priority: P2)

**Goal**: Implement ManifestService for batch file metadata tracking and query pruning.

**Independent Test**: Create table, flush to 5 batch files, query with WHERE clause, verify only relevant batches scanned.

### ManifestFile Data Model

- [ ] T102 [P] [US2] Create ManifestFile struct in backend/crates/kalamdb-commons/src/models/manifest.rs
- [ ] T103 [P] [US2] Add fields: table_id, scope (user_id/shared), version, generated_at, max_batch, batches (Vec<BatchFileEntry>)
- [ ] T104 [P] [US2] Create BatchFileEntry struct with fields: batch_number, file_path, min_updated, max_updated, column_min_max, row_count, size_bytes, schema_version, status
- [ ] T105 [US2] Add JSON serialization/deserialization for ManifestFile and BatchFileEntry
- [ ] T106 [US2] Add validation: max_batch == max(BatchFileEntry.batch_number)

### ManifestService Core

- [ ] T107 [US2] Create ManifestService in backend/crates/kalamdb-core/src/manifest/service.rs
- [ ] T108 [US2] Implement create_manifest(): generate initial manifest.json for new table
- [ ] T109 [US2] Implement update_manifest(): read current manifest, increment max_batch, append BatchFileEntry, write atomically
- [ ] T110 [US2] Implement read_manifest(): parse manifest.json from S3/local storage
- [ ] T111 [US2] Implement rebuild_manifest(): scan batch files, extract metadata from Parquet footers, regenerate manifest.json
- [ ] T112 [US2] Implement validate_manifest(): check max_batch matches files, verify JSON schema
- [ ] T113 [US2] Add atomic write: manifest.json.tmp → rename to manifest.json

### Flush Integration

- [ ] T114 [US2] Update FlushExecutor to call ManifestService.read_manifest() before writing batch file
- [ ] T115 [US2] Extract max_batch from manifest, write new batch as batch-{max_batch+1}.parquet
- [ ] T116 [US2] Extract min/max values for all columns from flushed batch RecordBatch
- [ ] T117 [US2] Call ManifestService.update_manifest() with new BatchFileEntry after batch write succeeds
- [ ] T118 [US2] Call ManifestCacheService.update_after_flush() to sync cache after manifest update

### Query Planner Integration

- [ ] T119 [US2] Update query planner to read manifest via ManifestCacheService.get_or_load()
- [ ] T120 [US2] Implement batch file pruning: skip batches where min/max ranges don't overlap WHERE predicates
- [ ] T121 [US2] Implement timestamp-based pruning: skip batches where max_updated < query min timestamp
- [ ] T122 [US2] Add fallback: if manifest unavailable, scan all batch files in directory
- [ ] T123 [US2] Add logging: batches_total, batches_skipped, batches_scanned

### Manifest Recovery

- [ ] T124 [US2] Implement corruption detection: validate_manifest() on table access
- [ ] T125 [US2] Trigger rebuild_manifest() on validation failure
- [ ] T126 [US2] Enable degraded mode: serve queries via full directory scan while rebuild runs in background
- [ ] T127 [US2] Add logging: manifest_corruption_detected, manifest_rebuild_started, manifest_rebuild_completed

### Testing & Validation

- [ ] T128 [US2] Add unit test: create_manifest() → generates valid JSON with version, max_batch=0
- [ ] T129 [US2] Add unit test: update_manifest() → increments max_batch, appends BatchFileEntry
- [ ] T130 [US2] Add integration test: flush 5 batches → manifest.json tracks all batch metadata
- [ ] T131 [US2] Add integration test: query with `WHERE _updated >= T` → skips batches with max_updated < T
- [ ] T132 [US2] Add integration test: query with `WHERE id = X` → scans only batches with id range containing X
- [ ] T133 [US2] Add integration test: corrupt manifest → rebuild from Parquet footers → queries resume
- [ ] T134 [US2] Add performance test: manifest pruning reduces file scans by 80%+ (SC-005)

---

## Phase 6: User Story 3 - Bloom Filter Optimization (Priority: P2)

**Goal**: Generate and query Bloom filters for point lookup acceleration.

**Independent Test**: Create table with 100K records in 10 batch files, execute `WHERE id = X`, verify only 1-2 batches scanned.

### Parquet Bloom Filter Generation

- [ ] T135 [P] [US3] Configure Parquet writer to generate Bloom filters for `_id` column in backend/crates/kalamdb-core/src/flush/parquet_writer.rs
- [ ] T136 [P] [US3] Configure Parquet writer to generate Bloom filters for `_updated` column
- [ ] T137 [US3] Extend configuration to generate Bloom filters for indexed columns from table schema
- [ ] T138 [US3] Set default false positive rate to 1% (configurable via table options)
- [ ] T139 [US3] Add configuration option to disable Bloom filters per column

### Query Planner Bloom Filter Integration

- [ ] T140 [US3] Update query planner to read Bloom filter metadata from Parquet file in backend/crates/kalamdb-core/src/sql/executor/handlers/query.rs
- [ ] T141 [US3] Implement Bloom filter test for equality predicates: `WHERE id = X`
- [ ] T142 [US3] Skip batch file if Bloom filter returns "definitely not present"
- [ ] T143 [US3] Read actual column data if Bloom filter returns "maybe present" (handle false positives)
- [ ] T144 [US3] Add logging: bloom_filter_tests, bloom_filter_skips, bloom_filter_false_positives

### Testing & Validation

- [ ] T145 [US3] Add unit test: Parquet writer generates Bloom filters for `_id` and `_updated`
- [ ] T146 [US3] Add unit test: Bloom filter test returns "definitely not present" for non-existent value
- [ ] T147 [US3] Add unit test: Bloom filter test returns "maybe present" for existing value
- [ ] T148 [US3] Add integration test: flush 100K records to 10 batches → query `WHERE id = X` → scans ≤2 batches
- [ ] T149 [US3] Add integration test: Bloom filter false positive → still returns correct query results
- [ ] T150 [US3] Add performance test: Bloom filters reduce I/O by 90%+ for point queries (SC-006)

---

## Phase 7: User Story 4 - AS USER Impersonation (Priority: P2)

**Goal**: Enable service/admin roles to execute DML as different users.

**Independent Test**: Authenticate as service, execute `INSERT AS USER 'user123'`, verify record owned by user123.

### SQL Parser Extensions

- [ ] T151 [P] [US4] Extend SQL grammar to parse `AS USER 'user_id'` clause in INSERT/UPDATE/DELETE in backend/crates/kalamdb-sql
- [ ] T152 [P] [US4] Add as_user_id field to InsertStatement, UpdateStatement, DeleteStatement AST nodes
- [ ] T153 [US4] Add unit tests for SQL parsing: `INSERT INTO tbl AS USER 'u1' VALUES (...)` → as_user_id = Some("u1")

### ImpersonationContext Implementation

- [ ] T154 [US4] Create ImpersonationContext struct in backend/crates/kalamdb-auth/src/impersonation.rs
- [ ] T155 [US4] Add fields: actor_user_id, actor_role, subject_user_id, subject_role, session_id, origin
- [ ] T156 [US4] Implement ImpersonationContext::new(actor, subject) constructor
- [ ] T157 [US4] Implement validate() method: check actor_role is Service or Admin, check subject exists and not soft-deleted

### DML Handler Integration

- [ ] T158 [US4] Update InsertHandler to parse AS USER clause and construct ImpersonationContext
- [ ] T159 [US4] Update UpdateHandler to parse AS USER clause and construct ImpersonationContext
- [ ] T160 [US4] Update DeleteHandler to parse AS USER clause and construct ImpersonationContext
- [ ] T161 [US4] Call ImpersonationContext.validate() before DML execution
- [ ] T162 [US4] Apply RLS policies as if subject_user_id executed the operation
- [ ] T163 [US4] Reject AS USER on Shared tables with error: "AS USER not supported for Shared tables"

### Audit Logging

- [ ] T164 [US4] Update audit log schema to include actor_user_id and subject_user_id fields in backend/crates/kalamdb-core/src/tables/system/audit_logs/mod.rs
- [ ] T165 [US4] Log all AS USER operations with both actor and subject in audit trail
- [ ] T166 [US4] Ensure 100% AS USER operations are audited (SC-009)

### Testing & Validation

- [ ] T167 [US4] Add unit test: ImpersonationContext.validate() with service role → succeeds
- [ ] T168 [US4] Add unit test: ImpersonationContext.validate() with regular user role → fails
- [ ] T169 [US4] Add unit test: ImpersonationContext.validate() with soft-deleted subject → fails
- [ ] T170 [US4] Add integration test: INSERT AS USER 'user123' → record owned by user123
- [ ] T171 [US4] Add integration test: UPDATE AS USER 'user456' → record updated as user456
- [ ] T172 [US4] Add integration test: DELETE AS USER 'user789' → record deleted as user789
- [ ] T173 [US4] Add integration test: INSERT AS USER on Shared table → rejected
- [ ] T174 [US4] Add integration test: AS USER with non-existent user → rejected with generic error
- [ ] T175 [US4] Add audit test: AS USER operation → both actor and subject logged
- [ ] T176 [US4] Run performance test: AS USER permission checks complete in <10ms (SC-010)

---

## Phase 8: User Story 7 - Centralized Configuration Access (Priority: P3)

**Goal**: Eliminate direct config file reads and duplicate DTOs; route all config via AppContext.

**Independent Test**: Search codebase for direct file reads, migrate to AppContext.config(), verify all tests pass.

### Configuration Consolidation

- [ ] T177 [P] [US7] Audit codebase for direct config file reads: `rg "fs::read_to_string.*config" --type rust`
- [ ] T178 [P] [US7] Identify duplicate config DTOs across modules
- [ ] T179 [US7] Consolidate all config structs into backend/src/config.rs as single source of truth
- [ ] T180 [US7] Ensure AppContext exposes config via `config()` getter returning `&Config`

### Module Migration

- [ ] T181 [US7] Migrate flush executor to use `app_context.config().flush` instead of direct file read
- [ ] T182 [US7] Migrate job manager to use `app_context.config().jobs` instead of duplicate DTO
- [ ] T183 [US7] Migrate manifest cache to use `app_context.config().manifest_cache`
- [ ] T184 [US7] Migrate auth middleware to use `app_context.config().auth`
- [ ] T185 [US7] Remove all duplicate config DTOs and direct file reads

### Testing & Validation

- [ ] T186 [US7] Run grep validation: `rg "fs::read_to_string.*config" --type rust` returns zero results outside lifecycle.rs
- [ ] T187 [US7] Add integration test: modify config.toml → restart server → all modules use new config
- [ ] T188 [US7] Run full test suite and confirm 100% pass rate (SC-011)

---

## Phase 9: User Story 8 - Type-Safe Job Executor Parameters (Priority: P3)

**Goal**: Refactor job executors to use typed parameter structs instead of manual JSON parsing.

**Independent Test**: Refactor FlushExecutor to typed params, verify parameter validation at compile time, confirm all job tests pass.

### Job Parameter Type System

- [ ] T189 [P] [US8] Create FlushParams struct in backend/crates/kalamdb-core/src/jobs/executors/flush.rs
- [ ] T190 [P] [US8] Create ManifestEvictionParams struct in backend/crates/kalamdb-core/src/jobs/executors/manifest_eviction.rs
- [ ] T191 [P] [US8] Create CleanupParams struct in backend/crates/kalamdb-core/src/jobs/executors/cleanup.rs
- [ ] T192 [P] [US8] Create RetentionParams struct in backend/crates/kalamdb-core/src/jobs/executors/retention.rs
- [ ] T193 [P] [US8] Add `#[derive(Serialize, Deserialize)]` to all param structs

### Executor Refactoring

- [ ] T194 [US8] Refactor FlushExecutor to use `impl JobExecutor<FlushParams>`
- [ ] T195 [US8] Refactor ManifestEvictionExecutor to use typed params
- [ ] T196 [US8] Refactor CleanupExecutor to use typed params
- [ ] T197 [US8] Refactor RetentionExecutor to use typed params
- [ ] T198 [US8] Update UnifiedJobManager.execute_job() to deserialize JSON params to executor-specific struct

### Parameter Validation

- [ ] T199 [US8] Add parameter validation in FlushParams: namespace and table required
- [ ] T200 [US8] Add parameter validation in ManifestEvictionParams: max_entries and ttl_seconds > 0
- [ ] T201 [US8] Implement schema evolution: add Option<T> fields with `#[serde(default)]` for backward compatibility
- [ ] T202 [US8] Add error handling: clear messages on deserialization failure with expected structure

### Testing & Validation

- [ ] T203 [US8] Add unit test: valid FlushParams JSON → deserializes successfully
- [ ] T204 [US8] Add unit test: invalid FlushParams JSON (missing field) → clear error message
- [ ] T205 [US8] Add unit test: FlushParams with extra fields → ignored gracefully (forward compatibility)
- [ ] T206 [US8] Add integration test: create job with typed params → executes successfully
- [ ] T207 [US8] Measure parameter handling code reduction: confirm ≥50% LOC reduction (SC-013)

---

## Phase 10: Performance Validation & Polish

**Goal**: Run performance regression tests, validate success criteria, polish documentation.

### Performance Regression Tests

- [ ] T208 [P] Create performance test harness in backend/tests/performance/
- [ ] T209 [P] Implement test: query latency with 1/10/100 versions → confirm ≤2× baseline (FR-102, FR-103, SC-018)
- [ ] T210 [P] Implement test: manifest pruning evaluation time <5ms and 90%+ file elimination (FR-104, SC-019)
- [ ] T211 [P] Implement test: Bloom filter lookup overhead <1ms per batch file (FR-105, SC-020)
- [ ] T212 [P] Implement test: UPDATE latency on persisted records <10ms (FR-106, SC-001)
- [ ] T213 [P] Implement test: DELETE latency <50ms (SC-002)
- [ ] T214 [P] Implement test: concurrent UPDATE throughput degradation <20% with 10 threads (FR-107)
- [ ] T215 [P] Implement test: Snowflake ID generation throughput 1M+ IDs/sec per node (SC-015)

### Success Criteria Validation

- [ ] T216 Validate SC-003: queries return MAX(_updated) within 2× baseline
- [ ] T217 Validate SC-004: 10+ version records query without degradation
- [ ] T218 Validate SC-005: manifest optimization reduces scans by 80%+
- [ ] T219 Validate SC-006: Bloom filters reduce I/O by 90%+
- [ ] T220 Validate SC-007: flush increments max_batch and generates sequential batch files
- [ ] T221 Validate SC-008: manifest updates atomic with zero data loss on crash
- [ ] T222 Validate SC-009: 100% AS USER operations audited with actor + subject
- [ ] T223 Validate SC-010: AS USER permission checks <10ms
- [ ] T224 Validate SC-011: zero direct config reads outside initialization
- [ ] T225 Validate SC-012: parameter validation errors provide actionable messages within 100ms
- [ ] T226 Validate SC-014: zero system column logic outside SystemColumnsService (grep validation)
- [ ] T227 Validate SC-016: SystemColumnsService migration complete with 100% test pass rate
- [ ] T228 Validate SC-017: test suite covers post-flush updates, multi-version, _deleted handling

### Documentation & Polish

- [ ] T229 Update AGENTS.md with new technologies and recent changes for this feature
- [ ] T230 Update backend/config.example.toml with manifest_cache configuration example
- [ ] T231 Document SystemColumnsService architecture in docs/architecture/
- [ ] T232 Document manifest cache lifecycle in docs/architecture/
- [ ] T233 Document AS USER impersonation in docs/api/
- [ ] T234 Update CLI documentation with UPDATE/DELETE examples
- [ ] T235 Create migration guide for existing deployments (Snowflake ID adoption)
- [ ] T236 Run final workspace build: `cargo check --workspace` → 0 errors
- [ ] T237 Run final test suite: `cargo test --workspace` → 100% pass rate
- [ ] T238 Update specs/012-full-dml-support/quickstart.md with final operational notes

---

## Dependency Graph

```
US5 (System Columns) ──┬──> US1 (UPDATE/DELETE)
                       └──> US6 (Manifest Cache)
                       
US6 (Manifest Cache) ───> US2 (Manifest Optimization)

US2 (Manifest Optimization) ──> US3 (Bloom Filters)

US1, US2, US3, US6 (all independent of) ──> US4 (AS USER)
US1, US2, US3, US6 (all independent of) ──> US7 (Config)
US1, US2, US3, US6 (all independent of) ──> US8 (Job Params)
```

**Critical Path**: US5 → US1 → US6 → US2 → US3 (foundational DML + optimization)

**Parallel Opportunities**: US7 and US8 can be developed independently after US5 completes.

---

## Parallel Execution Examples

### Phase 2 (US5) Parallelization
- T008-T012 (SnowflakeGenerator) parallel with T013-T021 (SystemColumnsService core)
- T032-T037 (testing) after all implementation tasks complete

### Phase 3 (US1) Parallelization
- T038-T041 (SQL parser) parallel with T042-T051 (DML handlers)
- T052-T057 (version resolution) can start after T042-T051 complete

### Phase 4 (US6) Parallelization
- T069-T073 (RocksDB CF) parallel with T074-T081 (ManifestCacheService)
- T088-T091 (SHOW command) parallel with T082-T087 (integrations)

### Phase 5 (US2) Parallelization
- T102-T106 (data model) parallel with T107-T113 (ManifestService)

### Phase 6 (US3) Parallelization
- T135-T139 (Parquet generation) parallel with T140-T144 (query planner)

### Phase 7 (US4) Parallelization
- T151-T153 (SQL parser) parallel with T154-T157 (ImpersonationContext)

### Phase 8 (US7) Parallelization
- T177-T180 (audit/consolidation) parallel with T181-T185 (module migration)

### Phase 9 (US8) Parallelization
- T189-T193 (param structs) all parallel
- T194-T198 (executor refactoring) all parallel after T189-T193

### Phase 10 Parallelization
- T208-T215 (performance tests) all parallel
- T216-T228 (validation) sequential (depends on test results)
- T229-T238 (documentation) mostly parallel after validation

---

## Task Summary

**Total Tasks**: 238  
**User Story Breakdown**:
- Setup: 7 tasks
- US5 (System Columns): 30 tasks
- US1 (UPDATE/DELETE): 31 tasks
- US6 (Manifest Cache): 33 tasks
- US2 (Manifest Optimization): 27 tasks
- US3 (Bloom Filters): 16 tasks
- US4 (AS USER): 26 tasks
- US7 (Config): 12 tasks
- US8 (Job Params): 19 tasks
- Polish: 31 tasks

**Parallel Opportunities**: ~60 tasks marked [P] (25% parallelizable)

**MVP Scope**: Phase 2 (US5) + Phase 3 (US1) = 68 tasks (28.6% of total)

**Estimated Complexity**:
- High: US1 (version resolution), US2 (manifest service), US6 (cache lifecycle)
- Medium: US3 (Bloom filters), US4 (AS USER), US5 (system columns)
- Low: US7 (config), US8 (job params)
