# Old Spec (002-simple-kalamdb) Feature Comparison

## Purpose
Compare features from old spec (002-simple-kalamdb) with new spec (004-system-improvements-and) to identify any missing features that should be incorporated.

## Comparison Approach
- ✅ Feature already in new spec or already implemented
- ⚠️ Feature missing but should be added to new spec
- 📌 Feature is foundational/base functionality (assume already implemented)
- ❌ Feature explicitly out of scope or deprecated

---

## Architecture & Data Flow

| Feature | Status | Notes |
|---------|--------|-------|
| Table-per-user multi-tenancy | 📌 | Base architecture - already implemented |
| Two-tier storage (RocksDB + Parquet) | 📌 | Base architecture - already implemented |
| Write path: API→RocksDB→Flush→Parquet | 📌 | Base architecture - already implemented |
| Read path: DataFusion union of RocksDB+Parquet | 📌 | Base architecture - already implemented |
| System columns (_updated, _deleted) | 📌 | Base architecture - already implemented |
| Live query actor-based WebSocket system | 📌 | Base architecture - already implemented |

---

## REST API & WebSocket (User Story 0)

| Feature | Status | Notes |
|---------|--------|-------|
| POST /api/sql endpoint for SQL execution | 📌 | Base API - already implemented |
| Single SQL statement support | 📌 | Base API - already implemented |
| Multiple SQL statements (semicolon separated) | ⚠️ | **MISSING** - should be added to new spec |
| WebSocket /ws endpoint for subscriptions | 📌 | Base API - already implemented |
| Multiple subscriptions per WebSocket | 📌 | Base feature - already implemented |
| Initial data fetch (last N rows) on subscription | ⚠️ | **MISSING** - should be added to new spec |
| Change notifications (INSERT/UPDATE/DELETE) | 📌 | Base feature - already implemented |
| Query ID in change notifications | 📌 | Base feature - already implemented |
| Error messages for malformed SQL (HTTP 400) | 📌 | Base feature - already implemented |

---

## RocksDB Column Family Architecture

| Feature | Status | Notes |
|---------|--------|-------|
| User tables: one CF per table | 📌 | Base architecture - already implemented |
| User table key format: {user_id}:{row_id} | 📌 | Base architecture - already implemented |
| Shared tables: one CF per table | 📌 | Base architecture - already implemented |
| Shared table key format: {row_id} | 📌 | Base architecture - already implemented |
| Stream tables: one CF per table | 📌 | Base architecture - already implemented |
| Stream table key format: {timestamp}:{row_id} | 📌 | Base architecture - already implemented |
| System tables: one CF per system table | 📌 | Base architecture - already implemented |
| user_table_counters CF for flush tracking | 📌 | Base architecture - already implemented |

---

## System Tables

| Feature | Status | Notes |
|---------|--------|-------|
| system.users table | 📌 | Base system table - already implemented |
| system.namespaces table | 📌 | Base system table - already implemented |
| system.tables table | 📌 | Base system table - already implemented |
| system.table_schemas table | 📌 | Base system table - already implemented |
| system.storage_locations table | 📌 | Base system table - already implemented |
| system.live_queries table | 📌 | Base system table - already implemented |
| system.jobs table | 📌 | Base system table - already implemented |
| live_queries.node field (cluster routing) | ⚠️ | **MISSING** - cluster-aware features not in new spec |
| live_queries.options field (JSON) | ⚠️ | **MISSING** - query options storage not in new spec |
| live_queries.changes counter | ⚠️ | **MISSING** - change tracking counter not in new spec |
| jobs.parameters array | ⚠️ | **MISSING** - job parameters not in new spec |
| jobs.result string | ⚠️ | **MISSING** - job result storage not in new spec |
| jobs.trace string | ⚠️ | **MISSING** - job trace/context not in new spec |
| jobs.memory_used, cpu_used fields | ⚠️ | **MISSING** - resource metrics not in new spec |

---

## Namespace Management (User Story 1)

| Feature | Status | Notes |
|---------|--------|-------|
| CREATE NAMESPACE | 📌 | Base feature - already implemented |
| DROP NAMESPACE | 📌 | Base feature - already implemented |
| ALTER NAMESPACE (metadata) | 📌 | Base feature - already implemented |
| SHOW NAMESPACES | 📌 | Base feature - already implemented |
| Namespace dependency checking (prevent drop if tables exist) | 📌 | Base feature - already implemented |

---

## User Table Management (User Story 3)

| Feature | Status | Notes |
|---------|--------|-------|
| CREATE USER TABLE | 📌 | Base feature - already implemented |
| DROP USER TABLE | 📌 | Base feature - already implemented |
| LOCATION with ${user_id} template | 📌 | Base feature - already implemented |
| LOCATION REFERENCE to predefined storage | 📌 | Base feature - already implemented |
| FLUSH POLICY ROWS | 📌 | Base feature - already implemented |
| FLUSH POLICY INTERVAL | 📌 | Base feature - already implemented |
| Per-user-per-table flush tracking | 📌 | Base feature - already implemented |
| Automatic _updated, _deleted columns | 📌 | Base feature - already implemented |
| deleted_retention per table | 📌 | Base feature - already implemented |
| Soft delete mechanism | 📌 | Base feature - already implemented |
| Prevent DROP TABLE with active subscriptions | ⚠️ | **MISSING** - safety check not in new spec |

---

## Shared Table Management (User Story 5)

| Feature | Status | Notes |
|---------|--------|-------|
| CREATE SHARED TABLE | 📌 | Base feature - already implemented |
| DROP SHARED TABLE | 📌 | Base feature - already implemented |
| Single storage location (no user_id template) | 📌 | Base feature - already implemented |
| FLUSH POLICY support | 📌 | Base feature - already implemented |
| deleted_retention support | 📌 | Base feature - already implemented |

---

## Stream Table Management (User Story 4a)

| Feature | Status | Notes |
|---------|--------|-------|
| CREATE STREAM TABLE | 📌 | Base feature - already implemented |
| DROP STREAM TABLE | 📌 | Base feature - already implemented |
| retention option (TTL) | 📌 | Base feature - already implemented |
| ephemeral option | 📌 | Base feature - already implemented |
| max_buffer option | 📌 | Base feature - already implemented |
| NO disk persistence | 📌 | Base feature - already implemented |
| NO _updated/_deleted columns | 📌 | Base feature - already implemented |
| Automatic TTL eviction | 📌 | Base feature - already implemented |
| Buffer limit eviction | 📌 | Base feature - already implemented |

---

## Schema Management & Versioning

| Feature | Status | Notes |
|---------|--------|-------|
| Arrow schema storage in RocksDB | 📌 | Base feature - already implemented |
| Schema versioning (v1, v2, v3, ...) | 📌 | Base feature - already implemented |
| system.table_schemas for version history | 📌 | Base feature - already implemented |
| current_schema_version in system.tables | 📌 | Base feature - already implemented |
| ALTER TABLE ADD COLUMN | 📌 | Base feature - already implemented |
| ALTER TABLE DROP COLUMN | 📌 | Base feature - already implemented |
| ALTER TABLE MODIFY COLUMN | 📌 | Base feature - already implemented |
| Schema evolution with Parquet projection | 📌 | Base feature - already implemented |
| DEFAULT values for new columns | 📌 | Base feature - already implemented |
| Prevent ALTER on system columns | 📌 | Base feature - already implemented |
| Prevent ALTER on stream tables | 📌 | Base feature - already implemented |
| Schema cache invalidation | 📌 | Base feature - already implemented |
| DESCRIBE TABLE showing schema history | ⚠️ | **MISSING** - enhanced introspection not in new spec |

---

## Metadata Persistence (RocksDB-only)

| Feature | Status | Notes |
|---------|--------|-------|
| All metadata in RocksDB (no file system JSON) | 📌 | Base architecture - already implemented |
| system_namespaces CF | 📌 | Base architecture - already implemented |
| system_tables CF | 📌 | Base architecture - already implemented |
| system_table_schemas CF | 📌 | Base architecture - already implemented |
| In-memory catalog loaded on startup | 📌 | Base architecture - already implemented |
| Atomic updates via WriteBatch | 📌 | Base architecture - already implemented |

---

## kalamdb-sql Crate (Unified SQL Engine)

| Feature | Status | Notes |
|---------|--------|-------|
| Separate kalamdb-sql crate | 📌 | Base architecture - already implemented |
| execute_system_query() interface | 📌 | Base architecture - already implemented |
| SQL parsing via sqlparser-rs | 📌 | Base architecture - already implemented |
| System table CRUD (SELECT/INSERT/UPDATE/DELETE) | 📌 | Base architecture - already implemented |
| Rust models for all system tables | 📌 | Base architecture - already implemented |
| RocksDB CF mapping | 📌 | Base architecture - already implemented |
| Stateless/idempotent design (Raft-ready) | ⚠️ | **MISSING** - Raft preparation not documented in new spec |
| Future change event emission support | ⚠️ | **MISSING** - Raft replication prep not in new spec |

---

## kalamdb-store Crate (K/V Operations)

| Feature | Status | Notes |
|---------|--------|-------|
| Separate kalamdb-store crate | 📌 | Base architecture - already implemented |
| UserTableStore with put/get/delete | 📌 | Base architecture - already implemented |
| SharedTableStore (planned) | ⚠️ | **MISSING** - SharedTableStore not yet implemented |
| StreamTableStore (planned) | ⚠️ | **MISSING** - StreamTableStore not yet implemented |
| Key encoding utilities | 📌 | Base architecture - already implemented |
| System column injection | 📌 | Base architecture - already implemented |

---

## Live Query Subscriptions (User Story 6)

| Feature | Status | Notes |
|---------|--------|-------|
| WebSocket subscriptions on user tables | 📌 | Base feature - already implemented |
| Filtered queries with WHERE clauses | 📌 | Base feature - already implemented |
| Implicit user_id filter for user tables | 📌 | Base feature - already implemented |
| No implicit filter for shared/stream tables | 📌 | Base feature - already implemented |
| INSERT/UPDATE/DELETE notifications | 📌 | Base feature - already implemented |
| Multiple subscriptions per connection | 📌 | Base feature - already implemented |
| Automatic cleanup on disconnect | 📌 | Base feature - already implemented |
| Change tracking via _updated/_deleted | 📌 | Base feature - already implemented |
| "last N rows" initial data fetch | ⚠️ | **MISSING** - initial data not in new spec |
| subscription options (JSON) | ⚠️ | **MISSING** - options storage not in new spec |
| KILL LIVE QUERY command | ⚠️ | **MISSING** - manual subscription termination not in new spec |

---

## Flush Policy & Resource Management

| Feature | Status | Notes |
|---------|--------|-------|
| Row-based flush policy (per-user-per-table) | 📌 | Base feature - already implemented |
| user_table_counters CF | 📌 | Base feature - already implemented |
| Counter increment on INSERT | 📌 | Base feature - already implemented |
| Counter reset after flush | 📌 | Base feature - already implemented |
| Time-based flush policy | 📌 | Base feature - already implemented |
| Combined flush policy (ROWS + INTERVAL) | 📌 | Base feature - already implemented |
| Flush job execution | 📌 | Base feature - already implemented |
| Flush job recording in system.jobs | 📌 | Base feature - already implemented |
| Concurrent flush jobs | 📌 | Base feature - already implemented |

---

## Storage Backend

| Feature | Status | Notes |
|---------|--------|-------|
| Filesystem storage support | 📌 | Base feature - already implemented |
| S3 storage support | 📌 | Base feature - already implemented |
| ${user_id} template substitution | 📌 | Base feature - already implemented |
| Storage location validation | 📌 | Base feature - already implemented |
| Retry logic for failures | 📌 | Base feature - already implemented |

---

## Catalog & Introspection (User Story 8)

| Feature | Status | Notes |
|---------|--------|-------|
| SHOW NAMESPACES | 📌 | Base feature - already implemented |
| SHOW TABLES | 📌 | Base feature - already implemented |
| DESCRIBE TABLE | 📌 | Base feature - already implemented |
| Table type distinction (USER/SHARED/STREAM) | 📌 | Base feature - already implemented |
| SHOW TABLE STATS | ⚠️ | **MISSING** - enhanced statistics not in new spec |
| information_schema.tables | ⚠️ | **MISSING** - SQL standard introspection not in new spec |

---

## Backup & Restore (User Story 7)

| Feature | Status | Notes |
|---------|--------|-------|
| BACKUP DATABASE command | 📌 | Base feature - already implemented |
| RESTORE DATABASE command | 📌 | Base feature - already implemented |
| SHOW BACKUP command | 📌 | Base feature - already implemented |
| Incremental backup (future) | ❌ | Out of scope - future consideration |
| Stream table exclusion from backups | 📌 | Base feature - already implemented |
| Soft-deleted rows included in backups | 📌 | Base feature - already implemented |

---

## Future Features (Documented but Not Implemented)

| Feature | Status | Notes |
|---------|--------|-------|
| Table export functionality | ❌ | Out of scope - future consideration |
| Raft consensus replication | ❌ | Out of scope - future consideration |
| Deleted row retention cleanup jobs | ❌ | Out of scope - future consideration |
| Distributed cluster with node specialization | ❌ | Out of scope - future consideration |
| system.cluster_nodes table | ❌ | Out of scope - future consideration |

---

## Summary of Missing Features to Add to New Spec

### High Priority (Core Functionality)
1. **Multiple SQL statements in single API request** (semicolon separated)
2. **Initial data fetch on WebSocket subscription** (last N rows option)
3. **Prevent DROP TABLE when active subscriptions exist** (safety check)
4. **KILL LIVE QUERY command** (manual subscription termination)

### Medium Priority (System Table Enhancements)
5. **system.live_queries enhanced fields**: options (JSON), changes counter, node identifier
6. **system.jobs enhanced fields**: parameters array, result string, trace string, resource metrics
7. **DESCRIBE TABLE showing schema history** (enhanced introspection)
8. **SHOW TABLE STATS command** (row counts, storage size)

### Low Priority (Architecture Documentation)
9. **kalamdb-sql Raft-ready design** (stateless, idempotent, future event emission)
10. **SharedTableStore and StreamTableStore** (kalamdb-store completion)
11. **information_schema.tables** (SQL standard catalog queries)

### Already Covered in New Spec
- ✅ Parametrized queries (FR-001 to FR-009 in new spec)
- ✅ Automatic flushing improvements (FR-010 to FR-023 in new spec)
- ✅ Manual flushing commands (FR-024 to FR-029 in new spec)
- ✅ Session-level caching (FR-030 to FR-036 in new spec)
- ✅ Storage backend abstraction (FR-071 to FR-076 in new spec)

---

## Recommendations

### Must Add to New Spec
1. Add **FR-106 to FR-110**: Multiple SQL statements in single request
2. Add **FR-111 to FR-115**: WebSocket initial data fetch (last N rows)
3. Add **FR-116 to FR-118**: DROP TABLE safety checks (active subscriptions)
4. Add **FR-119 to FR-121**: KILL LIVE QUERY command

### Should Add to New Spec
5. Add **FR-122 to FR-130**: Enhanced system.live_queries fields
6. Add **FR-131 to FR-140**: Enhanced system.jobs fields
7. Add **FR-141 to FR-145**: Enhanced introspection (DESCRIBE, SHOW STATS)

### Document as Assumptions
8. Note that base architecture (table-per-user, two-tier storage, etc.) is already implemented
9. Reference old spec (002-simple-kalamdb) for foundational features
10. Clarify that new spec focuses on improvements/enhancements only
