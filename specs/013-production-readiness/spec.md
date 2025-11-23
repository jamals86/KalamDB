# Feature Specification: Production Readiness & Full DML/DDL

**Feature Branch**: `013-production-readiness`
**Created**: 2025-11-23
**Status**: Draft
**Input**: User description: "The backend needs: Test the DELETE changes - Restart server and run DELETE tests. UPDATE handler - Apply same DataFusion approach as DELETE. ALTER TABLE support - Full DDL implementation. Manifest persistence - Write manifest.json after flush operations. System table enhancements - Add options column and live_queries table. Refactor providers for system tables and users/shared/stream tables. Revisit register_table logic."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Full DML Support (UPDATE/DELETE) (Priority: P1)

Users need to be able to modify and delete existing data using standard SQL commands, and trust that these changes are persisted even if the server restarts.

**Why this priority**: Essential for any mutable database. Without this, the system is append-only or read-only.

**Independent Test**: Can be tested by inserting data, updating it, deleting it, restarting the server, and verifying the final state matches expectations.

**Acceptance Scenarios**:

1. **Given** a table with existing records, **When** a user executes `UPDATE table SET col = val WHERE id = 1`, **Then** the record is updated and the change persists after server restart.
2. **Given** a table with existing records, **When** a user executes `DELETE FROM table WHERE id = 1`, **Then** the record is removed and remains removed after server restart.
3. **Given** a high volume of updates/deletes, **When** the system flushes to disk, **Then** the operations are correctly applied to the storage backend.

---

### User Story 2 - Schema Evolution (ALTER TABLE) (Priority: P1)

Users need to be able to modify the structure of existing tables as their application requirements evolve, without losing existing data.

**Why this priority**: Applications change over time; rigid schemas block development.

**Independent Test**: Create a table, insert data, alter the schema (add/drop column), and verify data accessibility and schema metadata.

**Acceptance Scenarios**:

1. **Given** an existing table, **When** a user executes `ALTER TABLE table ADD COLUMN new_col INT`, **Then** the column appears in the schema and can be queried (returning NULL for old rows).
2. **Given** an existing table, **When** a user executes `ALTER TABLE table DROP COLUMN old_col`, **Then** the column is removed from the schema and cannot be queried.
3. **Given** an existing table, **When** a user executes `ALTER TABLE table RENAME COLUMN old TO new`, **Then** the column is accessible via the new name.

---

### User Story 3 - System Observability (Priority: P2)

Administrators and developers need to inspect the current state of the system, including active queries and table configurations, to debug issues and optimize performance.

**Why this priority**: Critical for production maintenance and debugging.

**Independent Test**: Run a long query and check `system.live_queries`. Check `system.tables` for configuration options.

**Acceptance Scenarios**:

1. **Given** a running long query, **When** a user queries `SELECT * FROM system.live_queries`, **Then** the running query is listed with its duration and state.
2. **Given** a table created with specific options, **When** a user queries `SELECT * FROM system.tables`, **Then** the `options` column reflects the configuration.

---

### User Story 4 - Data Durability & Recovery (Priority: P1)

The system must ensure that the storage state is consistent and recoverable using a manifest file that tracks all data segments.

**Why this priority**: Prevents data corruption and ensures correct recovery after crashes.

**Independent Test**: Perform writes, trigger a flush, verify manifest file exists and contains correct segment info.

**Acceptance Scenarios**:

1. **Given** a system with recent writes, **When** a flush operation completes, **Then** a manifest file is written/updated in the storage directory.
2. **Given** a system restart, **When** the system initializes, **Then** it reads the manifest file to correctly load data segments.

### Edge Cases

- **Concurrent DDL/DML**: What happens if an ALTER TABLE occurs during an UPDATE? (Should lock or fail gracefully).
- **Partial Flush Failure**: If writing the manifest file fails, the system should not mark the flush as complete.
- **Invalid Manifest**: If the manifest file is corrupted, the system should fail to start or attempt recovery (safe mode).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support SQL `UPDATE` statements using optimized execution plans.
- **FR-002**: System MUST support SQL `DELETE` statements with persistence across server restarts.
- **FR-003**: System MUST support SQL `ALTER TABLE` statements including `ADD COLUMN`, `DROP COLUMN`, and `RENAME COLUMN`.
- **FR-004**: System MUST persist a manifest file after every flush operation to track storage segments.
- **FR-005**: The `system.tables` table MUST include an `options` column to display table configuration.
- **FR-006**: System MUST provide a `system.live_queries` table to view currently executing queries.
- **FR-007**: Table provider implementations (`UserTableProvider`, `SharedTableProvider`, `StreamTableProvider`) MUST be refactored to share common logic. Specifically, `User` and `Shared` tables share significant logic (DML, Flush, Manifest) which MUST be unified to prevent duplication.
- **FR-008**: Table registration logic MUST be centralized and optimized to avoid redundant column checks during query planning.

### Technical Implementation Details

The implementation MUST adhere to the following architectural patterns:

1.  **Insert Strategy**:
    -   Inserts are append-only operations.
    -   New rows are assigned a monotonically increasing `_seq` ID.
    -   No existence check is performed during insert (conflicts resolved at read time).
    -   On the first insert, a manifest entry MUST be initialized in the hot store (if not present) for the specific scope (User or Shared).

2.  **Update Strategy**:
    -   Updates first search for the existing row in hot storage (RocksDB).
    -   If not found in hot storage, search in cold storage (Parquet).
    -   The old values are retrieved, merged with new values, and appended as a new version with a new `_seq`.

3.  **Scan Strategy**:
    -   Scans must query both hot (RocksDB) and cold (Parquet) storage.
    -   Scans MUST rely on the manifest to determine data location. The manifest dictates which Parquet files to read and provides metadata for the hot storage segment.
    -   Conflict resolution uses `MAX(_seq)` per primary key to return the latest version.

4.  **Manifest Usage**:
    -   The `manifest.json` file is the source of truth for data location.
    -   Scans must consult the manifest to avoid unnecessary file I/O (pruning based on ranges).
    -   The manifest tracks what is currently in hot storage vs. cold storage.
    -   The manifest MUST include metadata for the hot storage segment, including `column_stats` (min/max/null_count) for all columns to optimize lookups and enable predicate pushdown.

5.  **Manifest Lifecycle**:
    -   The manifest is created/initialized in the hot store (separate column family/table) upon the first insert operation.
    -   The manifest is maintained in memory/hot store during active writes.
    -   **Hot/Cold Split**:
        -   **Hot Store (L1/L2 Cache)**: `ManifestCacheService` uses an in-memory `DashMap` (L1) and RocksDB `EntityStore` (L2) for sub-millisecond access during query planning.
        -   **Cold Store (Persistence)**: `ManifestService` manages the authoritative `manifest.json` file on disk/S3.
    -   During flush operations, the manifest is serialized to `manifest.json` in the cold storage directory (created or updated).
    -   This ensures durability and fast access during runtime.

6.  **DataFusion Integration**:
    -   All DML and query operations MUST rely on DataFusion's execution engine.
    -   Direct access to RocksDB or storage providers bypassing DataFusion is prohibited for query execution.
    -   Table providers must expose data via standard DataFusion interfaces (`TableProvider`, `ExecutionPlan`).

### Key Entities

- **Manifest**: A file tracking the list of active storage segments and checkpoints for recovery.
- **LiveQuery**: A system entity representing an active SQL query, including ID, query text, start time, and user.
- **TableOptions**: A set of configuration parameters (e.g., retention policy, compression) associated with a table.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `UPDATE` and `DELETE` operations are verified to be durable across 3 consecutive server restarts in integration tests.
- **SC-002**: `ALTER TABLE` operations successfully modify schema for 100% of supported column types without data loss.
- **SC-003**: Manifest file is generated within 500ms of flush completion.
- **SC-004**: Refactoring reduces the lines of code in table provider definitions by at least 10% (qualitative review).
- **SC-005**: Table registration overhead is reduced, measurable by micro-benchmarks or code complexity analysis.
