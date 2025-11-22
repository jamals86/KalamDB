# KalamDB Test Coverage Analysis

**Date**: November 22, 2025  
**Branch**: 012-full-dml-support  
**Purpose**: Map current test coverage against features in SQL.md and CLI.md

---

## Executive Summary

### Current Coverage Status

| Feature Category | Coverage | Tests | Gaps |
|-----------------|----------|-------|------|
| **Table Types** | 🟢 Good | USER/SHARED/STREAM smoke tests | Missing WITH(...) syntax verification |
| **Flush Policies** | 🟢 Good | smoke_test_flush_operations.rs | Missing combined policies, manifest checks |
| **DML Operations** | 🟡 Partial | Basic CRUD in smoke tests | Missing batch INSERT, complex UPDATE/DELETE |
| **Custom Functions** | 🔴 Missing | None | SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER defaults |
| **Subscriptions** | 🟢 Good | smoke_test_user_table_subscription.rs | Missing SUBSCRIBE TO SQL syntax |
| **System Tables** | 🟡 Partial | smoke_test_system_and_users.rs | Missing system.tables options column checks |
| **Performance/Timing** | 🔴 Missing | None | No "Took: X.XXX ms" parsing tests |
| **HTTP API Limits** | 🔴 Missing | None | No tests for /api/v1/query limitations |

---

## Feature Matrix

### 2.1 Table Types with CREATE TABLE ... WITH (...)

| Feature | Documented | CLI Test | Backend Test | Gaps |
|---------|-----------|----------|--------------|------|
| `CREATE TABLE ... WITH (TYPE='USER')` | ✅ SQL.md L388-395 | ✅ smoke tests | ❌ | Missing explicit WITH syntax test |
| `CREATE TABLE ... WITH (TYPE='SHARED')` | ✅ SQL.md L402-410 | ✅ smoke_test_shared_table_crud.rs | ❌ | Missing ACCESS_LEVEL verification |
| `CREATE TABLE ... WITH (TYPE='STREAM')` | ✅ SQL.md L417-424 | ✅ smoke_test_stream_subscription.rs | ❌ | Good coverage |
| System columns `_updated`, `_deleted` for USER/SHARED | ✅ SQL.md L1256-1280 | ❌ | ✅ test_soft_delete.rs | Need CLI verification |
| No system columns for STREAM | ✅ SQL.md L1289 | ❌ | ❌ | Not tested |
| DML for USER tables (INSERT/UPDATE/DELETE/SELECT) | ✅ SQL.md L471-584 | ✅ smoke tests | ✅ multiple | Good coverage |
| DML for SHARED tables | ✅ SQL.md L471-584 | ✅ smoke_test_shared_table_crud.rs | ✅ test_shared_access.rs | Good coverage |
| DML for STREAM tables | ✅ SQL.md L471-584 | ✅ smoke_test_stream_subscription.rs | ❌ | Missing hard delete test |

**Priority Additions**:
1. ✅ Test explicit `CREATE TABLE ... WITH (TYPE='...', STORAGE_ID='...', FLUSH_POLICY='...')` parsing
2. Test `ACCESS_LEVEL` options for SHARED tables
3. Verify system columns (`_updated`, `_deleted`) are auto-managed and NOT user-modifiable
4. Test that STREAM tables do NOT have `_updated`/`_deleted` columns

---

### 2.2 Flush Policies & Manual Flushing

| Feature | Documented | CLI Test | Backend Test | Gaps |
|---------|-----------|----------|--------------|------|
| `FLUSH_POLICY='rows:N'` | ✅ SQL.md L375 | ✅ smoke_test_flush_operations.rs | ❌ | Good coverage |
| `FLUSH_POLICY='interval:S'` | ✅ SQL.md L375 | ❌ | ❌ | **Not tested** |
| `FLUSH_POLICY='rows:N,interval:S'` | ✅ SQL.md L375 | ❌ | ❌ | **Not tested** |
| `FLUSH TABLE <namespace>.<table>` | ✅ SQL.md L601-610 | ✅ smoke_test_flush_operations.rs | ❌ | Good coverage |
| `FLUSH ALL TABLES IN <namespace>` | ✅ SQL.md L631-643 | ❌ | ❌ | **Not tested** |
| `FLUSH ALL TABLES` (session namespace) | ✅ SQL.md L631-643 | ❌ | ❌ | **Not tested** |
| Error: FLUSH on STREAM table | ✅ SQL.md L652-656 | ❌ | ❌ | **Not tested** |
| Error: FLUSH on non-existent table | ✅ SQL.md L665-668 | ❌ | ❌ | **Not tested** |
| Error: Concurrent flush detection | ✅ SQL.md L657-663 | ❌ | ❌ | **Not tested** |
| Manifest.json existence after flush | ✅ README.md L58-67 | ❌ | ❌ | **Not tested** |
| Batch-*.parquet files exist | ✅ README.md L58-67 | ❌ | ❌ | **Not tested** |
| Manifest updated on second flush | ✅ README.md L58-67 | ❌ | ❌ | **Not tested** |

**Priority Additions**:
1. ❌ Test time-based `FLUSH_POLICY='interval:60'` (tricky in tests, use small intervals)
2. ❌ Test combined `FLUSH_POLICY='rows:100,interval:60'`
3. ❌ Test `FLUSH ALL TABLES IN <namespace>`
4. ❌ Test error cases (FLUSH on STREAM, non-existent table, concurrent flush)
5. ❌ **Filesystem checks**: verify manifest.json and batch-*.parquet exist after flush

---

### 2.3 DDL & DML Coverage

| Feature | Documented | CLI Test | Backend Test | Gaps |
|---------|-----------|----------|--------------|------|
| `CREATE NAMESPACE` / `DROP NAMESPACE` | ✅ SQL.md L79-93 | ✅ All smoke tests | ❌ | Good coverage |
| `CREATE TABLE` with PRIMARY KEY | ✅ SQL.md L350-424 | ✅ smoke tests | ✅ multiple | Good coverage |
| `ALTER TABLE ADD COLUMN` | ✅ SQL.md L438-441 | ❌ | ✅ test_schema_consolidation.rs | Need CLI test |
| `ALTER TABLE DROP COLUMN` | ✅ SQL.md L443-446 | ❌ | ❌ | **Not tested** |
| `ALTER TABLE MODIFY COLUMN` | ✅ SQL.md L448-451 | ❌ | ❌ | **Not tested** |
| `ALTER TABLE SET TBLPROPERTIES (ACCESS_LEVEL=...)` | ✅ SQL.md L453-460 | ❌ | ❌ | **Not tested** (SHARED only) |
| Error: ADD NOT NULL column without DEFAULT on non-empty table | ✅ SQL.md L440 | ❌ | ❌ | **Not tested** |
| Error: ALTER system columns (`_updated`, `_deleted`) | ✅ Implied SQL.md L1259 | ❌ | ❌ | **Not tested** |
| `INSERT` single row | ✅ SQL.md L471-487 | ✅ All smoke tests | ✅ multiple | Good coverage |
| `INSERT` multi-row batch `VALUES (...), (...), (...)` | ✅ SQL.md L475-487 | ❌ | ❌ | **Not tested** |
| `UPDATE` with WHERE | ✅ SQL.md L494-519 | ✅ smoke_test_all_datatypes.rs | ✅ test_update_delete_version_resolution.rs | Good coverage |
| `UPDATE` multi-row | ✅ SQL.md L509-511 | ❌ | ❌ | **Not tested** |
| `DELETE` (soft delete for USER/SHARED) | ✅ SQL.md L526-542 | ❌ | ✅ test_soft_delete.rs | Need CLI test |
| `DELETE` (hard delete for STREAM) | ✅ SQL.md L544-547 | ❌ | ❌ | **Not tested** |
| `SELECT` with WHERE, ORDER BY, LIMIT | ✅ SQL.md L554-584 | ✅ smoke tests | ✅ multiple | Good coverage |
| Aggregation (COUNT, SUM, GROUP BY) | ✅ SQL.md L574-576 | ❌ | ❌ | **Not tested** |

**Priority Additions**:
1. ❌ Test `ALTER TABLE ADD COLUMN` via CLI
2. ❌ Test `ALTER TABLE DROP COLUMN` (both CLI and backend)
3. ❌ Test `ALTER TABLE MODIFY COLUMN`
4. ❌ Test `ALTER TABLE SET TBLPROPERTIES` for SHARED tables
5. ❌ Test multi-row `INSERT VALUES (...), (...), (...)`
6. ❌ Test soft vs hard DELETE for USER/SHARED vs STREAM
7. ❌ Test aggregation queries (COUNT, SUM, GROUP BY)
8. ❌ Test error: ADD NOT NULL column without DEFAULT

---

### 2.4 Custom Functions

| Feature | Documented | CLI Test | Backend Test | Gaps |
|---------|-----------|----------|--------------|------|
| `SNOWFLAKE_ID()` in DEFAULT | ✅ SQL.md L1593-1652 | ❌ | ❌ | **Not tested** |
| `UUID_V7()` in DEFAULT | ✅ SQL.md L1654-1728 | ❌ | ❌ | **Not tested** |
| `ULID()` in DEFAULT | ✅ SQL.md L1730-1808 | ❌ | ❌ | **Not tested** |
| `CURRENT_USER()` in DEFAULT | ✅ SQL.md L1810-1875 | ❌ | ❌ | **Not tested** |
| `NOW()` in DEFAULT | ✅ SQL.md L1292-1299 | ✅ All smoke tests | ✅ multiple | Good coverage |
| SNOWFLAKE_ID time-ordering | ✅ SQL.md L1598-1604 | ❌ | ❌ | **Not tested** |
| UUID_V7 format (8-4-4-4-12) | ✅ SQL.md L1661 | ❌ | ❌ | **Not tested** |
| ULID format (26 chars, base32) | ✅ SQL.md L1737 | ❌ | ❌ | **Not tested** |

**Priority Additions**:
1. ❌ **Test SNOWFLAKE_ID() DEFAULT**: Create table, insert without ID, verify non-null + unique + monotonic
2. ❌ **Test UUID_V7() DEFAULT**: Verify UUID format and time-ordering
3. ❌ **Test ULID() DEFAULT**: Verify 26-char base32 format
4. ❌ **Test CURRENT_USER() DEFAULT**: Verify created_by = session user
5. ❌ Test all functions used in SELECT queries (not just DEFAULTs)

---

### 2.5 Subscriptions

| Feature | Documented | CLI Test | Backend Test | Gaps |
|---------|-----------|----------|--------------|------|
| `SUBSCRIBE TO <ns>.<table>` HTTP SQL | ✅ SQL.md L750-766 | ❌ | ❌ | **Not tested** |
| `SUBSCRIBE TO ... WHERE ...` | ✅ SQL.md L771-789 | ❌ | ❌ | **Not tested** |
| `SUBSCRIBE TO ... OPTIONS (last_rows=N)` | ✅ SQL.md L771-789 | ❌ | ❌ | **Not tested** |
| WebSocket protocol (subscription message) | ✅ SQL.md L809-848 | ✅ smoke_test_user_table_subscription.rs | ❌ | Good coverage (via CLI) |
| Initial data message | ✅ SQL.md L836-848 | ✅ smoke_test_user_table_subscription.rs | ❌ | Good coverage |
| Change notifications (INSERT/UPDATE/DELETE) | ✅ SQL.md L850-890 | ✅ smoke tests | ❌ | Good coverage |
| `\subscribe` CLI meta-command | ✅ CLI.md L77 | ✅ smoke tests | N/A | Good coverage |
| Error: SUBSCRIBE without namespace | ✅ SQL.md L951-954 | ❌ | ❌ | **Not tested** |
| Error: SUBSCRIBE to non-existent table | ✅ SQL.md L968-971 | ❌ | ❌ | **Not tested** |
| `system.live_queries` table | ✅ SQL.md L926-934 | ❌ | ❌ | **Not tested** |

**Priority Additions**:
1. ❌ **Test HTTP SUBSCRIBE TO**: POST /v1/api/sql with SUBSCRIBE TO syntax, verify ws_url in response
2. ❌ Test SUBSCRIBE with WHERE filter
3. ❌ Test SUBSCRIBE with OPTIONS (last_rows=10)
4. ❌ Test error cases (missing namespace, non-existent table)
5. ❌ Test `system.live_queries` query after establishing subscription

---

### 2.6 System Tables

| Feature | Documented | CLI Test | Backend Test | Gaps |
|---------|-----------|----------|--------------|------|
| `system.tables` query | ✅ CLI.md L75 | ✅ smoke_test_system_and_users.rs | ❌ | Good coverage |
| `system.tables.options` JSON column | ✅ SQL.md (implied) | ❌ | ❌ | **Not tested** |
| Verify `options` contains TYPE, STORAGE_ID, FLUSH_POLICY | N/A (architecture) | ❌ | ❌ | **Not tested** |
| Verify `options` contains TTL_SECONDS for STREAM | N/A (architecture) | ❌ | ❌ | **Not tested** |
| `system.live_queries` query | ✅ SQL.md L926-934 | ❌ | ❌ | **Not tested** |
| `system.stats` query (via `\stats`) | ✅ CLI.md L222-284 | ❌ | ❌ | **Not tested** |
| `\dt` meta-command | ✅ CLI.md L75 | ❌ | ❌ | Need explicit test |
| `\d <table>` meta-command | ✅ CLI.md L76 | ❌ | ❌ | Need explicit test |
| `\stats` meta-command | ✅ CLI.md L77 | ❌ | ❌ | Need explicit test |

**Priority Additions**:
1. ❌ **Test system.tables**: Query after CREATE TABLE, verify table_type, options JSON
2. ❌ Verify `options` JSON contains TYPE, STORAGE_ID, FLUSH_POLICY, TTL_SECONDS
3. ❌ Test `system.live_queries` after establishing subscription
4. ❌ Test `\stats` meta-command and parse output
5. ❌ Test `\dt` and `\d <table>` meta-commands

---

### 2.7 Performance & Timing

| Feature | Documented | CLI Test | Backend Test | Gaps |
|---------|-----------|----------|--------------|------|
| CLI prints "Took: X.XXX ms" | ✅ CLI.md L146-152 | ❌ | N/A | **Not tested** |
| Timing for small table (~10 rows) | N/A (smoke test idea) | ❌ | ❌ | **Not tested** |
| Timing for medium table (~1,000 rows) | N/A (smoke test idea) | ❌ | ❌ | **Not tested** |
| Timing for large table (~10,000 rows) | N/A (smoke test idea) | ❌ | ❌ | **Not tested** |

**Priority Additions**:
1. ❌ **smoke_test_queries_benchmark.rs extension**: Parse "Took: X.XXX ms" from CLI output
2. ❌ Run same query at 3 table sizes, log timings (no strict thresholds to avoid flakiness)
3. ❌ Optionally output timings to CSV/JSON for manual inspection

---

### 2.8 HTTP API Limitations

| Feature | Documented | CLI Test | Backend Test | Gaps |
|---------|-----------|----------|--------------|------|
| `/api/v1/query` supports INSERT only | ✅ TESTING_SQL_API.md L12-13 | N/A | ❌ | **Not tested** |
| `/api/v1/query` supports SELECT only | ✅ TESTING_SQL_API.md L12-13 | N/A | ❌ | **Not tested** |
| Error: UPDATE via /api/v1/query | ✅ TESTING_SQL_API.md L124-129 | N/A | ❌ | **Not tested** |
| Error: DELETE via /api/v1/query | ✅ TESTING_SQL_API.md L124-129 | N/A | ❌ | **Not tested** |
| Error: CREATE TABLE via /api/v1/query | ✅ TESTING_SQL_API.md L124-129 | N/A | ❌ | **Not tested** |

**Priority Additions**:
1. ❌ **backend/tests/integration/test_api_query_limits.rs**: Test that UPDATE/DELETE/CREATE/DROP return error
2. ❌ Verify error message matches docs: "Only INSERT and SELECT statements are supported"

---

## Test File Organization

### Current Files (Good)
- ✅ `cli/tests/smoke/*.rs` - CLI integration tests
- ✅ `backend/tests/test_*.rs` - Backend integration tests
- ✅ `backend/tests/integration/*.rs` - HTTP API tests

### Suggested New Files
- ❌ `cli/tests/smoke/smoke_test_custom_functions.rs` - SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER
- ❌ `cli/tests/smoke/smoke_test_ddl_alter.rs` - ALTER TABLE operations
- ❌ `cli/tests/smoke/smoke_test_system_tables.rs` - system.tables, system.live_queries, system.stats
- ❌ `cli/tests/smoke/smoke_test_flush_manifest.rs` - Filesystem manifest.json verification
- ❌ `backend/tests/integration/test_api_query_limits.rs` - HTTP API limitation tests
- ❌ `cli/tests/smoke/smoke_test_subscribe_to_sql.rs` - SUBSCRIBE TO HTTP endpoint

---

## Test Priority Matrix

| Priority | Feature | Effort | Impact | File |
|----------|---------|--------|--------|------|
| 🔴 **P0** | Custom functions defaults | Medium | High | `smoke_test_custom_functions.rs` |
| 🔴 **P0** | system.tables options column | Low | High | `smoke_test_system_tables.rs` |
| 🔴 **P0** | Flush manifest.json checks | Medium | High | `smoke_test_flush_manifest.rs` |
| 🟡 **P1** | ALTER TABLE operations | Medium | Medium | `smoke_test_ddl_alter.rs` |
| 🟡 **P1** | Multi-row INSERT | Low | Medium | Extend existing |
| 🟡 **P1** | Soft vs hard DELETE | Low | Medium | Extend existing |
| 🟡 **P1** | SUBSCRIBE TO SQL | Medium | Medium | `smoke_test_subscribe_to_sql.rs` |
| 🟡 **P1** | HTTP API limits | Low | Medium | `test_api_query_limits.rs` |
| 🟢 **P2** | Flush policy combinations | Medium | Low | Extend flush tests |
| 🟢 **P2** | Timing output parsing | Low | Low | Extend benchmark test |
| 🟢 **P2** | Aggregation queries | Low | Low | Extend existing |

---

## Next Steps

1. **Create P0 tests** (custom functions, system.tables, manifest checks)
2. **Create P1 tests** (ALTER TABLE, SUBSCRIBE TO, HTTP limits)
3. **Extend existing tests** for gaps (multi-row INSERT, soft/hard DELETE)
4. **Run full suite** and verify all features are covered
5. **Document any behavior mismatches** between docs and implementation (TODO comments)

---

**Status**: 📋 Analysis Complete  
**Next**: 🚀 Begin P0 test implementation
