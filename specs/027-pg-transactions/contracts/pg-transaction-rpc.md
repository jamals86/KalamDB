# Contract: pg Transaction RPC

**Last Updated**: 2026-04-07 (session foundation details added)

## Purpose

Define the externally visible behavior of the existing pg RPC transaction lifecycle once server-side transactional semantics are implemented.

## Transport

- Service: `kalamdb.pg.PgService`
- Existing message shapes remain unchanged.
- Session continuity is keyed by `session_id`.
- The `transaction_id` returned by `BeginTransaction` is the canonical transaction identifier used through staging, observability, cleanup, and durable commit.

## Session Model (IMPLEMENTED ✅)

Sessions are the **central management entity** for transactions. The session_id is the key that connects the entire transaction lifecycle:

| Layer | Component | Session Key |
|-------|-----------|-------------|
| pg extension | `RemoteStateRegistry` | Config-scoped: `pg-<pid>-<config_hash>` |
| pg extension | `fdw_xact::CURRENT_TX` | HashMap keyed by session_id |
| gRPC transport | `PgServiceClient` | session_id in every RPC |
| Server registry | `SessionRegistry` | DashMap keyed by session_id |
| Server coordinator | `TransactionCoordinator` | TransactionHandle.session_id |
| Observability | `system.sessions` | session_id as primary key for pg transport sessions |
| Cross-origin observability | `system.transactions` | owner_id / transaction_id for active transactions across pg and SQL batch |

**Session ID format**: `pg-<pid>-<config_hash_hex>` where:
- `<pid>` is the PostgreSQL backend process ID
- `<config_hash_hex>` is a 16-character hex hash of the `RemoteServerConfig` (host, port, auth)

**Transaction ID format**: `tx-{session_id}-{counter}` where:
- `{session_id}` is the owning session's ID
- `{counter}` is a monotonic per-registry counter (`tx_counter: AtomicU64`)

`system.sessions` remains pg-session-focused. Cross-origin active transaction visibility belongs in `system.transactions`.

## Lifecycle Methods

### `BeginTransaction(session_id)`

- Input:
  - `session_id`: non-empty session identifier.
- Success result:
  - returns a non-empty `transaction_id`.
  - captures a transaction snapshot for that session.
  - marks the session as having one active explicit transaction.
  - establishes the canonical transaction ID that all later staged mutations for that session are associated with until commit or rollback.
- Failure cases:
  - empty `session_id` -> `INVALID_ARGUMENT`
  - session unavailable or server cannot allocate transaction state -> `FAILED_PRECONDITION` or `INTERNAL`
  - nested `BEGIN` on an already active transaction -> `FAILED_PRECONDITION`

### `CommitTransaction(session_id, transaction_id)`

- Input:
  - `session_id`: non-empty session identifier.
  - `transaction_id`: must match the active transaction.
- Success result:
  - applies all staged user/shared-table writes atomically.
  - makes committed rows visible to other sessions.
  - releases live query notifications, publisher events, and other deferred side effects only after durable commit succeeds.
  - derives that post-commit release work from one commit-local plan built from the transaction's staged writes, so inserts/updates/deletes are not fanned out incrementally while the transaction is still open.
  - never includes stream-table writes in the commit payload because stream tables are rejected before staging.
  - returns the committed `transaction_id`.
- Failure cases:
  - empty identifiers -> `INVALID_ARGUMENT`
  - no active transaction or ID mismatch -> `FAILED_PRECONDITION`
  - validation/storage failure before durable commit -> request fails and transaction is rolled back

### `RollbackTransaction(session_id, transaction_id)`

- Input:
  - `session_id`: non-empty session identifier.
  - `transaction_id`: active transaction identifier.
- Success result:
  - discards all staged writes.
  - clears the active transaction from the session.
  - returns the rolled back `transaction_id` or empty string for idempotent no-op rollback where no active transaction exists.
- Failure cases:
  - empty `session_id` -> `INVALID_ARGUMENT`
  - mismatched `transaction_id` -> `FAILED_PRECONDITION`

## DML and Scan Semantics While a Transaction Is Active

### `Insert`, `Update`, `Delete`

- Requests continue to use the existing fields: table identity, `session_id`, optional `user_id`, and payload.
- If the referenced session has an active transaction:
  - stream-table targets are rejected before staging because they are outside the persisted explicit-transaction model.
  - the server resolves the active canonical `transaction_id` from session state.
  - DML is staged in that transaction instead of being applied immediately.
  - live query fanout, publisher release, and other side effects are not emitted yet; they remain deferred until successful `COMMIT`.
  - the mutation participates in the eventual `COMMIT` or `ROLLBACK` under that same transaction ID.
- If the session has no active transaction:
  - behavior remains autocommit and matches current pre-transaction semantics.

## Post-Commit Side-Effect Rules

- The open transaction keeps one source of truth for staged inserts/updates/deletes in its write set; the server does not maintain a second incremental fanout queue while the transaction is active.
- On successful `CommitTransaction`, the server derives one commit-local side-effect plan from that staged write set and releases live query notifications, publisher events, and other external effects only after durability succeeds.
- On `RollbackTransaction`, timeout, or session cleanup, the staged write set is discarded and no live/publisher side effects are released.
- Stream-table operations never enter this plan because explicit transactions reject them before staging.

### `Scan`

- If the referenced session has an active transaction:
  - results are read from the transaction snapshot plus that transaction's staged writes.
  - uncommitted writes from other sessions are never visible.
- If the session has no active transaction:
  - behavior remains unchanged.

## Raft Cluster Semantics

- In single-node mode, pg transaction behavior is unchanged by cluster concerns.
- If pg transaction support is later enabled in cluster mode, the open transaction state remains leader-local until `COMMIT`; it is not replicated to followers before the final `TransactionCommit` proposal.
- In cluster mode, the transaction may begin before a data table is touched, but the first transactional table access must bind it to exactly one data Raft group and leader.
- Once bound, all transactional scans, DML, `COMMIT`, and `ROLLBACK` calls must execute on, or be forwarded to, that bound leader.
- If a later statement resolves to a different data Raft group, the transaction fails with a clear not-supported error in this phase.
- If the bound leader changes before commit or rollback is sealed, the transaction is aborted and the client must retry from a new transaction.
- Successful `CommitTransaction` in cluster mode follows normal Raft semantics: success means quorum replication plus leader apply of the final commit entry, not "all followers have applied".

## Session Cleanup Semantics

- `CloseSession(session_id)` MUST rollback any active transaction via `TransactionCoordinator::rollback()` before removing session state from `SessionRegistry`. **⚠️ Current implementation gap**: `close_session` simply calls `remove()` — the rollback-before-remove step is tracked as T046/T047 in Phase 6.
- PostgreSQL backend exit triggers `on_proc_exit_close_sessions()` which iterates ALL registered remote states and calls `CloseSession` for each session (IMPLEMENTED ✅).
- The `fdw_xact::xact_callback()` at ABORT drains all active transactions from the per-session HashMap and issues rollback RPCs for each (IMPLEMENTED ✅).
- Transaction timeout cleanup also clears session-bound transaction state.
- Cleanup must target the same canonical `transaction_id` that was returned by `BeginTransaction`.
- Cleanup must be idempotent when `CloseSession`, timeout cleanup, and explicit `ROLLBACK` race with an in-flight commit or rollback path.

## PostgreSQL Lifecycle Alignment

- pg_kalam continues to bind remote transaction finalization to PostgreSQL transaction callbacks.
- Remote durable commit occurs at PostgreSQL `XACT_EVENT_COMMIT` (the existing hook point in `fdw_xact.rs`), NOT at `PRE_COMMIT`. This means the PostgreSQL WAL commit record has already been flushed before the KalamDB remote commit fires. If the remote commit fails at this point, PostgreSQL has already committed — this is the standard FDW best-effort finalization pattern. See Research Decision 8.
- PostgreSQL `Abort` or backend/session close must discard pending state for the canonical transaction ID without durable apply.
- DDL statements (`CREATE TABLE`, `DROP TABLE`, `ALTER TABLE`) are rejected inside explicit transactions on the pg RPC path, matching the KalamDB SQL batch behavior (see Research Decision 16).

## Compatibility Requirements

- pg_kalam does not need new request fields.
- Existing `session_id`-scoped behavior is preserved.
- Existing autocommit DML callers continue to work without explicit `BEGIN`.
- Config-scoped session IDs (`pg-<pid>-<config_hash>`) are backward-compatible — servers receiving the longer format treat it as an opaque session_id string.
- Multiple foreign server configs per PG backend produce independent sessions and transactions.
