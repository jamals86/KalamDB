# Feature Specification: PostgreSQL-Style Transactions for KalamDB

**Feature Branch**: `027-pg-transactions`  
**Created**: 2026-03-23  
**Last Updated**: 2026-04-08  
**Status**: Completed (Implemented and validated)
**Input**: User description: "I want to add transactions to kalamdb it should be similar to postgres transaction, the main cause is to be able to run transactions from pg_kalam to the kalamdb-server"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Atomic Multi-Statement Writes via pg_kalam (Priority: P1)

A developer using PostgreSQL with the pg_kalam foreign data wrapper needs to execute multiple write operations (INSERT, UPDATE, DELETE) against KalamDB tables within a single PostgreSQL transaction. All writes within the transaction must either fully succeed on COMMIT or be completely discarded on ROLLBACK, ensuring the KalamDB data is never left in a partial state.

**Why this priority**: This is the primary motivation for the feature. Today, pg_kalam already sends BEGIN/COMMIT/ROLLBACK RPCs to the server, but the server does not actually group writes into an atomic unit — writes are applied immediately and cannot be rolled back. Without server-side transaction support, pg_kalam transactions are unreliable for any multi-statement workflow.

**Independent Test**: Can be tested by opening a PostgreSQL transaction, inserting rows into two different KalamDB foreign tables, issuing ROLLBACK, and verifying that neither table contains the inserted rows. Conversely, issuing COMMIT and verifying both tables contain the expected rows.

**Acceptance Scenarios**:

1. **Given** two KalamDB foreign tables exist in PostgreSQL, **When** a user begins a transaction, inserts a row into each table, and commits, **Then** both rows are visible in subsequent queries.
2. **Given** a user begins a transaction and inserts rows into a KalamDB foreign table, **When** the user issues ROLLBACK, **Then** none of the inserted rows are visible in subsequent queries.
3. **Given** a user begins a transaction and inserts a row, then updates it within the same transaction, **When** the user commits, **Then** the final updated state is persisted.
4. **Given** a user begins a transaction and deletes a row, **When** the user issues ROLLBACK, **Then** the deleted row is still present.
5. **Given** pg_kalam receives a transaction ID from the server at BEGIN, **When** rows are staged and later committed or rolled back, **Then** that same transaction ID remains the canonical identifier for the work from pg_kalam through the server's durable apply path.
6. **Given** a transaction targets user or shared tables, **When** the user commits or rolls back, **Then** transactional semantics are enforced for those tables without requiring the caller to know which storage engine is backing the server internally.

---

### User Story 2 - Read-Your-Writes Within a Transaction (Priority: P1)

A developer performing multiple operations within a transaction expects to read back their own uncommitted changes. For example, inserting a row and then querying the table within the same transaction should return the newly inserted row, even though it has not yet been committed.

**Why this priority**: Read-your-writes consistency is a fundamental expectation of PostgreSQL transactions. Without it, application logic that depends on sequential operations within a transaction will produce incorrect results, making the transaction feature unreliable.

**Independent Test**: Can be tested by beginning a transaction, inserting a row, then selecting from the same table within the transaction and verifying the row is returned. After rollback, verify the row is gone.

**Acceptance Scenarios**:

1. **Given** a user begins a transaction and inserts a row, **When** the user queries the same table within the transaction, **Then** the inserted row is included in the results.
2. **Given** a user begins a transaction, inserts a row, and then deletes it, **When** the user queries the table within the transaction, **Then** the row is not returned.
3. **Given** a user begins a transaction and updates a row, **When** the user queries the row within the transaction, **Then** the updated values are returned.

---

### User Story 3 - Transaction Isolation Between Concurrent Sessions (Priority: P2)

Two PostgreSQL sessions connected via pg_kalam are operating concurrently. Each session's uncommitted changes must be invisible to the other session. Only committed data is visible to other sessions.

**Why this priority**: Isolation prevents data corruption and race conditions in multi-user environments. While single-session correctness (P1) is the foundation, multi-session isolation is critical for any production deployment.

**Independent Test**: Can be tested by opening two concurrent PostgreSQL connections, beginning a transaction in each, inserting a row in session A, and verifying session B does not see the row until session A commits.

**Acceptance Scenarios**:

1. **Given** session A begins a transaction and inserts a row, **When** session B queries the same table, **Then** session B does not see the uncommitted row.
2. **Given** session A commits its transaction, **When** session B queries the table, **Then** session B sees the newly committed row.
3. **Given** session A begins a transaction, **When** session B commits a new row, **Then** session A does not see session B's new row within its already-started transaction (snapshot isolation).

---

### User Story 4 - Direct Transaction Management via KalamDB SQL Statements (Priority: P2)

A developer using the KalamDB server directly, without pg_kalam, needs the server's SQL execution path itself to understand and execute BEGIN, COMMIT, and ROLLBACK statements. This enables batching multiple DML statements atomically through KalamDB's native SQL surface, including the existing HTTP SQL endpoint.

**Why this priority**: While pg_kalam is the primary driver, the transaction mechanism must also exist in KalamDB's own SQL execution path so the server is transaction-aware independently of PostgreSQL. This ensures one transaction model across all access paths.

**Independent Test**: Can be tested by sending a multi-statement SQL request containing BEGIN, DML statements, and COMMIT to the KalamDB server, then verifying the committed data. A second test should replace COMMIT with ROLLBACK and verify the data is discarded.

**Acceptance Scenarios**:

1. **Given** a user sends BEGIN through KalamDB's SQL execution path, executes two INSERT statements, and sends COMMIT, **Then** both rows are persisted and visible.
2. **Given** a user sends BEGIN through KalamDB's SQL execution path and executes an INSERT, **When** the user sends ROLLBACK, **Then** the row is not persisted.
3. **Given** a user sends a single `/v1/api/sql` request containing more than one transaction block, **When** each block is explicitly committed or rolled back in order, **Then** each block is executed independently within that same request.
4. **Given** a `/v1/api/sql` request terminates while one or more explicit transactions are still open, **When** the request ends, **Then** all still-open request-scoped transactions are automatically shut down and rolled back.

---

### User Story 5 - Automatic Rollback on Connection Drop (Priority: P3)

If a client session disconnects (network failure, crash, timeout) while a transaction is in progress, the server must automatically roll back any uncommitted changes to prevent data from being left in a partial state.

**Why this priority**: Safety net for production reliability. Without automatic cleanup, crashed clients would leave phantom uncommitted data in the system.

**Independent Test**: Can be tested by beginning a transaction, inserting rows, forcibly closing the connection, then verifying from a new session that the rows were not persisted.

**Acceptance Scenarios**:

1. **Given** a session has an active transaction with uncommitted writes, **When** the session disconnects unexpectedly, **Then** all uncommitted writes are discarded.
2. **Given** a session has an active transaction, **When** the session idle timeout expires, **Then** the transaction is rolled back and resources are freed.
3. **Given** a pg_kalam session is closed while a transaction is active, **When** the server cleans up the session, **Then** that transaction's canonical transaction ID is marked aborted and all staged writes for that transaction are discarded.

---

### User Story 6 - Transaction Timeout Protection (Priority: P3)

Long-running transactions that exceed a configurable time limit are automatically aborted to prevent resource exhaustion and lock contention.

**Why this priority**: Defensive measure for production stability. Runaway or forgotten transactions should not hold resources indefinitely.

**Independent Test**: Can be tested by beginning a transaction, waiting beyond the configured timeout, and verifying the transaction is automatically aborted and subsequent operations within it fail.

**Acceptance Scenarios**:

1. **Given** a transaction has been idle beyond the configured timeout, **When** the user attempts another operation within the transaction, **Then** the operation fails with a timeout error and the transaction is aborted.
2. **Given** the server is configured with a transaction timeout, **When** a transaction exceeds this limit, **Then** the server logs the timeout event and frees associated resources.

---

### Edge Cases

- What happens when a transaction spans writes to both shared tables and user-scoped tables?
- What happens when a transaction attempts to touch a stream table, which does not currently support transactions?
- How does the system handle a COMMIT that partially fails (e.g., one table write succeeds but another encounters a constraint violation)?
- What happens when a client sends DML statements outside of an explicit transaction (autocommit mode)?
- What happens when a client sends `BEGIN`, `COMMIT`, or `ROLLBACK` through KalamDB SQL without using pg_kalam?
- What happens when a single `/v1/api/sql` request contains multiple sequential `BEGIN ... COMMIT/ROLLBACK` blocks?
- What happens when a `/v1/api/sql` request ends while one or more transaction blocks remain open?
- How is the same transaction ID preserved from `BeginTransaction` through staged mutations, observability, and the final RocksDB commit batch?
- What happens if a client issues BEGIN while a transaction is already active (nested transaction / savepoint)?
- What happens when the server restarts while transactions are in progress?
- How does the system behave when a transaction contains only read operations (no writes)?
- What happens when a transaction buffer grows very large (e.g., millions of rows before COMMIT)?
- What happens when `COMMIT`, `ROLLBACK`, timeout cleanup, and session/request close race against each other for the same transaction?
- What happens when a large committed transaction fans out change notifications to thousands of subscribers on one hot table?
- What happens when a subscriber is still loading its initial snapshot while a committed transaction produces a burst of live changes?
- What happens when a slow subscriber cannot drain post-commit notifications as fast as the commit path produces them?
- What happens in cluster mode when the first table touched by a transaction maps to one Raft data group but a later statement targets another?
- What happens when the leader for a transaction's bound Raft group changes while the transaction is still open?
- What happens when a client reads from a follower immediately after a successful transaction commit but before that follower has locally applied the committed Raft entry?
- What happens when the newest committed version of a logical row is newer than the transaction snapshot but an older committed version of that same row is still visible?
- What happens when simple SQL fast-path DML (`INSERT ... VALUES`, point `UPDATE`, point `DELETE`) is executed inside an explicit transaction and would otherwise bypass staging?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The server MUST support explicit transaction lifecycle commands: BEGIN (or START TRANSACTION), COMMIT, and ROLLBACK, within an established session.
- **FR-001A**: The KalamDB server's own SQL execution path MUST recognize and execute BEGIN (or START TRANSACTION), COMMIT, and ROLLBACK statements, not only the pg RPC transaction calls.
- **FR-001B**: A single `/v1/api/sql` request MUST be allowed to contain zero, one, or more sequential explicit transaction blocks.
- **FR-002**: All DML operations (INSERT, UPDATE, DELETE) executed within an active transaction MUST be buffered and only made durable and visible to other sessions upon COMMIT.
- **FR-002A**: For this phase, explicit transaction support MUST apply to user tables and shared tables only. Stream tables are out of scope and MUST be rejected with a clear error before staging; they MUST NOT appear in the transaction write set, `TransactionCommit` batch, or post-commit fanout/publisher plan.
- **FR-003**: On ROLLBACK (explicit or implicit via disconnect/timeout), the server MUST discard all buffered writes for that transaction with no side effects.
- **FR-004**: Within an active transaction, the originating session MUST see its own uncommitted writes when querying (read-your-writes consistency).
- **FR-005**: Uncommitted writes from one session's transaction MUST NOT be visible to other sessions (session-level isolation).
- **FR-006**: DML operations executed outside of an explicit transaction MUST continue to behave as autocommit (each statement is its own implicit transaction, applied immediately).
- **FR-007**: The server MUST automatically rollback any in-progress transaction when a session disconnects or is terminated.
- **FR-008**: The server MUST enforce a configurable transaction timeout. Transactions exceeding this limit MUST be automatically aborted.
- **FR-009**: The server MUST reject nested BEGIN commands within an active transaction with a clear error message (savepoints are out of scope for this feature).
- **FR-010**: A COMMIT on an empty transaction (no DML was executed) MUST succeed as a no-op.
- **FR-011**: A ROLLBACK on an empty or non-existent transaction MUST succeed as a no-op.
- **FR-012**: The server MUST track active transactions and expose them through a `system.transactions` virtual view backed by the current in-memory transaction state, including at least count, age, origin, and owner identifier.
- **FR-013**: Writes within a transaction MUST apply atomically on COMMIT — either all writes across all affected tables succeed, or none do.
- **FR-014**: The pg_kalam extension's existing BEGIN/COMMIT/ROLLBACK RPC calls MUST work with the new server-side transaction implementation without changes to the pg_kalam wire protocol.
- **FR-015**: The server MUST handle concurrent transactions from multiple sessions without data corruption or lost writes.
- **FR-016**: The implementation MUST include automated tests covering pg RPC transaction lifecycle, KalamDB SQL BEGIN/COMMIT/ROLLBACK execution, rollback-on-error, disconnect/timeout cleanup, isolation, autocommit regression behavior, repeated lifecycle race/fault-injection coverage, and live-fanout stress behavior with slow/high-cardinality subscriber sets.
- **FR-017**: The implementation MUST preserve the current non-transaction execution path so requests that do not use explicit transactions do not pay more than minimal overhead.
- **FR-018**: The transaction ID returned by `BeginTransaction` MUST remain the canonical identifier for that transaction across pg_kalam, server-side staged mutations, observability, cleanup, and the final durable commit path.
- **FR-019**: When a pg_kalam session closes with an active transaction, the server MUST automatically abort that same canonical transaction ID and discard all uncommitted staged writes for it.
- **FR-020**: When a `/v1/api/sql` request ends, the server MUST automatically shut down and roll back every still-open request-scoped transaction created by that request, because request-scoped SQL transactions are not allowed to survive across API calls.
- **FR-021**: The transaction orchestration layer MUST depend on the server's storage abstraction rather than on RocksDB-specific types, so the implementation can migrate to a different storage engine in the future without changing transaction semantics.

### Transaction Owner Binding Requirements (updated 2026-04-07)

- **FR-022**: Every transaction MUST be owned by exactly one execution owner identifier. For `PgRpc`, this owner is the session_id. For `SqlBatch`, it is a lightweight request-scoped owner identifier. That owner identifier is the correlation key carried through the transaction lifecycle.
- **FR-023**: Each PostgreSQL backend process × foreign server config pair MUST map to a unique session (format: `pg-<pid>-<config_hash>`). Transactions from different foreign server configs in the same PG backend MUST be independent.
- **FR-024**: The pg_kalam extension's `fdw_xact` module MUST track active transactions per session_id (not as a single global), enabling multiple concurrent sessions per PG backend when different foreign server configs are used.
- **FR-025**: When a PostgreSQL backend process exits, the extension MUST close ALL sessions registered for that backend (iterating the RemoteStateRegistry), which triggers server-side transaction rollback for each.
- **FR-026**: The `system.sessions` virtual view MUST expose transaction state (transaction_id, transaction_state, transaction_has_writes) for active pg sessions only.
- **FR-027**: The server MUST expose a `system.transactions` virtual view listing all active in-memory transactions across `PgRpc`, `SqlBatch`, and `Internal` origins.
- **FR-028**: Transactions opened through `/v1/api/sql` MUST appear in `system.transactions` while active, but MUST NOT create rows in `system.sessions`.
- **FR-029**: `system.transactions` MUST be computed on demand from the in-memory transaction coordinator state only; it MUST NOT persist transaction history, start background polling, or materialize extra storage solely for observability.

### Production Hardening Requirements (added 2026-04-07)

- **FR-030**: The transaction coordinator MUST use typed execution-owner keys internally rather than raw string owner identifiers on the hot path, while still exposing human-readable owner identifiers in APIs and virtual views.
- **FR-031**: The transaction coordinator MUST maintain explicit lifecycle states that distinguish at least read-only active, write-active, committing, rolling back, timed out, and terminal phases.
- **FR-032**: `BEGIN` / `START TRANSACTION` MUST remain cheap for read-only and empty transactions; write buffers, overlays, and other heavy staged-write structures MUST be allocated lazily on first write.
- **FR-033**: Commit, rollback, timeout cleanup, and owner-loss cleanup MUST follow idempotent ordered phases so races between `COMMIT`, `ROLLBACK`, session close, and request-end cleanup cannot leak staged writes or double-release side effects.
- **FR-034**: Live query notifications, publisher events, and other external side effects produced by committed user/shared-table mutations from an explicit transaction MUST be released only after durable commit succeeds, and MUST NOT hold the transaction coordinator's active-state lock while fanout executes.
- **FR-035**: Live query fanout for committed transaction changes MUST start from pre-indexed candidate subscription sets keyed at least by table and ownership scope; it MUST NOT scan every active connection for each committed row.
- **FR-036**: Live query fanout MUST enforce bounded per-connection and per-subscription buffering for slow consumers, with deterministic drop or disconnect behavior rather than unbounded backlog growth.
- **FR-037**: During fanout, payload materialization and serialization work MUST be shared across subscribers with identical projection and wire-format requirements whenever possible.

### Raft / Cluster Alignment Requirements (added 2026-04-07)

- **FR-038**: When explicit transactions are enabled in cluster mode, a transaction MUST bind to exactly one data Raft group for its lifetime. The first transactional table access may establish that binding, but once established it MUST NOT move to another group.
- **FR-039**: In cluster mode, any explicit transaction that attempts to read or write a table mapped to a different data Raft group than the bound group MUST be rejected with a clear error in this phase.
- **FR-040**: In cluster mode, open transaction state and staged writes MUST remain leader-local until `COMMIT`; they MUST NOT be treated as durable or failover-safe before the final `TransactionCommit` Raft proposal succeeds.
- **FR-041**: If the leader for the bound Raft group changes while an explicit transaction is still open, the transaction MUST be aborted and all uncommitted staged writes discarded. The client MUST retry from a new transaction.
- **FR-042**: Successful transaction commit in cluster mode MUST continue to follow normal Raft semantics: success is returned after quorum replication and leader apply of the final commit entry, not after every follower has applied it locally.

### Visibility and Concurrency Requirements (added 2026-04-07)

- **FR-043**: Every committed user-table and shared-table row version MUST carry a persisted hidden commit-order marker (for example `_commit_seq`) in both hot and cold storage representations so snapshot visibility can be evaluated without consulting a separate per-row side table.
- **FR-044**: Snapshot visibility MUST resolve the latest visible committed version for each logical row using `commit_seq <= snapshot_commit_seq`, with `_seq` used only as a deterministic tie-break within the same commit boundary. The system MUST NOT resolve `MAX(_seq)` first and then post-filter by snapshot.
- **FR-045**: Autocommit writes and explicit-transaction commits MUST obtain their commit-order values from the same durable apply-path source so `snapshot_commit_seq` is globally ordered across both execution modes and remains monotonic after restart.
- **FR-046**: The transaction implementation MUST avoid holding coordinator-wide locks, map guards, or equivalent shared state across awaited Raft writes, storage I/O, file cleanup, or live fanout. Timeout enforcement MUST use bounded shared background work rather than one timer task per active transaction.

### Key Entities

- **Session**: A long-lived transport connection tracked by the server, currently the pg_kalam gRPC session model. Each pg session is uniquely identified by a config-scoped session_id (format `pg-<pid>-<config_hash>`). `system.sessions` remains focused on these live pg sessions and their current transaction metadata.
- **Execution Owner ID**: The owner identifier for a transaction. For `PgRpc`, this is the pg session_id. For `SqlBatch`, this is a lightweight request-scoped identifier such as `sql-req-<request_id>`. The owner ID is carried through the transaction lifecycle without requiring every origin to create a `system.sessions` row.
- **Transaction**: Represents an active unit of work within one execution owner context. Has a lifecycle (active → committed or aborted), a unique identifier, a start timestamp, and an association with its owner ID. Contains a buffer of uncommitted write operations.
- **Canonical Transaction ID**: The stable transaction identifier created at BEGIN and preserved across pg_kalam, SQL batch execution, staged mutations, observability, and the final durable commit batch. Format: `tx-{owner_id}-{counter}`.
- **Write Operation**: An individual DML operation (insert, update, or delete) captured within a transaction's buffer. Includes the target table, operation type, and row data.
- **Transaction Snapshot**: A point-in-time view of committed data established when a transaction begins. Uses a global `commit_seq` counter (not per-row `_seq`). Used to determine which committed rows are visible to the transaction.
- **Committed Version Visibility Marker**: Hidden system metadata stored with each committed user/shared row version (for example `_commit_seq`). Used to choose the newest committed version visible to a given snapshot before applying tombstone rules.
- **Transaction Raft Binding**: In cluster mode, the leader/group affinity for an explicit transaction. A transaction may begin unbound, but the first table access pins it to one data Raft group leader until commit, rollback, or abort.

## Assumptions

- **Isolation Level**: The initial implementation will target Read Committed or Snapshot Isolation semantics (each query within a transaction sees the latest committed data plus its own writes). Serializable isolation is out of scope.
- **Atomic Commit Primitive**: Transaction commits will rely on the storage layer's atomic commit primitive for the final durable apply step. Rollback before commit is achieved by discarding the staged write set rather than undoing partially applied writes.
- **Single-Node Focus**: This feature targets single-node KalamDB deployments for GA. Any later cluster-mode enablement before a dedicated distributed transaction design exists is limited to one data Raft group with leader-local open state and abort-on-failover behavior.
- **Table Scope**: Transactional support in this phase is limited to user and shared tables. Stream tables remain non-transactional until a future design defines their transactional model.
- **No Savepoints**: Nested transactions and savepoints (SAVEPOINT / RELEASE / ROLLBACK TO) are explicitly out of scope for this feature.
- **No DDL in Transactions**: DDL statements (CREATE TABLE, ALTER TABLE, DROP TABLE) within a transaction are out of scope. DDL continues to execute with autocommit semantics.
- **Transaction Timeout Default**: The default transaction timeout will be 5 minutes, configurable via server settings. This is a reasonable default for interactive workloads while preventing resource leaks.
- **Write Buffer Size Limit**: The server will enforce a maximum write buffer size per transaction to prevent memory exhaustion. A reasonable default is 100MB. Transactions exceeding this limit are aborted with an error.
- **SQL Request Ownership**: `/v1/api/sql` explicit transactions use a lightweight request-scoped owner identifier only when `BEGIN` is encountered. They do not populate the pg `SessionRegistry` or `system.sessions`.
- **Active-Only Transaction View**: `system.transactions` is an active-only virtual view computed from in-memory handles. It is not a persisted audit trail and does not retain committed or rolled-back rows after cleanup.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A multi-statement transaction (INSERT into two tables + COMMIT) via pg_kalam completes successfully and both rows are visible, with the total roundtrip taking less than 50ms for small payloads on localhost.
- **SC-002**: A ROLLBACK after multiple writes discards 100% of uncommitted changes — zero stale rows remain.
- **SC-003**: Read-your-writes within a transaction returns all uncommitted changes from the current session with 100% accuracy.
- **SC-004**: Uncommitted writes are invisible to concurrent sessions in 100% of test cases.
- **SC-005**: An unexpected client disconnect with an active transaction results in automatic rollback within the session cleanup window (configurable, default 30 seconds).
- **SC-006**: The system supports at least 100 concurrent active transactions without performance degradation.
- **SC-007**: Autocommit mode (no explicit BEGIN) continues to work identically to pre-transaction behavior — zero regressions in existing tests.
- **SC-008**: Existing pg_kalam e2e performance tests (sequential insert 100, 1k) continue to pass with no throughput regression greater than 5%.
- **SC-009**: Automated tests validate `BEGIN`, `COMMIT`, and `ROLLBACK` through KalamDB's native SQL execution path with 100% pass rate.
- **SC-010**: Autocommit read and write performance without explicit transactions regresses by no more than 5% in targeted baseline benchmarks.
- **SC-011**: In pg_kalam transaction tests, the same transaction ID is observable from `BeginTransaction` through commit or rollback handling with 100% consistency.
- **SC-012**: Automated tests validate that a single `/v1/api/sql` request can contain multiple sequential transaction blocks and that any still-open blocks are rolled back automatically when the request terminates.
- **SC-013**: Automated tests verify that explicit transactions succeed for user/shared tables and are rejected cleanly for stream tables.
- **SC-014**: Each PG backend using multiple foreign server configs maintains independent sessions and transactions — verified by integration test showing config A's transaction does not interfere with config B's.
- **SC-015**: The `system.sessions` view correctly shows transaction_id, transaction_state, and transaction_has_writes for active pg sessions only.
- **SC-016**: Automated tests validate that active transactions from both pg_kalam and `/v1/api/sql` appear in `system.transactions` while active and disappear immediately after commit, rollback, timeout, or request-end cleanup.
- **SC-017**: Querying `system.transactions` with 100 active in-memory transactions completes from a single on-demand memory snapshot without requiring persistent storage writes or background sampling.
- **SC-018**: Repeating an empty `BEGIN; ROLLBACK;` cycle 10,000 times does not produce unbounded memory growth and keeps localhost roundtrip latency within the existing transaction budget.
- **SC-019**: A hot-table fanout stress test with 10,000 active subscriptions preserves per-table notification order for delivered events and keeps memory bounded by the configured notification and buffering caps.
- **SC-020**: Repeated race/fault-injection tests covering `COMMIT` vs `ROLLBACK`, timeout, session close, and request-end cleanup finish with zero leaked active transactions and zero orphaned staged write sets after each run.
- **SC-021**: In cluster mode, a single-group explicit transaction remains leader-affine from first table access through commit, and operations reaching a follower/frontdoor are forwarded or rejected without duplicate staging or partial durable writes.
- **SC-022**: A leader change during an open explicit transaction results in an aborted transaction with zero leaked active coordinator state and zero partial committed rows.
- **SC-023**: Snapshot-isolation tests where one logical row has multiple committed versions across the snapshot boundary always return the newest version with `commit_seq <= snapshot_commit_seq`; they never return a newer post-snapshot version and never report a false "row missing" result caused by resolving only on `_seq`.
