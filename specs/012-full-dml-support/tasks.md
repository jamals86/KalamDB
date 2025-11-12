# Tasks: Full DML Support (MVCC Architecture)

**Feature**: Full DML Support  
**Branch**: `012-full-dml-support`  
**Generated**: 2025-11-11  
**Updated**: 2025-11-11 (MVCC Architecture Redesign)

## ⚠️ CRITICAL ARCHITECTURE CHANGE (2025-11-11)

**Original Phase 2 (User Story 5)**: SystemColumnsService with `_id`/`_updated`/`_deleted` management

**New Phase 2 (User Story 5 - MVCC)**: Multi-Version Concurrency Control with `_seq` versioning and unified DML

### MVCC Architecture Overview:

```
┌─────────────────────────────────────────────────────────────────────┐
│                        MVCC Storage Layer                            │
├─────────────────────────────────────────────────────────────────────┤
│  Storage Keys:                                                       │
│  - UserTableRowId: {user_id}:{_seq} (composite struct with UserId  │
│    and SeqId fields, implements StorageKey trait like TableId)     │
│  - SharedTableRowId: {_seq} (SeqId directly, no wrapper)           │
│                                                                      │
│  Note: table_id NOT in key (already in column family name)         │
├─────────────────────────────────────────────────────────────────────┤
│  Row Structures:                                                     │
│                                                                      │
│  UserTableRow:                                                       │
│    - user_id: UserId (identifies row owner, all users in same store)│
│    - _seq: SeqId (Snowflake ID wrapper with timestamp extraction)  │
│    - _deleted: bool                                                 │
│    - fields: JsonValue (user PK + all user columns)                │
│                                                                      │
│  SharedTableRow:                                                     │
│    - _seq: SeqId                                                    │
│    - _deleted: bool                                                 │
│    - fields: JsonValue (all shared table columns)                  │
│    - NO access_level (cached in schema definition, not per-row)    │
├─────────────────────────────────────────────────────────────────────┤
│  SeqId Type:                                                         │
│  - Wraps Snowflake ID (i64)                                         │
│  - Provides timestamp_millis() for extraction                       │
│  - Implements Ord for range queries                                 │
│  - Serializes as i64, displays as string                           │
├─────────────────────────────────────────────────────────────────────┤
│  Operations:                                                         │
│  - INSERT: Generate SeqId, append to hot storage                   │
│  - UPDATE: Generate new SeqId, append new version (never modify)   │
│  - DELETE: Generate new SeqId, set _deleted=true, append           │
│  - SELECT: MAX(_seq) per PK, filter _deleted=false                 │
│  - FLUSH: Snapshot → deduplicate via MAX(_seq) → write Parquet    │
│  - SCAN BY USER: RocksDB prefix scan on {user_id}:                │
│  - SYNC QUERY: RocksDB range scan where _seq > threshold           │
├─────────────────────────────────────────────────────────────────────┤
│  Unified DML Functions:                                             │
│  - append_version(): Used by INSERT/UPDATE/DELETE                  │
│  - resolve_latest_version(): Used by SELECT                        │
│  - validate_primary_key(): Used by INSERT                          │
│  - generate_storage_key(): Used by all DML                         │
└─────────────────────────────────────────────────────────────────────┘
```

### Code Consolidation Impact:

**Before (Current Architecture)**:
- `user_table_insert.rs` - User table INSERT logic
- `user_table_update.rs` - User table UPDATE logic
- `user_table_delete.rs` - User table DELETE logic
- `shared_table_insert.rs` - Shared table INSERT logic (duplicate)
- `shared_table_update.rs` - Shared table UPDATE logic (duplicate)
- `shared_table_delete.rs` - Shared table DELETE logic (duplicate)
- **Row structures**: 
  - UserTableRow (row_id, user_id, fields, _updated, _deleted)
  - SharedTableRow (row_id, fields, _updated, _deleted, access_level)
- **Total**: ~1200 lines of DML code with 50%+ duplication

**After (MVCC Architecture)**:
- `unified_dml/mod.rs` - Single module with 4 core functions
  - append_version() - Used by ALL INSERT/UPDATE/DELETE operations
  - resolve_latest_version() - Used by ALL SELECT operations
  - validate_primary_key() - Used by ALL INSERT operations
  - generate_storage_key() - Used by ALL DML operations
- `user_table_dml.rs` - Thin wrapper calling unified_dml (150 lines)
- `shared_table_dml.rs` - Thin wrapper calling unified_dml (150 lines)
- **Row structures**: 
  - UserTableRow: `user_id: UserId`, `_seq: SeqId`, `_deleted: bool`, `fields: JsonValue`
  - SharedTableRow: `_seq: SeqId`, `_deleted: bool`, `fields: JsonValue` (no access_level)
- **Storage keys**: 
  - UserTableRowId: Composite struct with `user_id: UserId` and `_seq: SeqId` fields, implements StorageKey trait with storage_key() method
  - SharedTableRowId: `SeqId` directly (no wrapper)
- **Total**: ~600 lines of DML code (50% reduction, zero duplication, minimal per-row overhead)

### Migration Strategy:

1. **Phase 2 (New)**: Implement MVCC architecture with unified DML
2. **Phase 3 (Deprecated)**: Old UPDATE/DELETE tasks consolidated into Phase 2
3. **Phases 4-10**: Continue with manifest caching, Bloom filters, etc. (unchanged)

**Story Execution Order**: US5 (P1 - MVCC) → US6 (P1 - Manifest Cache) → US2 (P2 - Manifest Optimization) → US3 (P2 - Bloom Filters) → US4 (P2 - AS USER) → US7 (P3 - Config) → US8 (P3 - Job Params)

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

## Phase 2: User Story 5 - MVCC Storage Architecture with System Columns (Priority: P1)

**Goal**: Implement Multi-Version Concurrency Control (MVCC) with `_seq` versioning and unified DML functions for user/shared tables.

**Independent Test**: Create table with user-defined PK, insert rows (auto-generate `_seq`), update/delete (append new versions), query (verify MAX(`_seq`) resolution), flush (verify Parquet deduplication). Confirm user/shared tables use same code paths.

### Foundational Infrastructure (MVCC Core)

- [X] T008 [P] [US5] Create SeqId type in backend/crates/kalamdb-commons/src/ids/seq_id.rs wrapping Snowflake ID with timestamp extraction
- [X] T009 [P] [US5] Implement SeqId methods: new(), as_i64(), timestamp_millis(), timestamp(), to_string(), from_string(), to_bytes(), from_bytes()
- [X] T010 [P] [US5] Add SeqId trait implementations: Display, From<i64>, Into<i64>, Ord, PartialOrd, Serialize, Deserialize
- [X] T011 [P] [US5] Update CREATE TABLE parser to REQUIRE user-specified primary key column (reject tables without PK)
- [X] T012 [P] [US5] Add automatic `_seq: SeqId` column injection to all user/shared table schemas during CREATE TABLE
- [X] T013 [P] [US5] Add automatic `_deleted: bool` column injection to all user/shared table schemas during CREATE TABLE
- [X] T014 [P] [US5] Update CachedTableData to include Arrow schema with `_seq` and `_deleted` columns (cache at table creation, never recompute)
- [X] T015 [US5] Refactor UserTableRow struct: Remove row_id, _updated; Keep user_id: UserId; Add _seq: SeqId, _deleted: bool, fields: JsonValue
- [X] T016 [US5] Refactor SharedTableRow struct: Remove row_id, _updated, access_level; Add _seq: SeqId, _deleted: bool, fields: JsonValue
- [X] T017 [US5] Refactor UserTableRowId to composite struct with user_id: UserId and _seq: SeqId fields, implement StorageKey trait with storage_key() method (similar to TableId pattern)
- [X] T018 [US5] Replace SharedTableRowId struct with SeqId directly (no wrapper needed)

### Unified DML Functions Module

- [X] T019 [P] [US5] Create unified_dml module in backend/crates/kalamdb-core/src/tables/unified_dml/mod.rs
- [X] T020 [P] [US5] Implement append_version() function: generate SeqId via SnowflakeGenerator, create storage key, append to RocksDB (used by INSERT/UPDATE/DELETE)
- [X] T021 [P] [US5] Implement resolve_latest_version() function: group by PK from fields JSON, apply MAX(`_seq`), filter `_deleted = false` (used by query planning)
- [X] T022 [US5] Implement validate_primary_key() function: extract PK from fields JSON, check uniqueness, enforce NOT NULL constraints (used by INSERT)
- [X] T023 [US5] Implement generate_storage_key() function: create UserTableRowId with user_id and _seq, call storage_key() method for user tables; use SeqId directly for shared tables (used by all DML)
- [X] T024 [US5] Implement extract_user_pk_value() function: parse user-defined PK column value from fields JSON (used by INSERT/UPDATE)
- [X] T025 [US5] Add comprehensive unit tests for unified_dml module (append, resolve, validate, key generation)
- [X] T026 [US5] Add SeqId ordering tests: verify SeqId comparison works correctly for MAX() operations and range scans

### INSERT Handler Integration

- [X] T027 [US5] Refactor UserTableInsertHandler to call unified_dml::append_version_sync() with fields JSON (all user columns including PK)
  - Created append_version_sync() as synchronous core implementation in unified_dml/append.rs
  - append_version() now wraps append_version_sync() for async compatibility
  - Updated UserTableInsertHandler.insert_row() to call append_version_sync() with TableId parameter
  - Fixed all 4 test sites to pass TableId::new(namespace_id.clone(), table_name.clone())
  - Updated UserTableAccess.insert_row() to pass self.shared.core().table_id()
  - Exported append_version_sync from unified_dml module
  - All 15 UserTableInsertHandler tests passing ✅
  - Build successful (0 errors, warnings only) ✅
- [X] T028 [US5] Refactor SharedTableInsertHandler to use SystemColumnsService for SeqId generation (same pattern as user tables)
  - Updated SharedTableProvider.insert() to use SystemColumnsService.handle_insert()
  - Now returns SeqId instead of () for test compatibility
  - Creates SharedTableRow with (_seq, _deleted, fields) structure
  - Stores via self.store for proper test isolation
  - Fixed 3 test sites to use returned SeqId for verification
  - 3/4 SharedTableProvider tests passing ✅ (test_insert, test_delete_hard, test_column_family_name)
  - 1 test failing: test_update (expected - UPDATE refactoring is T032-T035)
  - Build successful (0 errors, warnings only) ✅
- [X] T029 [US5] Add primary key validation before append operations in INSERT handlers
  - Added PK validation to UserTableInsertHandler.insert_row()
  - Added PK validation to SharedTableProvider.insert()
  - Uses extract_user_pk_value() to validate PK field exists and is not null
  - Gets table definition from SchemaRegistry to find PK column name
  - Fixed SchemaRegistry.get_table_definition() to use AppContext::try_get() for test compatibility
  - All 15 UserTableInsertHandler tests passing ✅
  - 3/4 SharedTableProvider tests passing ✅ (test_update expected to fail - UPDATE refactoring is T032-T035)
  - Build successful (0 errors, warnings only) ✅
  - Note: Full uniqueness checking (scanning existing PKs) deferred to T060
- [X] T030 [US5] Ensure INSERT handlers generate SeqId via SystemColumnsService and set `_deleted = false`
  - UserTableInsertHandler uses append_version_sync() which calls SystemColumnsService.generate_seq_id()
  - SharedTableProvider uses SystemColumnsService.handle_insert() which returns (SeqId, deleted=false)
  - Both handlers correctly set _deleted=false for INSERT operations
  - SeqId generation verified in all 18 passing tests (15 UserTableInsertHandler + 3 SharedTableProvider)
- [~] T031 [US5] Remove duplicate INSERT logic from user_table_insert.rs and shared_table_insert.rs (consolidate to unified_dml)
  - **Status**: DEFERRED - requires test infrastructure refactoring
  - **Reason**: SharedTableProvider tests expect `provider.store` to be the same instance as `app_context.shared_table_store()`, but test creates separate InMemoryBackend-backed store for isolation
  - **Current State**: UserTableInsertHandler uses unified_dml::append_version_sync() ✅, SharedTableProvider uses SystemColumnsService + self.store.put() (T028 pattern)
  - **Future Work**: Refactor test infrastructure to align AppContext stores with provider stores, OR add store parameter to append_version_sync()
  - **PK Validation**: Both handlers share identical PK validation logic via extract_user_pk_value() ✅

### UPDATE Handler Integration

- [ ] T032 [US5] Refactor UserTableUpdateHandler to call unified_dml::append_version() with modified fields JSON (append new version, never in-place update)
- [ ] T033 [US5] Refactor SharedTableUpdateHandler to call unified_dml::append_version() with modified fields JSON (same function as user tables)
- [ ] T034 [US5] Update UPDATE handlers to fetch existing row, modify fields JSON, generate new SeqId, and append new version
- [ ] T035 [US5] Remove duplicate UPDATE logic from user_table_update.rs and shared_table_update.rs (consolidate to unified_dml)

### DELETE Handler Integration

- [ ] T036 [US5] Refactor UserTableDeleteHandler to call unified_dml::append_version() with `_deleted = true` in new version (append new version, never in-place delete)
- [ ] T037 [US5] Refactor SharedTableDeleteHandler to call unified_dml::append_version() with `_deleted = true` (same function as user tables)
- [ ] T038 [US5] Update DELETE handlers to fetch existing row, set `_deleted = true`, generate new SeqId, and append new version
- [ ] T039 [US5] Remove duplicate DELETE logic from user_table_delete.rs and shared_table_delete.rs (consolidate to unified_dml)

### Query Planning Integration (Version Resolution)

- [ ] T040 [US5] Integrate unified_dml::resolve_latest_version() into UserTableProvider.scan() method
- [ ] T041 [US5] Integrate unified_dml::resolve_latest_version() into SharedTableProvider.scan() method (same function as user tables)
- [ ] T042 [US5] Add MAX(`_seq`) grouping logic per PK in version resolution (extract PK from fields JSON, ensure only latest version returned)
- [ ] T043 [US5] Add `WHERE _deleted = false` filtering after version resolution in scan() methods
- [ ] T044 [US5] Implement RocksDB prefix scan for UserTableStore using `{user_id}:` prefix for efficient user-specific queries
- [ ] T045 [US5] Implement RocksDB range scan for both stores using SeqId ordering for efficient `WHERE _seq > threshold` queries

### Flush Integration (Snapshot Deduplication)

- [ ] T046 [US5] Update FlushExecutor to call unified_dml::resolve_latest_version() on hot storage snapshot before writing Parquet
- [ ] T047 [US5] Ensure Parquet files contain only latest versions (MAX(`_seq`) per PK extracted from fields JSON) with `_deleted = false` filtering
- [ ] T048 [US5] Add logging: rows_before_deduplication, rows_after_deduplication, deduplication_ratio
- [ ] T049 [US5] Verify hot storage retains ALL versions after flush (flush is snapshot operation, not destructive)

### Testing & Validation

- [ ] T050 [US5] Add unit test: SeqId creation, timestamp extraction, ordering, serialization
- [ ] T051 [US5] Add unit test: CREATE TABLE without PK → rejected with error
- [ ] T052 [US5] Add unit test: CREATE TABLE with user PK → `_seq: SeqId` and `_deleted: bool` auto-added to schema
- [ ] T053 [US5] Add integration test: INSERT → verify storage key format UserTableRowId.storage_key() returns `{user_id}:{_seq}` bytes for user tables, SeqId for shared tables
- [ ] T054 [US5] Add integration test: INSERT → verify UserTableRow has user_id, _seq, _deleted, fields (no row_id, _updated)
- [ ] T055 [US5] Add integration test: INSERT to shared table → verify SharedTableRow has only _seq, _deleted, fields (no access_level)
- [ ] T056 [US5] Add integration test: UPDATE → verify new version appended with new SeqId (original version unchanged)
- [ ] T057 [US5] Add integration test: DELETE → verify new version appended with `_deleted = true` and new SeqId (original version unchanged)
- [ ] T058 [US5] Add integration test: SELECT after UPDATE → verify MAX(`_seq`) resolution returns latest version only
- [ ] T059 [US5] Add integration test: SELECT after DELETE → verify `_deleted = true` row excluded from results
- [ ] T060 [US5] Add integration test: INSERT duplicate PK → rejected with uniqueness error
- [ ] T061 [US5] Add integration test: FLUSH → verify Parquet contains deduplicated rows (MAX(`_seq`) per PK from fields JSON)
- [ ] T062 [US5] Add integration test: incremental sync `WHERE _seq > X` → returns all versions after SeqId threshold
- [ ] T063 [US5] Add integration test: RocksDB prefix scan `{user_id}:` → efficiently returns only that user's rows
- [ ] T064 [US5] Add integration test: RocksDB range scan `_seq > threshold` → efficiently skips older versions
- [ ] T065 [US5] Add code analysis test: verify user/shared INSERT handlers call identical unified_dml::append_version()
- [ ] T066 [US5] Add code analysis test: verify user/shared UPDATE handlers call identical unified_dml::append_version()
- [ ] T067 [US5] Add code analysis test: verify user/shared DELETE handlers call identical unified_dml::append_version()
- [ ] T068 [US5] Add code analysis test: measure code reduction in user_table_*.rs and shared_table_*.rs (target: 50%+ reduction)
- [ ] T069 [US5] Run performance test: INSERT throughput with SeqId generation (target: >10K ops/sec per core)
- [ ] T070 [US5] Run performance test: UPDATE/DELETE append latency (target: <5ms per operation)
- [ ] T071 [US5] Run performance test: MAX(`_seq`) query resolution with 10+ versions per PK (target: <2x baseline latency)
- [ ] T072 [US5] Run performance test: SeqId timestamp extraction overhead (target: <1μs per extraction)

**Phase 2 Summary**: MVCC architecture with SeqId versioning, minimal row structures (UserTableRow/SharedTableRow with only _seq, _deleted, fields), efficient storage keys ({user_id}:{_seq} and {_seq}), unified DML functions eliminating 50%+ duplicate code, append-only hot storage with prefix/range scan support, and snapshot deduplication on flush.

---

## Phase 2.5: Provider Consolidation (User/Shared Table DRY Refactoring) ⚠️ **CONFLICTS WITH PHASE 13**

**⚠️ CRITICAL CONFLICT**: This phase extracts shared helpers incrementally. **Phase 13** replaces the entire provider architecture with trait-based implementation. **Choose ONE approach:**

- **Option A (Phase 2.5)**: Incremental refactoring via helper extraction (~350 lines reduced, 60-70% duplication)
- **Option B (Phase 13)**: Complete architectural redesign via trait implementation (~1200 lines reduced, eliminates wrappers + duplication)

**Recommendation**: **Skip Phase 2.5, proceed directly to Phase 13** for maximum benefit (3.4× more code reduction, cleaner architecture)

**Goal**: Eliminate duplicate logic between UserTableProvider and SharedTableProvider by extracting shared helpers to base_table_provider module.

**Rationale**: Both providers share ~80% of their implementation logic. The only differences are:
1. Storage path resolution (per-user vs shared directory)
2. RocksDB scan filtering (user_id prefix vs full table)
3. User context extraction (SessionState vs none)

By extracting shared helpers with strategy parameters, we can reduce code duplication by 60-70% while maintaining type safety.

### Shared Helper Extraction ⚠️ **CONFLICTS WITH T211-T239**

- [ ] T073a [P] [US5] [**⚠️ SKIP - Phase 13**] Extract `validate_insert_rows()` from UserTableProvider to base_table_provider as public helper
- [ ] T073b [US5] [**⚠️ SKIP - Phase 13**] Update SharedTableProvider to use extracted validate_insert_rows() helper
- [ ] T074a [P] [US5] [**⚠️ SKIP - Phase 13**] Create generic `scan_rocksdb_with_filter<K, V>()` helper in base_table_provider accepting filter closure
- [ ] T074b [US5] [**⚠️ SKIP - Phase 13**] Refactor UserTableProvider.scan_rocksdb_as_batch() to call generic helper with user_id prefix filter
- [ ] T074c [US5] [**⚠️ SKIP - Phase 13**] Refactor SharedTableProvider.scan_rocksdb_as_batch() to call generic helper with no filter (full scan)
- [ ] T075a [P] [US5] [**⚠️ SKIP - Phase 13**] Create generic `scan_parquet_with_path<K, V>()` helper in base_table_provider accepting path resolver closure
- [ ] T075b [US5] [**⚠️ SKIP - Phase 13**] Refactor UserTableProvider.scan_parquet_as_batch() to call generic helper with per-user path resolver
- [ ] T075c [US5] [**⚠️ SKIP - Phase 13**] Refactor SharedTableProvider.scan_parquet_as_batch() to call generic helper with shared path resolver
- [ ] T076a [P] [US5] [**⚠️ SKIP - Phase 13**] Extract `find_row_by_id_field()` helper to base_table_provider for delete_by_id_field/update_by_id_field logic
- [ ] T076b [US5] [**⚠️ SKIP - Phase 13**] Refactor UserTableProvider.delete_by_id_field() to use find_row_by_id_field() helper
- [ ] T076c [US5] [**⚠️ SKIP - Phase 13**] Refactor SharedTableProvider.delete_by_id_field() to use find_row_by_id_field() helper
- [ ] T076d [US5] [**⚠️ SKIP - Phase 13**] Refactor UserTableProvider.update_by_id_field() to use find_row_by_field() helper
- [ ] T076e [US5] [**⚠️ SKIP - Phase 13**] Refactor SharedTableProvider.update_by_id_field() to use find_row_by_id_field() helper

### Trait-Based Abstraction (Alternative Approach) ⚠️ **CONFLICTS WITH T200-T239**

- [ ] T077a [US5] [**⚠️ SKIP - Phase 13**] Create `TableProviderDML` trait in base_table_provider with default implementations for shared DML operations
- [ ] T077b [US5] [**⚠️ SKIP - Phase 13**] Add trait methods: validate_schema(), scan_with_strategy(), delete_by_logical_id(), update_by_logical_id()
- [ ] T077c [US5] [**⚠️ SKIP - Phase 13**] Implement TableProviderDML for UserTableProvider with user-specific overrides (path resolver, scan filter)
- [ ] T077d [US5] [**⚠️ SKIP - Phase 13**] Implement TableProviderDML for SharedTableProvider with shared-specific overrides (no filter, single path)
- [ ] T077e [US5] [**⚠️ SKIP - Phase 13**] Add comprehensive unit tests for TableProviderDML default implementations

### Code Consolidation Validation ⚠️ **REPLACED BY T236-T239**

- [ ] T078 [US5] [**⚠️ SKIP - Phase 13**] Run code analysis: measure LOC reduction in UserTableProvider (target: 200+ lines removed)
- [ ] T079 [US5] [**⚠️ SKIP - Phase 13**] Run code analysis: measure LOC reduction in SharedTableProvider (target: 150+ lines removed)
- [ ] T080 [US5] [**⚠️ SKIP - Phase 13**] Verify all UserTableProvider tests still pass after consolidation
- [ ] T081 [US5] [**⚠️ SKIP - Phase 13**] Verify all SharedTableProvider tests still pass after consolidation
- [ ] T082 [US5] [**⚠️ SKIP - Phase 13**] Add integration test: verify UserTableProvider and SharedTableProvider behavior unchanged after refactoring

**Phase 2.5 Summary**: Extract 5-7 shared helpers to base_table_provider, eliminating 350+ lines of duplicate code (60-70% reduction in provider files) while maintaining full type safety and test coverage.

---

## Phase 3: User Story 1 - Consolidated with Phase 2 (MVCC Architecture)

**Note**: This phase has been consolidated into Phase 2 (User Story 5). The MVCC architecture with `_seq` versioning replaces the original append-only UPDATE/DELETE design. All tasks from Phase 3 are now covered by Phase 2's unified DML implementation.

**Original Goal**: Implement append-only UPDATE/DELETE with version resolution across fast and long-term storage.

**New Approach (Phase 2)**: 
- All DML operations (INSERT/UPDATE/DELETE) use unified `append_version()` function
- Version resolution uses MAX(`_seq`) per PK instead of MAX(`_updated`) with storage layer prioritization
- User and shared tables share identical code paths (50%+ code reduction)
- Hot storage is always append-only (zero in-place updates)
- Flush deduplicates using MAX(`_seq`) per PK before writing Parquet

**Tasks Covered in Phase 2**:
- T038-T041: SQL parser extensions → Now part of unified DML parsing
- T042-T051: UPDATE/DELETE handlers → Replaced by unified append_version() function (T026-T033 in Phase 2)
- T052-T057: Version resolution → Replaced by unified resolve_latest_version() function (T016, T034-T037 in Phase 2)
- T058-T059: Flush integration → Covered by snapshot deduplication (T038-T041 in Phase 2)
- T060-T068: Testing → Covered by MVCC integration tests (T042-T058 in Phase 2)

**See Phase 2 for complete MVCC implementation details.**

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

**Total Tasks**: 253 (was 238 + 15 provider consolidation tasks)
**User Story Breakdown**:
- Setup: 7 tasks
- **US5 (MVCC + Provider Consolidation)**: 87 tasks (was 72 + 15 provider tasks)
  - Phase 2: MVCC Architecture - 72 tasks
  - Phase 2.5: Provider Consolidation (NEW) - 15 tasks
- US1 (UPDATE/DELETE): Consolidated into Phase 2 (MVCC)
- US6 (Manifest Cache): 33 tasks
- US2 (Manifest Optimization): 27 tasks
- US3 (Bloom Filters): 16 tasks
- US4 (AS USER): 26 tasks
- US7 (Config): 12 tasks
- US8 (Job Params): 19 tasks
- Polish: 31 tasks

**Code Consolidation Targets**:
- **Phase 2 (MVCC)**: ~1200 lines → ~600 lines (50% reduction via unified DML functions)
- **Phase 2.5 (Providers)**: ~800 lines → ~450 lines (60-70% reduction via shared helpers)
- **Total Expected Reduction**: ~550 lines of duplicate code eliminated
- US5 (System Columns): 30 tasks
- US1 (UPDATE/DELETE): 31 tasks
- US6 (Manifest Cache): 33 tasks
- US2 (Manifest Optimization): 27 tasks
- US3 (Bloom Filters): 16 tasks
- US4 (AS USER): 26 tasks
- US7 (Config): 12 tasks
- US8 (Job Params): 19 tasks
- Polish: 31 tasks

**Parallel Opportunities**: ~65 tasks marked [P] (26% parallelizable, includes provider consolidation parallelism)

**MVP Scope**: Phase 2 (US5 MVCC + Provider Consolidation) = 87 tasks (34.4% of total)

**Estimated Complexity**:
- High: US1 (version resolution), US2 (manifest service), US6 (cache lifecycle)
- Medium: US3 (Bloom filters), US4 (AS USER), US5 (system columns)
- Low: US7 (config), US8 (job params)

---

## Phase 13: Provider Architecture Consolidation (US9) - SIMPLIFIED DESIGN

⚠️ **DEPENDENCY CONFLICTS IDENTIFIED**:
- **Phase 2.5 (T073-T082)**: SKIP - Phase 13 supersedes incremental helper extraction with complete trait-based redesign
- **Phase 2 Query Resolution (T040-T041)**: COMPATIBLE - scan() integration works with Phase 13 trait methods
- **Phase 4-10**: COMPATIBLE - Manifest cache, Bloom filters, AS USER, config, jobs work independently of provider architecture

**RECOMMENDATION**: Execute Phase 13 BEFORE Phase 2.5 to achieve maximum code reduction (1200 lines vs 350 lines)

**Objective**: Eliminate code duplication across User/Shared/Stream table providers by creating a unified trait-based architecture with generic storage abstraction

**Current State Analysis**:
- **UserTableProvider**: ~1460 lines with duplicate DML methods
- **SharedTableProvider**: ~915 lines with duplicate DML methods  
- **StreamTableProvider**: ~923 lines with duplicate DML methods
- **UserTableShared**: 200+ lines (singleton pattern)
- **TableProviderCore**: 130 lines (common fields)
- **BaseTableProvider trait**: 30 lines (minimal, not fully utilized)

**Total Duplication**: ~3300 lines with ~60% shared logic

**Proposed Simplified Architecture**:

```rust
/// Single unified trait with generic storage abstraction
pub trait BaseTableProvider<K: StorageKey, V>: Send + Sync + DataFusion::TableProvider {
    // Core metadata
    fn table_id(&self) -> &TableId;
    fn schema_ref(&self) -> SchemaRef;
    fn table_type(&self) -> TableType;
    
    // Storage access
    fn store(&self) -> &Arc<dyn EntityStore<K, V>>;
    fn app_context(&self) -> &Arc<AppContext>;
    
    // DML operations (unified with hot/cold storage merging)
    fn insert(&self, row_data: JsonValue) -> Result<K, KalamDbError>;
    fn insert_batch(&self, rows: Vec<JsonValue>) -> Result<Vec<K>, KalamDbError>;
    fn update(&self, key: &K, updates: JsonValue) -> Result<K, KalamDbError>;
    fn delete(&self, key: &K) -> Result<(), KalamDbError>;
    
    // Scan operations with storage merging
    fn scan_rows(&self, filter: Option<Expr>) -> Result<RecordBatch, KalamDbError>;
}

// Concrete implementations (no type aliases, no UserTableShared wrapper)
pub struct UserTableProvider {
    table_id: TableId,
    schema: SchemaRef,
    store: Arc<UserTableStore>,
    app_context: Arc<AppContext>,
    // ... user-specific fields
}

impl BaseTableProvider<UserTableRowId, UserTableRow> for UserTableProvider {
    // DML methods scan RocksDB (hot) + Parquet (cold), merge via version resolution
    fn update(&self, key: &UserTableRowId, updates: JsonValue) -> Result<UserTableRowId, KalamDbError> {
        // 1. Scan RocksDB for current version
        // 2. Scan Parquet files for older versions  
        // 3. Apply version resolution (MAX(_seq) wins)
        // 4. Merge updates into latest version
        // 5. Append new version via unified_dml::append_version_sync()
    }
}

pub struct SharedTableProvider {
    table_id: TableId,
    schema: SchemaRef,
    store: Arc<SharedTableStore>,
    app_context: Arc<AppContext>,
}

impl BaseTableProvider<SharedTableRowId, SharedTableRow> for SharedTableProvider {
    // Same DML pattern as UserTableProvider (hot+cold merge)
}

pub struct StreamTableProvider {
    table_id: TableId,
    schema: SchemaRef,
    store: Arc<StreamTableStore>,
    app_context: Arc<AppContext>,
    ttl_seconds: Option<u64>,
}

impl BaseTableProvider<StreamTableRowId, StreamTableRow> for StreamTableProvider {
    // DML methods use ONLY hot storage (RocksDB), NO Parquet merging
    // Stream tables are ephemeral with TTL-based eviction
}
```

**Key Simplifications**:
1. ❌ No UserTableShared wrapper (providers hold fields directly)
2. ❌ No TableProviderCore (merge fields into providers)
3. ❌ No type aliases (implement trait directly)
4. ✅ UserId extracted from context, not passed as parameter
5. ✅ Single trait, three implementations
6. ✅ DataFusion TableProvider integration (same struct serves both)

**Storage Merging Strategy**:
- **User/Shared Tables**: DML operations scan RocksDB (hot) + Parquet (cold), apply version resolution (MAX(_seq)), then operate on latest version
- **Stream Tables**: DML operations use ONLY RocksDB (hot storage), NO Parquet merging (ephemeral data with TTL eviction)

**Expected Code Reduction**: ~1200 lines (400 wrappers + 800 DML duplication)

### Tasks

#### Phase 13.1: Design & Trait Definition (5 tasks)

- [ ] T200 [P] Design BaseTableProvider trait signature with K: StorageKey and V row type generics
- [ ] T201 [P] Identify provider-specific methods (scan with RLS filters, TTL eviction, etc.)
- [ ] T202 Define core trait methods (table_id, schema_ref, table_type, store, app_context)
- [ ] T203 Define DML trait methods (insert, insert_batch, update, delete with generic return types)
- [ ] T204 Define scan trait methods (scan_rows with optional filter parameter, hot+cold merge for User/Shared, hot-only for Stream)

#### Phase 13.2: StreamTableStore Refactoring (6 tasks)

- [ ] T205 Update StreamTableRow struct to include user_id, _seq, fields (remove event_id, timestamp)
- [ ] T206 Update StreamTableRowId to composite struct with user_id and _seq (similar to UserTableRowId)
- [ ] T207 Update stream_table_store.rs to use new row structure with MVCC architecture
- [ ] T208 Update StreamTableProvider.insert_event to use SystemColumnsService for SeqId generation
- [ ] T209 Update all stream table tests to use new row structure
- [ ] T210 Verify StreamTableStore builds successfully with 0 errors

#### Phase 13.3: UserTableProvider Implementation (8 tasks)

- [ ] T211 Remove UserTableShared wrapper, move fields directly to UserTableProvider
- [ ] T212 Remove TableProviderCore, merge fields (table_id, schema) into UserTableProvider
- [ ] T213 Implement BaseTableProvider<UserTableRowId, UserTableRow> for UserTableProvider
- [ ] T214 Implement core trait methods (table_id, schema_ref, table_type, store, app_context)
- [ ] T215 Implement DML methods (insert, insert_batch, update, delete) with RocksDB+Parquet merging via scan_with_version_resolution_to_kvs()
- [ ] T216 Implement scan_rows with RLS filtering (user_id scoping) and hot+cold storage merge
- [ ] T217 Update all UserTableProvider call sites (remove UserTableShared references)
- [ ] T218 Verify UserTableProvider builds successfully with 0 errors

#### Phase 13.4: SharedTableProvider Implementation (7 tasks)

- [ ] T219 Remove TableProviderCore wrapper, move fields directly to SharedTableProvider
- [ ] T220 Implement BaseTableProvider<SharedTableRowId, SharedTableRow> for SharedTableProvider
- [ ] T221 Implement core trait methods (table_id, schema_ref, table_type, store, app_context)
- [ ] T222 Implement DML methods (insert, update, delete) with RocksDB+Parquet merging via scan_with_version_resolution_to_kvs()
- [ ] T223 Implement scan_rows without RLS filtering and hot+cold storage merge
- [ ] T224 Update all SharedTableProvider call sites
- [ ] T225 Verify SharedTableProvider builds successfully with 0 errors

#### Phase 13.5: StreamTableProvider Implementation (7 tasks)

- [ ] T226 Remove TableProviderCore wrapper, move fields directly to StreamTableProvider
- [ ] T227 Implement BaseTableProvider<StreamTableRowId, StreamTableRow> for StreamTableProvider
- [ ] T228 Implement core trait methods (table_id, schema_ref, table_type, store, app_context)
- [ ] T229 Implement DML methods (insert, insert_batch) using ONLY RocksDB (hot storage), NO Parquet merging
- [ ] T230 Implement scan_rows with TTL filtering (evict expired events) using ONLY RocksDB
- [ ] T231 Update all StreamTableProvider call sites
- [ ] T232 Verify StreamTableProvider builds successfully with 0 errors

#### Phase 13.6: Cleanup & Testing (7 tasks)

- [ ] T233 Delete UserTableShared struct and all references
- [ ] T234 Delete TableProviderCore struct and all references
- [ ] T235 Delete old BaseTableProvider trait (replace with new generic one)
- [ ] T236 Run cargo check on entire workspace
- [ ] T237 Fix all compilation errors in batch
- [ ] T238 Run all provider tests (user/shared/stream)
- [ ] T239 Measure code reduction (expected ~1200 lines)
- [ ] T201e [US9] Update SchemaRegistry cache: `insert_user_table_shared` → `insert_user_table_commons`
- [ ] T201f [US9] Update all test code to use new naming

#### T202: Make BaseTableCommons Generic Over RowId and Store Types
- [ ] T202a [US9] Add generic parameters to BaseTableCommons: `BaseTableCommons<RowId, Store>`
- [ ] T202b [US9] Add trait bounds: `where RowId: StorageKey, Store: EntityStore<RowId, ...>`
- [ ] T202c [US9] Update insert_handler/update_handler/delete_handler to use generic RowId type
- [ ] T202d [US9] Create type aliases: UserTableCommons, SharedTableCommons, StreamTableCommons
- [ ] T202e [US9] Update UserTableProvider to use UserTableCommons type alias
- [ ] T202f [US9] Verify compilation with generic constraints (cargo check)

#### T203: Enhance BaseTableProvider Trait with DML Methods
- [ ] T203a [US9] Add insert() method signature to BaseTableProvider trait
- [ ] T203b [US9] Add insert_batch() method signature to BaseTableProvider trait
- [ ] T203c [US9] Add update() method signature to BaseTableProvider trait
- [ ] T203d [US9] Add update_batch() method signature to BaseTableProvider trait
- [ ] T203e [US9] Add delete() method signature to BaseTableProvider trait
- [ ] T203f [US9] Add delete_batch() method signature to BaseTableProvider trait
- [ ] T203g [US9] Add scan_rows() method signature to BaseTableProvider trait (provider-specific)
- [ ] T203h [US9] Add optional user_id parameter to all DML methods for RLS support

#### T204: Implement Generic DML Methods in BaseTableCommons
- [ ] T204a [US9] Implement insert() in BaseTableCommons delegating to insert_handler
- [ ] T204b [US9] Implement insert_batch() using insert() in loop (or batch handler if available)
- [ ] T204c [US9] Implement update() delegating to update_handler with generic RowId
- [ ] T204d [US9] Implement update_batch() using update() in loop
- [ ] T204e [US9] Implement delete() delegating to delete_handler with generic RowId
- [ ] T204f [US9] Implement delete_batch() using delete() in loop
- [ ] T204g [US9] Add user_id scoping logic for UserTableCommons (key prefix filtering)
- [ ] T204h [US9] Verify SharedTableCommons ignores user_id parameter (no RLS)

#### T205: Migrate UserTableProvider to Use BaseTableProvider Trait Methods
- [ ] T205a [US9] Replace UserTableProvider.insert_row() implementation with trait method call
- [ ] T205b [US9] Replace UserTableProvider.insert_batch() implementation with trait method call
- [ ] T205c [US9] Replace UserTableProvider.update_row() implementation with trait method call
- [ ] T205d [US9] Replace UserTableProvider.update_batch() implementation with trait method call
- [ ] T205e [US9] Replace UserTableProvider.delete_row() implementation with trait method call
- [ ] T205f [US9] Replace UserTableProvider.delete_batch() implementation with trait method call
- [ ] T205g [US9] Keep provider-specific methods: delete_by_id_field, update_by_id_field (not in base trait)
- [ ] T205h [US9] Run user table tests to verify migration (15 INSERT tests + UPDATE/DELETE tests)

#### T206: Migrate SharedTableProvider to Use BaseTableProvider Trait Methods
- [ ] T206a [US9] Create SharedTableCommons instance (similar to UserTableCommons)
- [ ] T206b [US9] Replace SharedTableProvider.insert() with BaseTableProvider.insert() trait call
- [ ] T206c [US9] Replace SharedTableProvider.update() with BaseTableProvider.update() trait call
- [ ] T206d [US9] Replace SharedTableProvider.delete_soft/delete_hard with unified delete() (controlled by _deleted flag)
- [ ] T206e [US9] Keep provider-specific methods: delete_by_id_field, update_by_id_field
- [ ] T206f [US9] Update SharedTableProvider to store Arc<SharedTableCommons> instead of individual fields
- [ ] T206g [US9] Run shared table tests to verify migration (4 provider tests)

#### T207: Migrate StreamTableProvider to Use BaseTableProvider Trait Methods
- [ ] T207a [US9] Create StreamTableCommons instance with StreamRowId and StreamTableStore
- [ ] T207b [US9] Replace StreamTableProvider.insert_event() with BaseTableProvider.insert() trait call
- [ ] T207c [US9] Replace StreamTableProvider.insert_batch() with BaseTableProvider.insert_batch() trait call
- [ ] T207d [US9] Keep stream-specific methods: evict_expired, count_events, get_event, scan_events (not in base trait)
- [ ] T207e [US9] Add ephemeral mode logic to insert() implementation (check subscribers before storing)
- [ ] T207f [US9] Update StreamTableProvider to store Arc<StreamTableCommons> instead of individual fields
- [ ] T207g [US9] Run stream table tests to verify migration

#### T208: Consolidate Metadata Access Methods
- [ ] T208a [US9] Move namespace_id() implementation to TableProviderCore (already delegates to core)
- [ ] T208b [US9] Move table_name() implementation to TableProviderCore (already delegates to core)
- [ ] T208c [US9] Move column_family_name() to TableProviderCore with table_type switch logic
- [ ] T208d [US9] Remove duplicate namespace_id/table_name implementations from all 3 providers
- [ ] T208e [US9] Update all providers to use core.namespace(), core.table_name(), core.column_family_name()

#### T209: Eliminate TableId Usage Where NamespaceId + TableName Suffice
- [ ] T209a [US9] Audit all method signatures using (NamespaceId, TableName) separately
- [ ] T209b [US9] Replace (namespace_id, table_name) parameter pairs with single table_id: &TableId
- [ ] T209c [US9] Update INSERT/UPDATE/DELETE handlers to accept &TableId instead of separate namespace/table
- [ ] T209d [US9] Update all test call sites to use TableId::new(namespace, table_name)
- [ ] T209e [US9] Verify no regressions in handler tests (run full test suite)

#### T210: Code Cleanup & Validation
- [ ] T210a [US9] Remove old UserTableShared references (if any remain after rename)
- [ ] T210b [US9] Remove duplicate insert/update/delete method implementations from providers
- [ ] T210c [US9] Run cargo clippy to identify remaining duplication or unused code
- [ ] T210d [US9] Run full test suite: `cargo test --package kalamdb-core --lib`
- [ ] T210e [US9] Measure code reduction: `git diff --stat` before/after (target: 800-1000 lines removed)
- [ ] T210f [US9] Update AGENTS.md with new architecture: BaseTableCommons generic pattern
- [ ] T210g [US9] Update docs/architecture/ with provider consolidation explanation

#### T211: Performance & Integration Testing
- [ ] T211a [US9] Run smoke tests to verify DML operations work across all 3 table types
- [ ] T211b [US9] Verify RLS enforcement in UserTableProvider (user_id scoping still works)
- [ ] T211c [US9] Verify shared tables ignore user_id parameter (no RLS leakage)
- [ ] T211d [US9] Verify stream tables ephemeral mode + TTL eviction still work
- [ ] T211e [US9] Run performance benchmarks: INSERT/UPDATE/DELETE throughput unchanged
- [ ] T211f [US9] Verify LiveQueryManager notifications still fire for all 3 table types

### Summary

- **Total Tasks**: 60 tasks (T200-T211)
- **Expected Code Reduction**: 800-1000 lines (60-70% of DML handler code)
- **Breaking Changes**: 
  - UserTableShared → BaseTableCommons rename (type alias for compatibility)
  - Handlers now generic over RowId type
  - Providers use Arc<BaseTableCommons> instead of individual handler fields
- **Non-Breaking**:
  - Provider-specific methods preserved (evict_expired, delete_by_id_field, etc.)
  - All tests should pass without modification (except type alias updates)
  - RLS behavior unchanged for UserTableProvider
- **Validation**: Full test suite + smoke tests + performance benchmarks

---

## Task Statistics (Updated with Phase 13)

⚠️ **CONFLICT RESOLUTION**: Phase 2.5 (T073-T082) tasks marked as SKIP - superseded by Phase 13

- **Total Tasks**: 313 tasks
  - **Active**: 273 tasks (253 original + 60 Phase 13 - 40 Phase 2.5 conflicts)
  - **Skipped due to conflicts**: 40 tasks (Phase 2.5: T073-T082)
  
- **Total Expected Code Reduction**: ~1400 lines
  - Phase 13 alone: ~1200 lines (800 DML + 400 wrappers)
  - Other phases: ~200 lines (query helpers, etc.)
  - Phase 2.5 (SKIPPED): Would have been ~350 lines (now redundant)

- **User Story Breakdown**:
  - US5 (System Columns): 30 tasks ✅ COMPATIBLE with Phase 13
  - US1 (UPDATE/DELETE): 31 tasks ✅ COMPATIBLE (T040-T041 integrate with trait methods)
  - US9 (Provider Consolidation): 60 tasks (Phase 13) ⚠️ SUPERSEDES Phase 2.5
  - US6 (Manifest Cache): 33 tasks ✅ COMPATIBLE
  - US2 (Manifest Optimization): 27 tasks ✅ COMPATIBLE
  - US3 (Bloom Filters): 16 tasks ✅ COMPATIBLE
  - US4 (AS USER): 26 tasks ✅ COMPATIBLE
  - US7 (Config): 12 tasks ✅ COMPATIBLE
  - US8 (Job Params): 19 tasks ✅ COMPATIBLE
  - Polish: 31 tasks ✅ COMPATIBLE

**Parallel Opportunities**: ~65 tasks marked [P] (26% parallelizable, includes provider consolidation parallelism)

**MVP Scope**: Phase 2 (US5 MVCC) + Phase 13 (US9 Consolidation) = 90 tasks (32.9% of active tasks)
- US7 (Config): 12 tasks
- US8 (Job Params): 19 tasks
- **US9 (Provider Consolidation): 60 tasks** ← NEW
- Polish: 31 tasks

**Parallel Opportunities**: ~80 tasks marked [P] (25.5% parallelizable)

**MVP Scope**: Phase 2 (US5 MVCC + US9 Provider Consolidation) = 147 tasks (47% of total)

**Estimated Complexity**:
- High: US1 (version resolution), US2 (manifest service), US6 (cache lifecycle), **US9 (provider generics)**
- Medium: US3 (Bloom filters), US4 (AS USER), US5 (system columns)
- Low: US7 (config), US8 (job params)
