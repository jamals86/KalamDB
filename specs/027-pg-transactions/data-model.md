# Data Model: PostgreSQL-Style Transactions for KalamDB

**Last Updated**: 2026-04-07

## Entity: RemotePgSession (IMPLEMENTED ✅)

- Purpose: Server-side mutable session state tracking for each PostgreSQL backend connection. **This is the anchor entity for pg-origin transactions** — each pg transaction is owned by exactly one session, and the session_id is the correlation key across the pg transport path.
- Location: `backend/crates/kalamdb-pg/src/session_registry.rs`
- Fields:
  - `session_id: String`: Unique identifier, format `pg-<pid>-<config_hash_hex>` for pg_kalam sessions.
  - `current_schema: Option<String>`: Active schema/search_path for this session.
  - `transaction_id: Option<String>`: Canonical transaction ID if a transaction is active (format: `tx-{session_id}-{counter}`).
  - `transaction_state: Option<TransactionState>`: Current lifecycle state of the session's transaction.
  - `transaction_has_writes: bool`: Whether the current transaction has performed any DML writes.
  - `opened_at_ms: i64`: Timestamp when the session was created.
  - `last_seen_at_ms: i64`: Timestamp of the most recent RPC activity.
  - `client_addr: Option<String>`: Remote address of the client (from gRPC metadata).
  - `last_method: Option<String>`: Name of the last RPC method invoked on this session.
- Validation rules:
  - A session may own at most one active transaction at a time.
  - When `begin_transaction` is called while a stale `Active` transaction exists, it is auto-rolled-back with a warning log.
  - Session removal MUST rollback any active transaction before dropping state (currently a known gap — see plan.md).

## Entity: RemoteStateRegistry (IMPLEMENTED ✅)

- Purpose: Per-process registry on the pg_kalam extension side mapping foreign server configs to remote connection state. Enables multiple foreign server configs per PG backend with independent session and transaction lifecycles.
- Location: `pg/src/remote_state.rs`
- Fields:
  - `by_config: HashMap<RemoteServerConfig, Arc<RemoteExtensionState>>`: Primary index keyed by server config.
  - `by_session_id: HashMap<String, Arc<RemoteExtensionState>>`: Secondary index for session-id-based lookup (used by fdw_xact for transaction resolution).
  - `exit_handler_registered: bool`: Whether the process-exit callback has been registered.
- Behavior:
  - `ensure_remote_extension_state(config)`: Returns existing state or creates new one (opens session, registers exit handler).
  - `get_remote_extension_state_for_session(session_id)`: Looks up state by session_id (used by fdw_xact at transaction commit/rollback time).
  - `on_proc_exit_close_sessions()`: Iterates all states, calls `close_session` for each, handles panics via `catch_unwind`.

## Entity: SessionRegistry (IMPLEMENTED ✅)

- Purpose: Concurrent server-side registry for all PostgreSQL backend sessions. Manages session lifecycle and transaction state metadata.
- Location: `backend/crates/kalamdb-pg/src/session_registry.rs`
- Fields:
  - `sessions: Arc<DashMap<String, RemotePgSession>>`: Concurrent session map keyed by session_id.
  - `tx_counter: AtomicU64`: Monotonic counter for generating unique transaction IDs per session.
- Key Methods:
  - `begin_transaction(session_id) -> Result<String, String>`: Creates canonical tx_id, auto-rolls-back stale transactions.
  - `commit_transaction(session_id, transaction_id) -> Result<String, String>`: Validates state and ID match, clears transaction.
  - `rollback_transaction(session_id, transaction_id) -> Result<String, String>`: Idempotent rollback, clears transaction.
  - `mark_transaction_writes(session_id)`: Flags the session's transaction as having performed writes.
  - `snapshot() -> Vec<RemotePgSession>`: Point-in-time snapshot for `system.sessions` view.

## Entity: RequestTransactionState

- Purpose: Lightweight request-local holder for explicit transactions opened through `/v1/api/sql`.
- Fields:
  - `request_owner_id: String`: request-scoped owner identifier, for example `sql-req-<request_id>`.
  - `owner_key: ExecutionOwnerKey`: compact internal owner key derived from the request owner ID for hot-path coordinator lookups.
  - `transaction_id: Option<TransactionId>`: active transaction for the request, if any.
- Validation rules:
  - Created only when an explicit transaction statement is encountered.
  - Does not create a `RemotePgSession` or populate `SessionRegistry`.
  - Cleared on `COMMIT`, `ROLLBACK`, or request-end cleanup.

## Entity: ExecutionOwnerKey

- Purpose: Compact internal owner identifier used by `TransactionCoordinator` on the hot path. Replaces the old approach of carrying raw `session_id` / request strings through every coordinator lookup.
- Representative variants:
  - `PgSession { backend_pid: u32, config_hash: u64 }`
  - `SqlRequest { request_nonce: u64 }`
  - `Internal { source_nonce: u64 }`
- Boundary behavior:
  - APIs and system views still expose a human-readable `owner_id` label such as `pg-1234-a1b2...` or `sql-req-abc123`.
  - The compact key is internal only; it is never persisted as an opaque byte string in the user-facing contract.
- Validation rules:
  - One `ExecutionOwnerKey` may own at most one active transaction at a time.
  - `PgSession` ownership must map 1:1 with the config-scoped session model from `SessionRegistry`.
  - `SqlRequest` ownership must never survive beyond its request boundary.

## Entity: TransactionHandle

- Purpose: Represents the hot metadata for the single active explicit transaction owned by a pg session or a lightweight request-scoped owner. This entity intentionally excludes the cold staged-write buffer.
- Fields:
  - `transaction_id`: unique typed identifier for the transaction and the canonical end-to-end transaction ID.
  - `owner_key: ExecutionOwnerKey`: compact internal owner identifier used by the coordinator.
  - `owner_id: String`: human-readable owner label for logs, RPCs, and `system.transactions`.
  - `origin`: `PgRpc` or `SqlBatch` source of the transaction.
  - `raft_binding: TransactionRaftBinding`: single-node marker or cluster-mode leader/group binding for the transaction.
  - `state`: `OpenRead`, `OpenWrite`, `Committing`, `Committed`, `RollingBack`, `RolledBack`, `TimedOut`, or `Aborted`.
  - `snapshot_commit_seq: u64`: the global `commit_seq` counter value captured at `BEGIN`. All reads see committed data with `commit_seq <= snapshot_commit_seq`. This is NOT the per-row `_seq` Snowflake ID — see Research Decision 1.
  - `started_at: Instant`: server timestamp when the transaction began.
  - `last_activity_at: Instant`: timestamp of the most recent read or write in the transaction.
  - `write_count: usize`: number of staged mutations.
  - `write_bytes: usize`: approximate in-memory size of the staged write set, measured as the sum of serialized payload byte sizes across all `StagedMutation` entries.
  - `touched_tables: HashSet<TableId>`: set of `TableId` values referenced by staged mutations.
  - `has_write_set: bool`: whether the cold staged-write structure has been allocated yet.
- Validation rules:
  - A pg session or SQL request owner may own at most one `Active` transaction at a time.
  - `transaction_id` must remain stable for the full lifecycle of the transaction.
  - In cluster mode, once `raft_binding` moves to a bound data group, every later transactional read or write must resolve to that same group.
  - In cluster mode, leader change for the bound group forces the transaction into `Aborted` unless commit or rollback has already sealed it.
  - `snapshot_commit_seq` is immutable after `BEGIN`.
  - `write_bytes` must stay below the configured transaction memory limit.
  - `state` transitions are monotonic and terminal once committed or rolled back.
  - `OpenRead` transactions must not allocate a staged write buffer until the first write is staged.

## Entity: TransactionRaftBinding

- Purpose: Captures how an explicit transaction aligns with KalamDB's existing Raft routing model.
- Representative states:
  - `LocalSingleNode`: single-node deployment; no cluster binding required.
  - `UnboundCluster`: cluster-mode transaction has begun but has not yet touched a user/shared table, so no data Raft group has been selected.
  - `BoundCluster { group_id: GroupId, leader_node_id: NodeId }`: cluster-mode transaction is pinned to one data Raft group and the leader that currently owns it.
- Validation rules:
  - The first table read or write in cluster mode must transition `UnboundCluster -> BoundCluster`.
  - Once bound, all later transactional table access must resolve to the same `group_id` for the lifetime of the transaction.
  - Binding is leader-local and is not replicated to followers before `COMMIT`.
  - If `leader_node_id` no longer matches the current leader for the bound group, the transaction must abort rather than migrate live in-memory state.

## Entity: TransactionWriteSet

- Purpose: Cold staged-write state owned by an active transaction and allocated only when the transaction stages its first write. This is the sole open-transaction source of truth for staged insert/update/delete changes and the source from which post-commit side-effect planning is derived.
- Fields:
  - `transaction_id`: owning transaction.
  - `next_mutation_order: u64`: next monotonic in-transaction write sequence.
  - `ordered_mutations: Vec<StagedMutation>`: canonical write order for commit replay.
  - `latest_by_table_key: HashMap<TableId, HashMap<String, usize>>`: latest mutation index per primary key for overlay resolution.
  - `overlay_cache: HashMap<TableId, TransactionOverlay>`: derived table-local overlay views reused by reads.
  - `buffer_bytes: usize`: tracked memory usage for limit enforcement.
  - `savepoint_marks: Vec<u64>`: reserved for a future savepoint/checkpoint model; empty in Phase 1.
- Validation rules:
  - Absent for read-only and empty transactions.
  - Contains user/shared-table mutations only; stream-table operations are rejected before they can enter the write set.
  - Live fanout and publisher release work is derived from this write set during commit sealing rather than maintained as a separate always-live queue while the transaction is open.
  - Dropped immediately after commit or rollback cleanup finishes.
  - `buffer_bytes` must match the aggregate of staged payload sizes closely enough for limit enforcement.

## Entity: StagedMutation

- Purpose: Stores one logical DML operation buffered inside a transaction.
- Fields:
  - `transaction_id`: owning transaction and canonical correlation ID copied from `TransactionHandle`.
  - `mutation_order`: monotonically increasing order inside the transaction.
  - `table_id`: target table.
  - `table_type`: `Shared` or `User` in this phase. `Stream` is reserved for future support but is not valid for explicit transactions now.
  - `user_id`: optional subject user for user-scoped tables.
  - `operation_kind`: `Insert`, `Update`, or `Delete`.
  - `primary_key: String`: canonical primary key value (serialized as string) for overlay and conflict lookup.
  - `payload`: row payload or update payload after coercion/validation.
  - `tombstone`: whether the visible result for this key is deleted.
- Validation rules:
  - Every staged mutation must reference a valid `TableId` and a resolved primary key.
  - `table_type` must be `Shared` or `User` for Phase 1 explicit transactions.
  - Mutations are ordered per transaction and later mutations on the same key override earlier visible state.
  - Delete mutations must behave like tombstones in the overlay model.

## Entity: TransactionOverlay

- Purpose: Query-time projection of staged mutations used to implement read-your-writes.
- Fields:
  - `transaction_id`: owning transaction.
  - `entries_by_table`: per-table map of primary-key to latest visible staged state.
  - `inserted_keys`: keys first created in the transaction.
  - `deleted_keys`: keys hidden by transaction-local deletes.
  - `updated_keys`: keys whose visible row differs from the committed snapshot.
- Validation rules:
  - Overlay resolution must always prefer the latest staged mutation for a key.
  - Deleted keys suppress both committed snapshot rows and earlier inserted rows in the same transaction.
  - Overlay data is derived from `TransactionWriteSet` and must be discarded on rollback.

## Entity: TransactionQueryContext

- Purpose: Lightweight transaction metadata injected into DataFusion sessions so `kalamdb-tables` can apply read-your-writes and snapshot visibility without depending directly on `kalamdb-core`.
- Location: shared transaction/query crate (for example `backend/crates/kalamdb-transactions/src/query_context.rs`).
- Fields:
  - `transaction_id`: active typed transaction identifier.
  - `snapshot_commit_seq`: committed snapshot boundary captured at `BEGIN`.
  - `overlay_view`: lightweight trait-backed handle for reading transaction-local overlay state during provider scans.
- Validation rules:
  - Stored in a dedicated transaction-specific session extension, not in `SessionUserContext`.
  - Must carry only lightweight handles; it MUST NOT clone the full staged write set into every `SessionContext`.
  - Must be attachable from both the typed pg path and the SQL executor path.

## Entity: CommittedVersionVisibilityMarker

- Purpose: Hidden per-row commit-order metadata persisted with every committed user/shared row version in hot storage and Parquet so snapshot reads can choose the newest visible committed version.
- Representative shape:
  - `_commit_seq: u64`
- Validation rules:
  - Stamped during the durable apply path for both autocommit writes and explicit transaction commits.
  - Persisted in row codecs, Arrow batches, and Parquet projections used for version resolution.
  - Used before `_seq` tie-breaks when resolving the latest visible committed version for a snapshot.
  - Not exposed by default in ordinary user projections.

## Entity: TransactionCommitResult

- Purpose: Captures the outcome of applying a transaction.
- Fields:
  - `transaction_id`: committed or aborted transaction.
  - `outcome`: `Committed` or `RolledBack`.
  - `affected_rows`: total rows changed across all staged mutations.
  - `committed_at`: timestamp for successful commits.
  - `failure_reason`: optional structured error for aborted commits.
  - `emitted_side_effects: TransactionSideEffects`: typed struct containing `notifications_sent: usize` (live query notification count), `manifest_updates: usize` (manifests written), `publisher_events: usize` (topic events emitted). Provides an auditable summary of side effects released after commit.
- Validation rules:
  - `committed_at` is present only for successful commits.
  - Side effects are emitted only after durable commit succeeds.
  - `transaction_id` matches the originating `TransactionHandle` and all included `StagedMutation` records.

## Entity: CommitSideEffectPlan

- Purpose: Ephemeral post-commit plan derived once from the committed `TransactionWriteSet` after durable apply succeeds. It carries grouped deferred side effects out of the transaction coordinator after commit is sealed.
- Fields:
  - `transaction_id`: committed transaction.
  - `notifications: Vec<FanoutDispatchPlan>`: live-query fanout work grouped for post-commit delivery.
  - `publisher_events: usize`: number of topic publisher events to release.
  - `manifest_updates: usize`: number of manifest or file-lifecycle updates to finalize.
- Validation rules:
  - Built once from the committed `TransactionWriteSet`, but executed only after the transaction transitions out of `Committing`.
  - Acts as the single commit-local debug and dispatch surface for committed inserts/updates/deletes instead of keeping separate per-feature side-effect queues while the transaction is open.
  - Contains only user/shared-table committed changes; stream tables never enter it.
  - Never persisted solely for observability.
  - Dropped immediately after execution or terminal cleanup.

## Entity: FanoutDispatchPlan

- Purpose: Post-commit, table-scoped notification work item released to the live-query notification system after a transaction commits.
- Fields:
  - `table_id`: changed table.
  - `owner_scope`: user-scoped or shared fanout routing scope.
  - `change_count: usize`: number of committed row changes represented by the plan.
  - `projection_groups: usize`: number of distinct projection groups expected during dispatch.
  - `serialization_groups: usize`: number of wire-format groups expected during dispatch.
  - `seq_upper_bound: Option<SeqId>`: highest committed sequence exposed by the change set, if applicable.
- Validation rules:
  - Built only from existing in-memory `ConnectionsManager` indices; it must not require scanning all connections.
  - Represents committed insert/update/delete changes only after durable commit succeeds; rolled-back mutations never produce a fanout plan.
  - Executed only after durable commit succeeds.
  - Preserves per-table ordering by flowing through the existing notification worker sharding.

## Entity: ActiveTransactionMetric

- Purpose: Observability projection backing the `system.transactions` virtual view and lightweight transaction metrics.
- Fields:
  - `transaction_id`
  - `owner_id`: human-readable execution owner identifier derived from `TransactionHandle.owner_id`
  - `state`
  - `age_ms`
  - `idle_ms`
  - `write_count`
  - `write_bytes`
  - `touched_tables_count`
  - `snapshot_commit_seq`
  - `origin: TransactionOrigin` — `PgRpc`, `SqlBatch`, or `Internal` (see Research Decision 17).
- Validation rules:
  - Only active transactions are exposed in live metrics.
  - Metric rows disappear immediately after commit or rollback.
  - Rows are computed from the coordinator's in-memory active handle map only; no storage scan or historical persistence is required.

## Relationships

```
RemoteStateRegistry (pg extension side)
    │
    ├── by_config: RemoteServerConfig → RemoteExtensionState
    │                                      └── session_id (unique per config)
    └── by_session_id: session_id → RemoteExtensionState
                                       └── used by fdw_xact for tx resolution

SessionRegistry (server side)         RequestTransactionState (SQL path)    TransactionCoordinator (server side)
  │                                  │                                      │
  ├── DashMap<session_id,            ├── request_owner_id                   ├── active_by_owner: DashMap<ExecutionOwnerKey, TransactionId>
  │   RemotePgSession>               ├── owner_key                          ├── active_by_id: DashMap<TransactionId, TransactionHandle>
  │       └── transaction_id ────────────────────────────────────────────── │       ├── write_sets: DashMap<TransactionId, TransactionWriteSet>
  │       └── transaction_state                                           │       ├── commit_sequence_tracker (shared committed snapshot source)
  │       └── transaction_has_writes                                      │       └── active_metrics() → system.transactions rows
  │
  └── tx_counter (generates tx IDs)
```

- One `RemoteStateRegistry` (per PG process) contains zero or more `RemoteExtensionState` entries, one per foreign server config.
- One `RemoteExtensionState` maps to exactly one `RemotePgSession` in the server's `SessionRegistry`.
- One `RemotePgSession` owns zero or one active `TransactionHandle` for `PgRpc` origin (via shared `transaction_id`).
- One `RequestTransactionState` owns zero or one active `TransactionHandle` for `SqlBatch` origin and does not create a `RemotePgSession`.
- One `TransactionHandle` always carries one `TransactionRaftBinding` describing whether it is single-node, unbound in cluster mode, or pinned to one data-group leader.
- One `TransactionHandle` owns zero or one `TransactionWriteSet`.
- One `TransactionWriteSet` owns zero or many `StagedMutation` records, materializes `TransactionOverlay` views for reads, and remains the only open-transaction source of truth for deferred commit side effects.
- One `TransactionQueryContext` exposes the active transaction's snapshot boundary plus a lightweight overlay handle to query providers without cloning the full write set.
- One committed user/shared row version carries one hidden `CommittedVersionVisibilityMarker` (`_commit_seq`) used during snapshot resolution.
- One finished `TransactionHandle` yields one `TransactionCommitResult`.
- One committed `TransactionHandle` may yield one `CommitSideEffectPlan`, derived once from the committed `TransactionWriteSet`, which in turn contains zero or many `FanoutDispatchPlan` items.
- The same `transaction_id` is preserved across `RemotePgSession` or `RequestTransactionState`, `TransactionHandle`, `StagedMutation`, `system.transactions` rows, and `TransactionCommitResult`.

## State Transitions

### TransactionHandle

- `OpenRead -> OpenWrite`
  - Trigger: first staged write allocates the `TransactionWriteSet`.
- `OpenRead -> Committing`
  - Trigger: successful `COMMIT` on a read-only or empty transaction.
- `OpenWrite -> Committing`
  - Trigger: successful `COMMIT` after staged-write validation.
- `OpenRead -> RollingBack`
  - Trigger: explicit `ROLLBACK`, session close, or request-end cleanup before any write is staged.
- `OpenWrite -> RollingBack`
  - Trigger: explicit `ROLLBACK`, session close, or request-end cleanup with a staged write set present.
- `OpenRead -> TimedOut`
  - Trigger: timeout sweep finds an idle read-only transaction.
- `OpenWrite -> TimedOut`
  - Trigger: timeout sweep finds an idle write transaction.
- `TimedOut -> RollingBack`
  - Trigger: internal ordered cleanup begins.
- `Committing -> Committed`
  - Trigger: durable apply succeeds, the shared commit-sequence source stamps the committed `commit_seq`, and the owner is detached from the active-owner index.
- `RollingBack -> RolledBack`
  - Trigger: cold write state is dropped and owner/request/session bookkeeping is cleared.
- `OpenRead | OpenWrite | RollingBack -> Aborted`
  - Trigger: fatal owner loss or shutdown path where the transaction cannot return a normal explicit rollback response.

### TransactionRaftBinding

- `LocalSingleNode -> LocalSingleNode`
  - Trigger: all transaction activity in standalone mode.
- `UnboundCluster -> BoundCluster`
  - Trigger: first transactional table access resolves the owning `GroupId` and current leader.
- `BoundCluster -> BoundCluster`
  - Trigger: later operations hit the same data group and same leader.
- `BoundCluster -> Aborted`
  - Trigger: the group's leader changes before commit or rollback is sealed.

### Staged key visibility inside one transaction

- `Absent -> Inserted`
- `Inserted -> Updated`
- `Inserted -> Deleted`
- `CommittedSnapshotRow -> Updated`
- `CommittedSnapshotRow -> Deleted`

The final visible transaction-local state for each primary key is the last staged mutation in `mutation_order`.

## Entity: CommitSequenceTracker

- Purpose: Durable apply-path source of committed snapshot boundaries for both autocommit and explicit-transaction writes.
- Representative fields:
  - `current_committed: AtomicU64`: most recent committed sequence observed by the coordinator.
  - `durable_high_watermark: u64`: last persisted or recovered committed sequence available on startup.
- Behavior:
  - Lives beside the write/apply path (Raft/state machine or a dedicated commit-sequence service), not only inside `TransactionCoordinator`.
  - On `BEGIN`: the coordinator reads the latest committed value and stores it as `snapshot_commit_seq`.
  - On autocommit or explicit `TransactionCommit` apply: the next `commit_seq` is allocated once for that apply cycle and stamped into `_commit_seq` on every committed row version written by that cycle.
  - Returns the committed `commit_seq` to the coordinator so commit state can be sealed without inventing a second counter.
  - Must be persisted or recovered from durable metadata / stored row visibility markers on server restart.
- Validation rules:
  - `commit_seq` is strictly monotonically increasing across autocommit and explicit transactions.
  - The tracker MUST NOT be confused with the per-row `_seq` Snowflake ID, which remains a version/tie-break identifier rather than the snapshot boundary.

## Entity: TransactionSideEffects

- Purpose: Typed struct summarizing side effects released after a successful transaction commit.
- Fields:
  - `notifications_sent: usize`: count of live query notifications emitted.
  - `manifest_updates: usize`: count of manifest files updated.
  - `publisher_events: usize`: count of topic publisher events emitted.
- Validation rules:
  - Side effects are emitted only after durable commit succeeds.
  - This struct is informational for observability; it does not need to be persisted.

## Enum: TransactionOrigin

- Purpose: Type-safe identification of how a transaction was initiated.
- Variants:
  - `PgRpc`: Transaction initiated via pg gRPC session (PostgreSQL FDW).
  - `SqlBatch`: Transaction initiated via `/v1/api/sql` HTTP endpoint.
  - `Internal`: Transaction initiated internally by the server (e.g., system operations).
- Usage: Used in `TransactionHandle.origin` and `ActiveTransactionMetric.origin`. Replaces the previously inconsistent `caller_kind` naming.
