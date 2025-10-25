# KalamDB Implementation Priorities - Next Steps

**Date**: 2025-10-23  
**Current Status**: US2 Complete (Testing Blocked) | US1, US3-US16 Pending

---

## 🎯 Executive Summary

**Completed User Stories**: 
- ✅ **US2**: Automatic Table Flushing (100% implementation, testing blocked by WebSocket errors)
- ✅ **IS9**: SQL Migration Complete (kalamdb-sql crate)
- ✅ **US6**: Storage ID Implementation Complete

**Blocking Issues**:
- ⚠️ WebSocket session compilation errors prevent all integration test execution

**Next Priorities** (sorted by impact):
1. **Fix WebSocket compilation errors** (unblocks all testing)
2. **US1**: Parametrized Queries (critical for security)
3. **US3**: Manual Table Flushing (completes flush feature set)
4. **US12**: Memory Leak and Stress Testing (validates stability)
5. **US15**: SQL Functions (DEFAULT NOW(), SNOWFLAKE_ID(), UUID_V7(), ULID())
6. **US14**: API Versioning and Deprecations (production hardening)

---

## 🚨 Critical Blocker (Affects All Testing)

### WebSocket Session Compilation Errors
**Priority**: 🔴 URGENT  
**Impact**: Blocks all integration tests (US2, US11, and future tests)  
**Effort**: 1-2 hours

**File**: `/backend/crates/kalamdb-api/src/actors/ws_session.rs`

**Errors**:
```rust
error[E0425]: cannot find value `sub_id` in this scope
error[E0282]: type annotations needed for `Box<_>`
```

**What needs fixing**:
- Undefined `sub_id` variable references
- Missing type annotations in WebSocket message handling
- Likely related to recent SUBSCRIBE TO SQL command integration

**Tasks**:
- [ ] Debug `sub_id` undefined references
- [ ] Add explicit type annotations where needed
- [ ] Verify WebSocket session handler compiles
- [ ] Run `cargo build` to confirm compilation success
- [ ] Execute US2 tests: `cargo test --test test_automatic_flushing_comprehensive`

**Expected Outcome**: 32/32 US2 integration tests passing

---

## 📋 Incomplete User Stories (By Priority)

### Priority 1: Security & Core Functionality

#### 🔐 **US1: Parametrized Queries** (Priority: P0 - Critical)
**Why**: Prevents SQL injection vulnerabilities  
**Status**: 0% (0/22 tasks complete)  
**Effort**: ~2-3 days

**What it does**:
- Implement parameterized query API: `EXECUTE "SELECT * FROM users WHERE id = $1" WITH [user_id_value]`
- Add query plan caching for prepared statements
- Support DataFusion parameter binding
- Add HTTP API endpoint: `POST /api/v1/query` with `parameters` field

**Key Tasks** (22 total):
- [ ] T001-T011: Parser implementation (ParameterizedQuery struct, $1, $2 syntax)
- [ ] T012-T017: Executor with parameter binding and validation
- [ ] T018-T022: HTTP API integration and integration tests

**Why Critical**: 
- Current system vulnerable to SQL injection
- Required for production security compliance
- Enables efficient query plan reuse

**Files to Create**:
- `/backend/crates/kalamdb-sql/src/parametrized_queries.rs` - Parser
- `/backend/crates/kalamdb-core/src/query_cache.rs` - Plan cache
- `/backend/crates/kalamdb-api/src/handlers/parametrized_query_handler.rs` - HTTP API

---

#### ⚡ **US3: Manual Table Flushing** (Priority: P1 - High)
**Why**: Completes flush feature set (automatic + manual control)  
**Status**: 0% (0/23 tasks complete)  
**Effort**: ~1 day

**What it does**:
- `FLUSH TABLE namespace.table_name` - Trigger immediate flush
- `FLUSH ALL TABLES` - Flush all user tables
- Returns job_id for monitoring via system.jobs table
- Asynchronous execution using existing JobManager

**Key Tasks** (23 total):
- [ ] T237-T242: SQL command parsing (FLUSH TABLE, FLUSH ALL TABLES)
- [ ] T243-T248: Executor integration with JobManager
- [ ] T249-T259: Integration tests (manual flush, job monitoring, error handling)

**Why High Priority**:
- Builds on completed US2 infrastructure
- Minimal new code required (reuses FlushScheduler/JobManager)
- Enables manual data archival workflows
- Required for maintenance operations

**Files to Create**:
- `/backend/crates/kalamdb-sql/src/flush_commands.rs` - Parser
- Update `/backend/crates/kalamdb-core/src/sql/executor.rs` - Add FLUSH execution

---

### Priority 2: Production Stability

#### 🧪 **US12: Memory Leak and Stress Testing** (Priority: P1 - High)
**Why**: Validates production readiness under load  
**Status**: 38% (5/13 tasks complete)  
**Effort**: ~2 days

**What it does**:
- Run 10 concurrent writers + 20 WebSocket listeners for 5+ minutes
- Monitor memory/CPU usage with periodic measurements
- Detect memory leaks, connection leaks, performance degradation
- Verify graceful degradation under extreme load

**Completed** (5 tasks):
- [X] T219: Test file created (`test_stress_and_memory.rs`)
- [X] T229: Stress test utilities created (`stress_utils.rs`)
- [X] T232-T234: Memory/CPU monitoring, benchmarks

**Remaining** (8 tasks):
- [ ] T220: test_memory_stability_under_write_load
- [ ] T221: test_concurrent_writers_and_listeners
- [ ] T222-T228: CPU usage, WebSocket leak detection, query performance under stress

**Why High Priority**:
- Uncovers critical bugs before production
- Validates automatic flushing under sustained load
- Tests WebSocket subscription stability (US11)
- Identifies resource leaks early

**Files to Update**:
- `/backend/tests/integration/test_stress_and_memory.rs` - Add remaining tests
- `/backend/benches/stress.rs` - Expand benchmarks

---

### Priority 3: Essential Features

#### 🧮 **US15: SQL Functions (DEFAULT, SELECT, WHERE)** (Priority: P2 - Medium)
**Why**: Required for production schema design  
**Status**: 13% (13/99 tasks complete)  
**Effort**: ~5-6 days (largest user story)

**What it does**:
- **DEFAULT functions**: `DEFAULT NOW()`, `DEFAULT SNOWFLAKE_ID()`, `DEFAULT UUID_V7()`, `DEFAULT ULID()`
- **PRIMARY KEY requirements**: Enforce PK on all tables (except system tables)
- **NOT NULL enforcement**: Validate before RocksDB writes
- **SELECT * column order**: Preserve creation order using ordinal_position
- **Function evaluation**: Unified registry, DataFusion integration

**Completed** (13 tasks):
- [X] T489-T493: TableDefinition architecture, system.tables schema
- [X] T533-NEW1 to NEW17: Column metadata storage in system.tables

**Remaining** (86 tasks):
- [ ] T465-T488: Integration tests (24 tests)
- [ ] T489-T532: Implementation (DEFAULT functions, PK validation, NOT NULL)
- [ ] T567-T580: Documentation (14 ADRs and rustdocs)

**Why Medium Priority**:
- Many features already implemented (TableDefinition, schema storage)
- Remaining work is mostly testing and documentation
- Required for real-world applications (timestamp tracking, unique IDs)
- Blocking table creation workflows (PK requirement)

**Key Features**:
1. **SNOWFLAKE_ID()**: 64-bit time-sortable ID (41-bit timestamp + 10-bit node + 12-bit sequence)
2. **UUID_V7()**: RFC 9562 compliant (48-bit timestamp + 80-bit random)
3. **ULID()**: 26-char Crockford base32, time-sortable
4. **NOW()**: Server-side timestamp generation

**Files to Create**:
- `/backend/crates/kalamdb-core/src/functions/default_functions.rs` - Function implementations
- `/backend/crates/kalamdb-core/src/functions/registry.rs` - Unified function registry
- `/backend/tests/integration/test_schema_integrity.rs` - Integration tests
- ADRs: ADR-013 through ADR-016

---

#### 📦 **US14: API Versioning and Deprecations** (Priority: P2 - Medium)
**Why**: Ensures backward compatibility and clean API evolution  
**Status**: 54% (13/24 tasks complete)  
**Effort**: ~2 days

**What it does**:
- Remove deprecated syntax: `CREATE TABLE ... TABLE_TYPE 'shared'` (use `CREATE SHARED TABLE`)
- Remove deprecated syntax: `CREATE USER ... OWNER_ID` (auto-assigned)
- Rename system.storage_locations → system.storages (already done)
- Add parser validation errors with helpful migration messages
- Document credential security in storage abstraction

**Completed** (13 tasks):
- [X] T044a-T073a: system.storage_locations renamed, tests updated
- [X] T076a-T079a: system.storages implementation

**Remaining** (11 tasks):
- [ ] T038a-T043a: Move executor to kalamdb-sql crate (6 tasks)
- [ ] T082a-T088a: Reject deprecated syntax (7 tasks)
- [ ] T074a-T081a: Documentation updates (4 tasks)

**Why Medium Priority**:
- Most migrations already complete (system.storages)
- Remaining work is syntax validation and docs
- Prevents technical debt accumulation
- Cleans up API for production release

**Files to Update**:
- `/backend/crates/kalamdb-sql/src/ddl/user_management.rs` - Reject OWNER_ID
- `/backend/crates/kalamdb-sql/src/ddl/table_commands.rs` - Reject TABLE_TYPE
- `/backend/tests/integration/test_api_versioning.rs` - Add validation tests

---

### Priority 4: Advanced Features

#### 📊 **US16: Data Type System Consolidation** (Priority: P2 - Medium)
**Why**: Ensures type safety across RocksDB, Arrow, and SQL layers  
**Status**: 0% (0/54 tasks complete)  
**Effort**: ~3-4 days

**What it does**:
- **10 core types**: Boolean, Int, BigInt, Double, Text, Timestamp, Date, Time, Json, Bytes
- **RocksDB encoding**: Type tag byte + payload (e.g., [0x03][8 bytes LE] for BigInt)
- **Arrow integration**: KalamDbType ↔ ArrowDataType roundtrip conversions
- **SQL aliases**: Support BOOL, INT4, VARCHAR, BIGSERIAL, etc.
- **NULL encoding**: Reserved tag [0xFF]

**Key Tasks** (54 total):
- [ ] T494-T499: Core type enum and conversion tests (6 tests)
- [ ] T500-T517: RocksDB encoding roundtrip tests (18 tests)
- [ ] T518-T534: INSERT/SELECT integration tests (17 tests)
- [ ] T554-T566: Implementation (type aliases, validation, encoding)

**Why Medium Priority**:
- Foundation likely already implemented (types working in current system)
- Primarily testing and validation work
- Required for production data integrity guarantees
- Prevents type confusion bugs

**Files to Create**:
- `/backend/tests/integration/test_data_type_core.rs` - Type enum tests
- `/backend/tests/integration/test_data_type_rocksdb.rs` - Encoding tests
- `/backend/tests/integration/test_data_type_insert_query.rs` - Integration tests
- `/backend/crates/kalamdb-commons/src/types/encoding.rs` - RocksDB encoding logic

---

### Priority 5: Optimization & Polish

#### 🔄 **US4: Flush Partial User Partitions** (Priority: P3 - Low)
**Why**: Optimization for large user bases  
**Status**: 0% (0/19 tasks complete)  
**Effort**: ~2 days

**What it does**:
- Flush subset of users when total flush would exceed time limits
- Configurable batch size (e.g., flush 1000 users at a time)
- Resume tracking to avoid skipping users
- Prevents flush job timeouts on massive tables

**Why Low Priority**:
- Optimization, not critical functionality
- Current full-table flush works for moderate scale
- Can defer until production usage patterns known

---

#### 📝 **US5: Flush Status Tracking** (Priority: P3 - Low)
**Why**: Observability for flush operations  
**Status**: 0% (0/17 tasks complete)  
**Effort**: ~1-2 days

**What it does**:
- system.flush_status table with per-table tracking
- Columns: last_flush_time, next_scheduled_flush, rows_since_flush, flush_count
- SELECT query for monitoring flush health
- Metrics for alerting

**Why Low Priority**:
- Observability enhancement
- Can use system.jobs table for basic monitoring
- Defer until production monitoring requirements clear

---

#### 🧹 **US7-US10: Flush Enhancements** (Priority: P3 - Low)
**Status**: 0% complete  
**Effort**: ~1-2 days each

- **US7**: Purge Old Parquet Files (retention policies)
- **US8**: Pause/Resume Flush Scheduler (maintenance mode)
- **US10**: Flush Configuration via SQL (ALTER FLUSH SETTINGS)

**Why Low Priority**:
- Operational conveniences, not core functionality
- Can be added incrementally based on production needs

---

### Priority 6: Live Query Testing (Depends on WebSocket Fix)

#### 📡 **US11: Live Query Change Detection Integration Testing** (Priority: P1 - High)
**Why**: Validates core real-time feature  
**Status**: 100% implementation, testing blocked  
**Effort**: ~1 day (after WebSocket fix)

**What it does**:
- Test WebSocket subscriptions detecting INSERT/UPDATE/DELETE
- Verify change notifications delivered to all listeners
- Test high-frequency changes (1000 inserts rapidly)
- Validate no message loss under concurrent writes

**Completed**:
- [X] T195-T218m: All implementation and test infrastructure complete
- [X] Real WebSocket client with JWT authentication
- [X] SUBSCRIBE TO SQL command fully documented

**Blocked By**:
- ⚠️ WebSocket session compilation errors (same as US2)

**Remaining**:
- [ ] Execute 10 integration tests (marked #[ignore])
- [ ] Fix HTTP 500 error when connecting to WebSocket endpoint

**Why High Priority**:
- Core differentiator for KalamDB (real-time data sync)
- Already fully implemented, just needs testing
- WebSocket fix unblocks both US2 and US11 tests

---

#### 🔍 **US13: Flush Error Handling and Recovery** (Priority: P2 - Medium)
**Why**: Production resilience  
**Status**: 0% (0/17 tasks complete)  
**Effort**: ~1-2 days

**What it does**:
- Retry logic for transient S3 failures (network errors)
- Dead letter queue for failed flush jobs
- Alerting on consecutive failures
- Automatic recovery from partial flush failures

**Why Medium Priority**:
- Required for S3 production deployment
- Current local filesystem flush has minimal failure modes
- Can implement incrementally as S3 usage grows

---

## 📊 Overall Progress Summary

| User Story | Priority | Status | Tasks Complete | Effort | Blockers |
|------------|----------|--------|----------------|--------|----------|
| **US2** | P0 | ✅ 100% Impl | 58/58 (Tests blocked) | - | WebSocket errors |
| **US1** | P0 | 🔴 Not started | 0/22 | 2-3 days | None |
| **US3** | P1 | 🔴 Not started | 0/23 | 1 day | None |
| **US12** | P1 | 🟡 38% | 5/13 | 2 days | WebSocket errors |
| **US11** | P1 | ✅ 100% Impl | Testing blocked | 1 day | WebSocket errors |
| **US15** | P2 | 🟡 13% | 13/99 | 5-6 days | None |
| **US14** | P2 | 🟡 54% | 13/24 | 2 days | None |
| **US16** | P2 | 🔴 Not started | 0/54 | 3-4 days | None |
| **US13** | P2 | 🔴 Not started | 0/17 | 1-2 days | None |
| **US4-US10** | P3 | 🔴 Not started | 0/~70 | 1-2 days each | None |

**Legend**:
- ✅ Complete
- 🟡 In Progress
- 🔴 Not Started

---

## 🎯 Recommended Next Steps (Prioritized)

### Week 1: Unblock Testing & Critical Security

1. **Day 1: Fix WebSocket Compilation Errors** (🔴 URGENT)
   - Debug ws_session.rs errors
   - Verify compilation success
   - Execute US2 tests (32 tests expected passing)
   - Execute US11 tests (10 tests expected passing)
   - **Outcome**: Unblocks all integration testing

2. **Days 2-4: Implement US1 (Parametrized Queries)** (🔴 CRITICAL)
   - Prevents SQL injection vulnerabilities
   - Required for production security
   - **Outcome**: Production-ready query API

3. **Day 5: Implement US3 (Manual Flushing)** (🟡 HIGH)
   - Completes flush feature set
   - Reuses US2 infrastructure (minimal effort)
   - **Outcome**: Full flush control (automatic + manual)

### Week 2: Production Stability & Features

4. **Days 6-7: Complete US12 (Stress Testing)** (🟡 HIGH)
   - Validate memory/CPU stability
   - Detect resource leaks before production
   - **Outcome**: Confidence in production readiness

5. **Days 8-10: Implement US15 (SQL Functions)** (🟡 MEDIUM)
   - DEFAULT NOW(), SNOWFLAKE_ID(), UUID_V7(), ULID()
   - PRIMARY KEY enforcement
   - **Outcome**: Production-grade schema design

### Week 3: Polish & Advanced Features

6. **Days 11-12: Complete US14 (API Versioning)** (🟡 MEDIUM)
   - Remove deprecated syntax
   - Clean API for production
   - **Outcome**: Backward compatibility locked in

7. **Days 13-15: Implement US16 (Data Type System)** (🔴 MEDIUM)
   - Comprehensive type validation
   - RocksDB encoding tests
   - **Outcome**: Production data integrity guarantees

### Optional (Future Sprints)

8. **US13**: Flush Error Handling (for S3 production)
9. **US4-US10**: Flush optimizations and enhancements

---

## 📈 Success Metrics

**After Week 1** (Critical Path):
- ✅ All integration tests passing (US2, US11, US12)
- ✅ Parametrized queries prevent SQL injection
- ✅ Manual flush commands available

**After Week 2** (Production Ready):
- ✅ System stable under 5+ minute stress test
- ✅ SQL functions enable real-world schemas
- ✅ Memory usage predictable and bounded

**After Week 3** (Production Hardened):
- ✅ API versioning prevents breaking changes
- ✅ Data type system validated with comprehensive tests
- ✅ All P0-P2 user stories complete

---

## 🚀 Deployment Readiness Checklist

**Before Production**:
- [ ] WebSocket errors fixed (blocks testing)
- [ ] US1 (Parametrized Queries) complete (security)
- [ ] US12 (Stress Testing) passing (stability)
- [ ] US2 integration tests passing (32/32)
- [ ] US11 integration tests passing (10/10)
- [ ] Documentation complete for all features

**Nice to Have**:
- [ ] US3 (Manual Flushing)
- [ ] US15 (SQL Functions)
- [ ] US14 (API Versioning cleanup)
- [ ] US16 (Data Type validation)

---

## 📞 Questions / Decisions Needed

1. **WebSocket Fix**: Should we prioritize fixing WebSocket errors immediately, or work around by testing manually?
   - **Recommendation**: Fix immediately (1-2 hours) to unblock all automated testing

2. **US1 Scope**: Should parametrized queries support named parameters (`$user_id`) or only positional (`$1, $2`)?
   - **Recommendation**: Start with positional (simpler), add named later

3. **US15 Scope**: Should we implement all 4 ID functions (SNOWFLAKE_ID, UUID_V7, ULID, NOW) or prioritize subset?
   - **Recommendation**: Implement all 4 (unique value propositions, relatively simple)

4. **US12 Thresholds**: What memory growth % and CPU usage % should trigger test failures?
   - **Recommendation**: <10% memory growth, <80% CPU usage (per current spec)

---

**Last Updated**: 2025-10-23  
**Maintainer**: KalamDB Core Team  
**Next Review**: After WebSocket fix completion
