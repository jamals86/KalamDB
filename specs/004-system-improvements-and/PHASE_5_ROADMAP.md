# Phase 5 Implementation Roadmap
**Feature**: User Story 2 - Automatic Table Flushing with Job Management  
**Branch**: `004-system-improvements-and`  
**Created**: October 22, 2025  
**Total Tasks**: 58 remaining (T137-T194c)

## Overview

Phase 5 focuses on completing the automatic table flushing system with:
- Streaming flush operations (memory-efficient, RocksDB snapshot-based)
- Robust job persistence and crash recovery
- Multi-storage backend support (filesystem, S3)
- Comprehensive integration testing
- Production-ready documentation

## Current State Analysis

### ✅ Already Implemented (Prerequisites Complete)
- **FlushScheduler** (T145): `backend/crates/kalamdb-core/src/scheduler.rs`
- **JobManager Trait** (T146a): `backend/crates/kalamdb-core/src/jobs/job_manager.rs`
- **Sharding Strategies** (T147, T153-T154): Alphabetic, Numeric, ConsistentHash
- **Basic Flush Jobs**: UserTableFlushJob, SharedTableFlushJob
- **KILL JOB Command** (T158a-T158c): Parsing and execution complete
- **Trigger Monitoring** (T148-T149a): Time-based and row-count triggers

### ⚠️ Current Limitations Requiring Phase 5
1. **Memory Issues**: Current flush loads all users' data into memory (scalability bottleneck)
2. **No Crash Recovery**: Jobs lost on restart; no state persistence to system.jobs
3. **Single Storage**: Hardcoded filesystem paths; no S3 or multi-storage support
4. **Path Template Issues**: Non-standard filenames; no shard resolution
5. **No Job Coordination**: Duplicate flush jobs possible; no graceful shutdown
6. **Missing Tests**: No integration tests for automatic flushing
7. **Incomplete Documentation**: Missing ADRs and API reference updates

## Implementation Phases

### Phase 5A: Core Flush Improvements (Foundation) ⭐ PRIORITY
**Goal**: Memory-efficient streaming flush with proper path templates  
**Duration**: 2-3 hours | **Tasks**: T151-T152e (13 tasks) | **Risk**: Medium

#### Dependencies
- ✅ UserTableFlushJob exists
- ✅ ParquetWriter available
- ✅ ShardingStrategy registry exists

#### Task Breakdown
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T151 | Refactor execute_flush() method signature | user_table_flush.rs | 15m |
| T151a | Add RocksDB snapshot creation at flush start | user_table_flush.rs | 20m |
| T151b | Implement sequential CF scan (no load to memory) | user_table_flush.rs | 30m |
| T151c | Accumulate rows for current userId in streaming buffer | user_table_flush.rs | 25m |
| T151d | Detect userId boundary (trigger Parquet write) | user_table_flush.rs | 20m |
| T151e | Write accumulated rows to Parquet before continuing | user_table_flush.rs | 15m |
| T151f | Delete successfully flushed rows (atomic per-user batch) | user_table_flush.rs | 25m |
| T151g | On Parquet write failure, keep buffered rows (error handling) | user_table_flush.rs | 15m |
| T151h | Track per-user flush success/failure with logging | user_table_flush.rs | 15m |
| T152 | Create path template substitution function | user_table_flush.rs | 20m |
| T152a | Implement ISO 8601 timestamp filename generator | user_table_flush.rs | 15m |
| T152b | Resolve {shard} variable using table's sharding strategy | user_table_flush.rs | 20m |
| T152c | Handle empty {shard} when sharding not configured | user_table_flush.rs | 10m |
| T152d | Validate all required template variables before mkdir | user_table_flush.rs | 15m |
| T152e | Fail fast with clear error for undefined variables | user_table_flush.rs | 10m |

#### Deliverables
- Streaming flush implementation (processes one user at a time)
- RocksDB snapshot consistency (prevents data loss during concurrent writes)
- ISO 8601 Parquet filenames: `YYYY-MM-DDTHH-MM-SS.parquet`
- Path template validation with clear error messages
- Atomic per-user deletion (failed users remain in buffer)

#### Testing Strategy
- Unit tests: Streaming buffer logic, userId boundary detection
- Manual test: Insert 100K rows for 10 users, verify memory stays constant during flush

#### Risk Mitigation
- **Risk**: RocksDB snapshot API complexity
  - **Mitigation**: Review existing RocksDB snapshot usage in codebase
- **Risk**: Parquet write failure mid-flush
  - **Mitigation**: Per-user atomicity ensures partial success

---

### Phase 5B: Job Persistence & Recovery (Reliability) ⭐ PRIORITY
**Goal**: Crash recovery, duplicate prevention, graceful shutdown  
**Duration**: 2-3 hours | **Tasks**: T158d-T158s (16 tasks) | **Risk**: High

#### Dependencies
- ⚠️ Requires Phase 5A (streaming flush)
- ✅ system.jobs table exists with required columns
- ✅ JobManager trait defined

#### Task Breakdown
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T158d | Write job status to system.jobs BEFORE starting work | user_table_flush.rs | 20m |
| T158e | Query system.jobs on startup for incomplete jobs | scheduler.rs | 30m |
| T158e-b | Resume incomplete jobs after crash | scheduler.rs | 25m |
| T158f | Check system.jobs for running flush on same table | scheduler.rs | 20m |
| T158g | Return existing job_id instead of creating duplicate | scheduler.rs | 15m |
| T158h | Query system.jobs for active flush jobs at shutdown | main.rs / scheduler.rs | 25m |
| T158i | Wait for active jobs with configurable timeout | scheduler.rs | 30m |
| T158j | Add flush_job_shutdown_timeout_seconds to config.toml | config.toml / config.rs | 15m |
| T158k | Add DEBUG logging for flush start | user_table_flush.rs | 10m |
| T158l | Add DEBUG logging for flush completion | user_table_flush.rs | 10m |
| T158m | Use system.jobs as source of truth (not in-memory state) | scheduler.rs / job_manager.rs | 30m |
| T158n | Optimize system.jobs CF: Enable block cache | Column family config | 20m |
| T158o | Configure system.jobs CF with 256MB block cache | RocksDB initialization | 15m |
| T158p | Implement scheduled job cleanup logic | New: job_cleanup.rs | 40m |
| T158q | Add job_retention_days config parameter | config.toml / config.rs | 10m |
| T158r | Add job_cleanup_schedule config parameter | config.toml / config.rs | 10m |
| T158s | Create cleanup job (delete old records from system.jobs) | job_cleanup.rs | 35m |

#### Deliverables
- Crash recovery: Incomplete jobs resume after restart
- Duplicate prevention: Same table cannot have multiple concurrent flush jobs
- Graceful shutdown: Server waits up to 5 minutes for active flushes
- System.jobs optimization: 256MB block cache for fast queries
- Automated cleanup: Delete jobs older than 30 days (configurable)
- Debug logging: Track flush lifecycle for troubleshooting

#### Testing Strategy
- Integration test: Start flush, kill server, restart, verify job resumes
- Integration test: Start flush, attempt duplicate, verify error
- Integration test: Start flush, shutdown signal, verify wait for completion

#### Risk Mitigation
- **Risk**: Zombie jobs (marked running but actually dead)
  - **Mitigation**: Timeout-based cleanup; node_id tracking for cluster awareness
- **Risk**: Shutdown timeout too short (data loss)
  - **Mitigation**: Configurable timeout with sane default (300s)
- **Risk**: system.jobs queries slow down flush operations
  - **Mitigation**: Dedicated block cache (256MB), query optimization

---

### Phase 5C: Storage Management System (Multi-Storage) 🔧 ENHANCEMENT
**Goal**: Support multiple storage backends (filesystem, S3)  
**Duration**: 3-4 hours | **Tasks**: T163-T193 (63 tasks) | **Risk**: High

#### Dependencies
- ⚠️ Requires Phase 5A (path template logic)
- ✅ kalamdb-commons crate exists

#### Task Breakdown

##### C1: System Tables (T163-T163c)
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T163 | Create system.storages table schema | system_tables/storages.rs | 30m |
| T163a | Create StorageType enum in kalamdb-commons | commons/models.rs | 15m |
| T163b | Add storage_id column to system.tables (FK constraint) | system_tables/tables.rs | 20m |
| T163c | Add storage_mode and storage_id to system.users | system_tables/users.rs | 20m |

##### C2: Default Storage (T164-T164a)
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T164 | Insert default storage_id='local' on server startup | main.rs | 25m |
| T164a | Read default_storage_path from config.toml fallback | config.rs | 15m |

##### C3: Storage Registry (T165-T166c)
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T165 | Create StorageRegistry struct | storage/storage_registry.rs | 40m |
| T165a | Implement get_storage(storage_id) method | storage_registry.rs | 25m |
| T165b | Implement list_storages() with ordering | storage_registry.rs | 20m |
| T166 | Implement validate_template() method | storage_registry.rs | 30m |
| T166a | Validate shared template: {namespace} before {tableName} | storage_registry.rs | 15m |
| T166b | Validate user template: {namespace}→{tableName}→{shard}→{userId} | storage_registry.rs | 20m |
| T166c | Validate user template: {userId} required | storage_registry.rs | 10m |

##### C4: DDL Updates (T167-T168a)
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T167 | Add STORAGE parameter to CREATE TABLE DDL | sql/ddl_parser.rs | 25m |
| T167a | Default to storage_id='local' when omitted | ddl_parser.rs | 10m |
| T167b | Validate storage_id exists (FK validation) | ddl_parser.rs | 15m |
| T167c | Enforce NOT NULL on storage_id for user tables | ddl_parser.rs | 10m |
| T168 | Add USE_USER_STORAGE boolean option to DDL | ddl_parser.rs | 20m |
| T168a | Store use_user_storage flag in system.tables | system_tables/tables.rs | 15m |

##### C5: Storage Lookup Chain (T169-T170c)
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T169 | Create resolve_storage_for_user() in FlushJob | user_table_flush.rs | 35m |
| T169a-e | Implement 5-step lookup chain logic | user_table_flush.rs | 45m |
| T170 | Update FlushJob path resolution to use StorageRegistry | user_table_flush.rs | 30m |
| T170a | Replace hardcoded {storageLocation} with base_directory | user_table_flush.rs | 15m |
| T170b | Use Storage.user_tables_template based on table type | user_table_flush.rs | 20m |
| T170c | Validate template variable ordering during generation | user_table_flush.rs | 15m |

##### C6: S3 Backend (T171-T171c)
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T171 | Create S3Storage implementation file | storage/s3_storage.rs | 30m |
| T171a | Add aws-sdk-s3 dependency to Cargo.toml | kalamdb-store/Cargo.toml | 10m |
| T171b | Implement S3Storage::write_parquet() | s3_storage.rs | 45m |
| T171c | Implement S3Storage::read_parquet() | s3_storage.rs | 40m |

##### C7: Storage SQL Commands (T173-T173d)
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T173 | Create storage_commands.rs file | sql/storage_commands.rs | 20m |
| T173a | Implement CREATE STORAGE command parsing | storage_commands.rs | 35m |
| T173b | Implement ALTER STORAGE command parsing | storage_commands.rs | 30m |
| T173c | Implement DROP STORAGE command parsing | storage_commands.rs | 25m |
| T173d | Implement SHOW STORAGES command parsing | storage_commands.rs | 20m |

##### C8: Referential Integrity (T172-T172d)
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T172 | Implement DELETE FROM system.storages logic | system_tables/storages.rs | 30m |
| T172a | Query system.tables for COUNT(*) WHERE storage_id | storages.rs | 20m |
| T172b | Return error with table count if references exist | storages.rs | 15m |
| T172c | Include up to 10 table names in error message | storages.rs | 20m |
| T172d | Hardcoded protection: Prevent deleting storage_id='local' | storages.rs | 10m |

#### Deliverables
- system.storages table with storage configuration
- StorageRegistry for multi-storage management
- S3 storage backend (write/read Parquet to S3)
- CREATE/ALTER/DROP/SHOW STORAGES SQL commands
- Storage lookup chain (user storage mode override)
- Path template validation with ordering enforcement
- Referential integrity protection

#### Testing Strategy
- Integration tests (T174-T193): 20 tests for storage management
- Manual test: Create S3 storage, flush table, verify upload to S3

#### Risk Mitigation
- **Risk**: S3 authentication complexity
  - **Mitigation**: Use aws-sdk-s3 with standard credential chain
- **Risk**: Path template validation too restrictive
  - **Mitigation**: Allow flexible templates with clear validation rules
- **Risk**: Storage lookup chain confusing
  - **Mitigation**: Comprehensive documentation with diagrams

---

### Phase 5D: DDL & Server Integration (Wiring) 🔌 INTEGRATION
**Goal**: Wire FlushScheduler into server startup with DDL parameters  
**Duration**: 1 hour | **Tasks**: T155-T156 (4 tasks) | **Risk**: Low

#### Dependencies
- ⚠️ Requires Phase 5A (flush improvements)
- ⚠️ Requires Phase 5B (job persistence)
- ✅ FlushScheduler already exists

#### Task Breakdown
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T155 | Add flush_interval parameter to CREATE TABLE DDL | sql/ddl_parser.rs | 15m |
| T155a | Add flush_row_threshold parameter to CREATE TABLE DDL | sql/ddl_parser.rs | 15m |
| T155b | Validate at least one flush trigger is configured | ddl_parser.rs | 10m |
| T156 | Initialize FlushScheduler in main.rs | kalamdb-server/main.rs | 20m |

#### Deliverables
- CREATE TABLE with flush parameters: `FLUSH INTERVAL 60s ROW_THRESHOLD 1000`
- FlushScheduler started in server initialization
- Validation: At least one trigger (interval OR row count) required

#### Testing Strategy
- Integration test: CREATE TABLE without flush params, verify error
- Integration test: CREATE TABLE with both params, verify scheduling

---

### Phase 5E: Integration Testing (Verification) ✅ QUALITY
**Goal**: Comprehensive test coverage for automatic flushing  
**Duration**: 3-4 hours | **Tasks**: T137-T193 (57 tests) | **Risk**: Low

#### Dependencies
- ⚠️ Requires Phase 5A-D complete

#### Task Breakdown

##### E1: Automatic Flushing Tests (T137-T144c) - 15 tests
| Test | Description | File | Estimate |
|------|-------------|------|----------|
| T137 | Create test_automatic_flushing.rs file | tests/integration/ | 15m |
| T138-T138d | Time-based and row-count trigger tests (5 tests) | test_automatic_flushing.rs | 60m |
| T139-T144c | Multi-user, path templates, sharding, job cancellation (10 tests) | test_automatic_flushing.rs | 120m |

##### E2: Storage Management Tests (T174-T193) - 20 tests
| Test | Description | File | Estimate |
|------|-------------|------|----------|
| T174-T180 | Storage CRUD and validation tests (7 tests) | test_storage_management.rs | 70m |
| T181-T193 | Storage lookup chain and multi-storage (13 tests) | test_storage_management.rs | 130m |

#### Deliverables
- 15 automatic flushing integration tests
- 20 storage management integration tests
- Test coverage: Trigger logic, path templates, sharding, job lifecycle, storage CRUD

---

### Phase 5F: Documentation (Quality) 📚 POLISH
**Goal**: Production-ready documentation for flush system  
**Duration**: 1-2 hours | **Tasks**: T159-T194c (15 tasks) | **Risk**: Low

#### Dependencies
- ⚠️ Requires Phase 5A-E complete (implementation done)

#### Task Breakdown
| Task | Description | File | Estimate |
|------|-------------|------|----------|
| T159 | Add rustdoc to FlushScheduler | scheduler.rs | 20m |
| T160 | Add rustdoc to ShardingStrategy trait | sharding.rs | 15m |
| T161-T161c | Add inline comments to JobManager and FlushJob (4 items) | job_manager.rs, user_table_flush.rs | 40m |
| T162-T162b | Create ADR-006-flush-execution.md (3 items) | docs/architecture/adrs/ | 30m |
| T194-T194c | Create ADR-007-storage-registry.md (4 items) | docs/architecture/adrs/ | 30m |

#### Deliverables
- Comprehensive rustdoc for public APIs
- ADR-006: Streaming flush design with RocksDB snapshots
- ADR-007: Multi-storage architecture and lookup chain
- API_REFERENCE.md updates for storage commands

---

## Execution Strategy

### Recommended Order
1. **Phase 5A** (Foundation) - Must complete first; unblocks B, C, D
2. **Phase 5B** (Reliability) - Can run parallel with Phase 5C after 5A
3. **Phase 5C** (Multi-Storage) - Can run parallel with Phase 5B after 5A
4. **Phase 5D** (Wiring) - Requires 5A, 5B complete
5. **Phase 5E** (Testing) - Requires 5A-D complete
6. **Phase 5F** (Docs) - Requires 5A-E complete; can start earlier for completed phases

### Dependency Graph
```
Phase 5A (Foundation)
    ├─→ Phase 5B (Reliability)
    │       └─→ Phase 5D (Wiring)
    │               └─→ Phase 5E (Testing)
    │                       └─→ Phase 5F (Docs)
    └─→ Phase 5C (Multi-Storage)
            └─→ Phase 5D (Wiring)
```

### Parallel Execution Opportunities
- **After Phase 5A**: Start 5B and 5C in parallel (different files)
- **During Phase 5E**: Start writing ADRs (Phase 5F) for completed features

### Critical Path
Phase 5A → Phase 5B → Phase 5D → Phase 5E → Phase 5F (12-15 hours total)

### Optional/Deferrable
Phase 5C (Multi-Storage) can be deferred if S3 support not immediately needed. This reduces critical path to 8-10 hours.

---

## Risk Assessment

### High Risk Items
1. **RocksDB Snapshot Streaming** (T151a-T151h)
   - Complexity: Medium-High
   - Impact if failed: Flush remains memory-bound
   - Mitigation: Thorough unit testing; fallback to current implementation

2. **Crash Recovery** (T158e)
   - Complexity: High
   - Impact if failed: Jobs lost on restart
   - Mitigation: System.jobs as source of truth; comprehensive integration tests

3. **S3 Backend** (T171b-T171c)
   - Complexity: Medium-High
   - Impact if failed: No cloud storage support
   - Mitigation: Deferrable; can implement later

### Medium Risk Items
1. **Storage Lookup Chain** (T169-T169e)
   - Complexity: Medium
   - Documentation-heavy to prevent confusion
   
2. **Path Template Validation** (T166-T166c)
   - Complexity: Medium
   - Edge cases may emerge during testing

### Low Risk Items
- DDL parameter additions (T155-T155b)
- Server integration (T156)
- Integration tests (T137-T193)
- Documentation (T159-T194c)

---

## Success Criteria

### Phase 5A Complete
- [ ] Flush processes 100K rows for 10 users with constant memory (<100MB growth)
- [ ] RocksDB snapshot prevents data loss during concurrent writes
- [ ] Parquet filenames use ISO 8601 format (YYYY-MM-DDTHH-MM-SS.parquet)
- [ ] Path template validation catches invalid configurations

### Phase 5B Complete
- [ ] Server restart resumes incomplete flush jobs
- [ ] Duplicate flush jobs on same table are prevented
- [ ] Graceful shutdown waits up to 300s for active jobs
- [ ] System.jobs queries <10ms (p95) with 256MB cache

### Phase 5C Complete
- [ ] System.storages table supports filesystem and S3 storage types
- [ ] StorageRegistry correctly resolves storage based on lookup chain
- [ ] S3 backend successfully writes/reads Parquet files
- [ ] CREATE/DROP STORAGE commands work with referential integrity

### Phase 5D Complete
- [ ] CREATE TABLE accepts flush_interval and flush_row_threshold
- [ ] FlushScheduler automatically schedules tables created with flush params
- [ ] Server starts FlushScheduler without errors

### Phase 5E Complete
- [ ] 35 integration tests passing (100% pass rate)
- [ ] Test coverage includes: triggers, multi-user, sharding, storage lookup, S3

### Phase 5F Complete
- [ ] All public APIs have rustdoc with examples
- [ ] ADR-006 and ADR-007 merged to docs/architecture/adrs/
- [ ] API_REFERENCE.md includes storage management commands

---

## Estimated Timeline

| Phase | Duration | Cumulative | Can Start After |
|-------|----------|------------|------------------|
| 5A    | 2-3h     | 3h         | Immediately      |
| 5B    | 2-3h     | 6h         | 5A complete      |
| 5C    | 3-4h     | 10h        | 5A complete      |
| 5D    | 1h       | 11h        | 5A, 5B complete  |
| 5E    | 3-4h     | 15h        | 5A-D complete    |
| 5F    | 1-2h     | 17h        | 5A-E complete    |

**Total Estimated Time**: 12-17 hours (depends on parallel execution)

**With Parallel Execution** (5B and 5C after 5A): 12-14 hours

**Without Phase 5C** (defer S3 support): 8-10 hours

---

## Next Steps

### Immediate Actions
1. **Review and Approve Roadmap**: Confirm phases, estimates, and scope
2. **Choose Execution Mode**:
   - **Full Implementation**: All phases (12-17 hours)
   - **MVP Implementation**: Phases 5A, 5B, 5D, 5E, 5F (8-10 hours, skip multi-storage)
   - **Phased Delivery**: Complete 5A first, then decide on 5B vs 5C priority

3. **Set Up Environment**:
   - Review existing RocksDB snapshot API usage
   - Verify aws-sdk-s3 compatibility with project
   - Prepare test environment for integration tests

4. **Start Implementation**: Begin with Phase 5A (Foundation)

### Questions for Approval
1. **Scope Decision**: Full implementation or defer Phase 5C (multi-storage)?
2. **Priority**: If parallel execution, prioritize 5B (reliability) or 5C (multi-storage)?
3. **Testing Strategy**: Write tests alongside implementation or after phases complete?
4. **Documentation**: Write ADRs during implementation or batch at end (Phase 5F)?

---

## Appendix: Task Cross-Reference

### Phase 5A Tasks
T151, T151a-T151h, T152, T152a-T152e (13 tasks)

### Phase 5B Tasks
T158d-T158s (16 tasks)

### Phase 5C Tasks
T163-T173d, T174-T193 (63 tasks - includes integration tests)

### Phase 5D Tasks
T155-T156 (4 tasks)

### Phase 5E Tasks
T137-T144c (15 tests - automatic flushing)
T174-T193 (20 tests - storage management, counted in 5C)

### Phase 5F Tasks
T159-T162b, T194-T194c (15 tasks)

**Total**: 111 tasks across 6 phases
