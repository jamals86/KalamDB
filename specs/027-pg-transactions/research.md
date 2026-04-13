# Research: PostgreSQL-Style Transactions for KalamDB

**Last Updated**: 2026-04-07 (Decisions 18–41 added after session foundation, production-hardening, live fanout review, Raft alignment pass, and implementation-surface review)

## Decision 1: Use snapshot isolation for explicit transactions with a global commit sequence counter

- Decision: Explicit `BEGIN` transactions capture a snapshot boundary at transaction start using a global monotonic `commit_seq` counter (not the per-row `_seq` Snowflake ID). The transaction reads from committed data with `commit_seq <= snapshot_commit_seq` plus the transaction's own staged writes.
- Rationale: The feature spec requires read-your-writes and requires that a transaction not observe rows committed by another session after the first transaction has already started. Snapshot isolation satisfies both. However, the per-row `_seq` column (Snowflake ID from `SystemColumnsService`) is assigned at write time, not commit time — a concurrent transaction may receive a higher `_seq` but commit earlier. Using `_seq` as the snapshot boundary would produce incorrect visibility. A dedicated `commit_seq` source in the durable apply path is the correct MVCC boundary. Each committed write set is tagged with its `commit_seq`, and snapshots read "all data with `commit_seq <= my_snapshot`".
- Alternatives considered:
  - Read committed: simpler, but it fails the spec's concurrent-session acceptance scenario where a started transaction must not see later commits from another session.
  - Serializable: stronger, but materially more complex and out of scope for the first single-node implementation.
  - Use `_seq` (Snowflake ID) as snapshot boundary: rejected because `_seq` is assigned at row write time, not transaction commit time. Two concurrent transactions can interleave `_seq` assignments in ways that break snapshot visibility. A global `commit_seq` provides a clean, monotonic commit ordering.
  - Use wallclock timestamps: rejected because clock skew and non-monotonicity make this unreliable even on a single node.

## Decision 2: Put transaction orchestration in `kalamdb-core`, not in the pg RPC layer

- Decision: Add a transaction coordinator in `backend/crates/kalamdb-core` and keep `backend/crates/kalamdb-pg` limited to session/RPC lifecycle and transport validation.
- Rationale: The pg service already forwards scans and DML to `OperationService`, which currently writes immediately through `AppContext::applier()`. True transactions must coordinate reads and writes across the shared server write path, not just pg bookkeeping. This also allows the SQL REST path to reuse the same engine.
- Alternatives considered:
  - Keep all transaction logic in `kalamdb-pg`: rejected because REST SQL would need a second transaction implementation and core write/query paths would remain unaware of transaction visibility.
  - Push transactions fully into the storage crate: rejected because the MVCC skill and repo guidance prefer visibility filtering in the query/provider path, not as a leaky storage-engine-only abstraction.

## Decision 3: Stage typed mutations in memory until commit

- Decision: Buffer typed insert/update/delete operations as a per-transaction write set, then apply them on `COMMIT` through a dedicated commit routine.
- Rationale: The existing hot path applies writes immediately and emits side effects synchronously. To support rollback and all-or-nothing commit, the server must avoid mutating shared state before commit. A staged write set keeps rollback cheap and lets commit control when downstream side effects fire.
- Alternatives considered:
  - Immediate writes plus compensating rollback: rejected because it cannot guarantee atomicity across multiple tables or prevent leaked side effects.
  - Provider-local buffering only: rejected because transactions span tables and need one authoritative commit point.

## Decision 4: Implement read-your-writes in the provider scan path

- Decision: Extend request/session context with transaction metadata and merge staged writes with committed MVCC rows in the table provider scan path and direct DML lookup helpers.
- Rationale: Existing scans already resolve MVCC versions in `kalamdb-tables` providers and `base_scan`/`scan_rows`. The MVCC guidance says visibility belongs in the query path. Overlaying the transaction write set there preserves DataFusion planning and avoids SQL rewrites.
- Alternatives considered:
  - Rewrite SQL queries with transaction filters: rejected by repo guidance and would add hot-path parsing overhead.
  - Hide visibility entirely in RocksDB access methods: rejected because reads also combine Parquet plus RocksDB and already rely on provider-level resolution.

## Decision 5: Keep the pg RPC wire contract unchanged while making the returned transaction ID canonical end-to-end

- Decision: Preserve the existing `BeginTransaction`, `CommitTransaction`, and `RollbackTransaction` message shapes and continue binding DML/scan behavior to `session_id` on the server, but treat the `transaction_id` returned by `BeginTransaction` as the canonical correlation ID across staged mutations, observability, cleanup, and durable commit.
- Rationale: pg_kalam already issues the needed RPCs and tracks `session_id` plus `transaction_id`. Making that same `transaction_id` canonical end-to-end satisfies the requirement to track one transaction identity from pg_kalam until the transaction reaches RocksDB without forcing new wire fields on every DML request.
- Alternatives considered:
  - Add `transaction_id` to every scan/DML request: rejected for the first phase because it would force extension, client, and service API changes even though the session registry already tracks the active transaction.
  - Generate a second internal transaction identifier in the server: rejected because it would make observability and cleanup harder to reason about and would break the user's requirement for one stable transaction identity.

## Decision 6: Support KalamDB server SQL transaction statements through the existing SQL execution path

- Decision: KalamDB's SQL execution path itself supports `BEGIN`/`COMMIT`/`ROLLBACK`, exposed first through the existing `/v1/api/sql` multi-statement request flow. One request may contain multiple sequential transaction blocks, but no transaction created by `/v1/api/sql` may survive past the end of that request.
- Rationale: The user requirement is that transaction SQL statements work in KalamDB server itself, not only through pg RPC. The current SQL API is stateless HTTP with no transaction session token in `QueryRequest`, so request-scoped SQL batches are the lowest-risk way to make the server transaction-aware now while reusing the same core transaction engine as pg. Because the endpoint is stateless, all request-scoped transactions must be shut down automatically when the request finishes.
- Alternatives considered:
  - Add persistent HTTP transaction sessions: rejected for this phase because it introduces new transport/session contracts and is not required for pg_kalam.
  - Exclude SQL API transaction support entirely: rejected because the spec requires direct transaction control outside pg_kalam and the existing batch endpoint can satisfy that requirement.

## Decision 6A: Separate the `/v1/api/sql` transaction model from the pg gRPC transaction model

- Decision: `/v1/api/sql` transactions are request-scoped and may appear multiple times sequentially within one request, while pg gRPC transactions are session-scoped and intentionally span multiple network calls until explicit commit/rollback or session close.
- Rationale: This matches the current transport designs. The SQL HTTP endpoint has a single request body containing multiple statements and no transaction token for future requests. The pg gRPC path already has `session_id` and `transaction_id`, making it appropriate for multi-call transaction lifecycles.
- Alternatives considered:
  - Make `/v1/api/sql` mimic pg session-spanning transactions: rejected because the current REST contract has no persistent transaction session handle.
  - Force pg gRPC transactions to fit in one call: rejected because it would break the intended PostgreSQL FDW transaction lifecycle.

## Decision 7: Commit staged mutations through DmlExecutor (post-Raft apply layer), not through raw StorageBackend::batch()

- Decision: On transaction commit, replay all staged mutations through `DmlExecutor` (the post-Raft apply layer in `kalamdb-core`), which handles `_seq` generation, index updates, notification emission, publisher events, live query notifications, manifest updates, and file reference tracking. Do NOT commit through raw `StorageBackend::batch()`.
- Rationale: All writes in KalamDB currently flow through `UnifiedApplier` → `RaftApplier` → Raft consensus → state machine → `CommandExecutorImpl::dml()` → `DmlExecutor` → providers. The `DmlExecutor`/provider layer is where `_seq` is assigned, secondary indexes are updated, live query notifications fire, and topic publishing happens. Writing directly to `StorageBackend::batch()` would bypass all of that, producing rows without `_seq`, without index entries, without notifications, and without Raft log entries. For Phase 1 single-node, the commit path should propose a `TransactionCommit` command through Raft so the entire staged write set is applied atomically in one Raft apply cycle by `DmlExecutor`.
- Alternatives considered:
  - `StorageBackend::batch()` directly: rejected because it bypasses Raft consensus, MVCC `_seq` generation, secondary indexes, notifications, publisher events, and the entire provider layer.
  - Replay staged mutations as individual Raft proposals: rejected because if the server crashes after proposal N of M, you get partial commit. The entire transaction must be a single Raft proposal for atomicity.
  - Apply writes eagerly and depend on compensating undo: rejected because side effects and cross-table atomicity become fragile.
  - Introduce RocksDB `TransactionDB` as a first step: rejected for now because the existing stack already depends on atomic batch operations and the feature only needs one atomic durable apply step per commit.

## Decision 7A: TransactionCoordinator depends on Arc<AppContext>, not Arc<dyn StorageBackend>

- Decision: The `TransactionCoordinator` takes `Arc<AppContext>` as its dependency, not `Arc<dyn StorageBackend>`.
- Rationale: `StorageBackend` is a raw key-value interface (`put`/`get`/`batch`). The transaction coordinator needs to commit through typed table providers via `DmlExecutor`, which handles MVCC `_seq` generation, index updates, notification emission, publisher events, live query notifications, manifest updates, and file reference tracking. It also needs access to `SchemaRegistry` for table validation. All of these are available through `AppContext`. Committing through `StorageBackend` would skip the entire provider layer.
- Alternatives considered:
  - `Arc<dyn StorageBackend>`: rejected because it's the wrong abstraction level — transaction commit needs typed provider operations, not raw K/V writes.
  - Passing individual dependencies: rejected because `AppContext` is the established pattern for dependency injection in `kalamdb-core` and adding 5+ individual Arc parameters would be unwieldy.

## Decision 7B: Add TransactionCommit Raft command variant for atomic cross-table commit

- Decision: Add a `TransactionCommit(TransactionId, Vec<StagedMutation>)` variant to the Raft data command types. The Raft state machine applies the entire batch atomically in one apply cycle through `DmlExecutor`.
- Rationale: For true atomicity (all-or-nothing across tables), the commit must be a single Raft proposal. Currently, each INSERT/UPDATE/DELETE is a separate Raft proposal. If staged mutations were replayed as N separate Raft proposals and the server crashed after proposal K, you'd have partial commit — violating transaction atomicity. A single `TransactionCommit` command ensures the state machine processes all mutations together.
- Alternatives considered:
  - N separate Raft proposals per staged mutation: rejected because partial application on crash breaks atomicity.
  - Skip Raft entirely for transaction commit: rejected because it breaks replication consistency in cluster mode and loses durability guarantees.

## Decision 7C: Prepare Raft commands for future distributed transactions by adding Option<TransactionId>

- Decision: Add `Option<TransactionId>` to `UserDataCommand` and `SharedDataCommand` Raft enums now, defaulting to `None` for non-transactional operations.
- Rationale: If this field is not added now, any future distributed transaction support will require rewriting the Raft protocol and breaking the serialized log format. Adding an optional field now is zero-cost for non-transactional operations (serialized as a single byte) and prevents a breaking protocol change later. The `TransactionCommit` batch command uses this field; individual autocommit mutations leave it as `None`.
- Alternatives considered:
  - Add field later when distributed transactions are needed: rejected because it would require a Raft log migration, which is complex and disruptive.
  - Use a separate Raft group for transactions: rejected as unnecessary complexity for Phase 1.

## Decision 8: Mirror the PostgreSQL extension pattern used by ParadeDB for pending state finalization and abort cleanup

- Decision: Follow the same lifecycle pattern used by ParadeDB's PostgreSQL extension internals: keep pending extension-owned work in transaction-scoped state, finalize that work at PostgreSQL `XACT_EVENT_COMMIT` (the point at which the transaction is durably committed), and drop pending state on PostgreSQL `Abort`.
- Rationale: ParadeDB uses `pgrx::register_xact_callback` to manage transaction-scoped state. The existing pg_kalam code in `fdw_xact.rs` already fires its commit action at `XACT_EVENT_COMMIT` (not `PRE_COMMIT`). This is the correct hook point because at `XACT_EVENT_COMMIT` the PostgreSQL WAL commit record has been flushed, so if the KalamDB remote commit fails, PostgreSQL has already committed — but that risk is acceptable for FDW semantics and matches the architectural pattern of "best-effort remote finalization after local commit." The plan must align with the existing `fdw_xact.rs` implementation rather than incorrectly specifying `PRE_COMMIT`.
- Alternatives considered:
  - Use `PRE_COMMIT` hook: rejected because the existing pg_kalam `fdw_xact.rs` code uses `XACT_EVENT_COMMIT`, and PRE_COMMIT would require the remote server to be available during the commit critical path — any network failure would abort the PostgreSQL transaction.
  - Finalize remote work before PostgreSQL commit: rejected because it can expose committed remote state before PostgreSQL decides the transaction outcome.
  - Depend only on request boundaries instead of transaction callbacks: rejected because it does not mirror PostgreSQL transaction semantics closely enough.

## Decision 9: Roll back active transactions on session close, timeout, and incomplete SQL batches

- Decision: Active transactions are aborted on `CloseSession`, pg backend disconnect, configured transaction timeout, or when a SQL batch ends with an open transaction.
- Rationale: The current pg session lifecycle already closes sessions on PostgreSQL backend exit. Extending cleanup to abort staged writes prevents orphaned state and aligns with the feature's safety requirements.
- Alternatives considered:
  - Leave orphaned transactions for manual cleanup: rejected because it leaks memory and violates rollback guarantees.
  - Auto-commit on disconnect: rejected because it is incompatible with PostgreSQL semantics.

## Decision 10: Preserve a near-zero-cost path for non-transaction requests

- Decision: Requests that do not use explicit transactions continue through the existing autocommit path with only a cheap active-transaction presence check and no transaction overlay construction.
- Rationale: The repository guidance is performance-first, and the feature must not materially slow normal writes or reads when transactions are unused. The transaction coordinator should therefore be opt-in and bypassed quickly for ordinary requests.
- Alternatives considered:
  - Route every request through transaction machinery: rejected because it would add avoidable allocations and branching to the hot path.

## Decision 11: Limit Phase 1 to single-node transactional durability

- Decision: The initial plan guarantees explicit transactions only within one KalamDB server node and one local storage domain.
- Rationale: The spec already scopes distributed and Raft-spanning transactions out of the first implementation. This keeps the design aligned with the existing single-node `OperationService` and provider architecture.
- Alternatives considered:
  - Cross-Raft-group transactions: rejected as a separate consensus and recovery problem.
  - Savepoints and nested transactions: rejected as out of scope and unnecessary for the pg_kalam driver goal.

## Decision 12: Limit explicit transaction support to user and shared tables in Phase 1

- Decision: Explicit transactions in this phase apply only to user tables and shared tables. Stream tables are rejected when used inside an explicit transaction.
- Rationale: Stream tables currently do not support transactions, and their delivery/ordering semantics need a separate design. Making the scope explicit avoids overpromising and keeps the first implementation aligned with current capabilities.
- Alternatives considered:
  - Attempt partial stream-table transaction support now: rejected because it would introduce undefined behavior around stream semantics and expand the feature surface too far.

## Decision 13: TransactionId must follow the established type-safe newtype ID pattern

- Decision: Define `TransactionId` in `kalamdb-commons/src/models/ids/` following the established newtype pattern with `StorageKey`, `Display`, `FromStr`, `Serialize`/`Deserialize`, `From<String>`/`From<&str>`, and validation logic. Use UUID v7 (time-ordered) as the underlying format.
- Rationale: The codebase has an established pattern for type-safe IDs (`NamespaceId`, `TableId`, `UserId`, `StorageId`) in `kalamdb-commons/src/models/ids/`. Using a raw `String` for transaction identifiers (as the current pg session registry does) loses type safety and violates the project's coding guidelines. The newtype pattern prevents accidental mixing of transaction IDs with other string values, enables StorageKey derivation for RocksDB persistence, and provides validated construction. UUID v7 gives time-ordering for debugging.
- Alternatives considered:
  - Raw `String`: rejected because it violates type-safety guidelines and enables accidental misuse.
  - Numeric auto-increment: rejected because it doesn't provide enough entropy for concurrent transaction creation and is harder to correlate across pg sessions.
  - UUID v4: rejected in favor of UUID v7 because time-ordering aids debugging and log analysis.

## Decision 14: Replace the existing pg TransactionState enum with a unified version in kalamdb-commons

- Decision: Move `TransactionState` from `kalamdb-pg/src/session_registry.rs` (currently has 3 variants: `Active`, `Committed`, `RolledBack`) to `kalamdb-commons` and extend it with the additional states needed by the transaction coordinator (5 variants: `Active`, `Committing`, `Committed`, `RolledBack`, `TimedOut`). Update the pg crate to use the commons version.
- Rationale: The pg crate currently defines its own `TransactionState` with 3 variants. The transaction coordinator needs additional states (`Committing` for the in-flight commit phase, `TimedOut` for transaction timeout). Having two `TransactionState` enums in different crates creates confusion and type collision. A single enum in commons, used by both `kalamdb-core` and `kalamdb-pg`, prevents variant drift and simplifies state machine reasoning.
- Alternatives considered:
  - Keep separate enums: rejected because it creates maintenance burden and potential semantic drift.
  - Add new states only to pg crate: rejected because the coordinator in `kalamdb-core` is the authoritative state owner, not the pg transport layer.

## Decision 15: Use a TransactionOverlayExec DataFusion execution plan for scan merging

- Decision: Implement overlay scan merging via a custom `TransactionOverlayExec` DataFusion execution plan node that wraps the provider's base scan and merges staged writes. Do not attempt to merge at the `RecordBatch` level outside DataFusion's execution framework.
- Rationale: DataFusion expects execution plans to produce `RecordBatch` streams through `ExecutionPlan::execute()`. The overlay must merge staged inserts/updates and filter staged deletes with the base committed scan. Implementing this as a proper `ExecutionPlan` node means it participates in DataFusion's query planning, optimization, and execution framework — including projection pushdown and predicate pushdown from the base scan. Merging outside the execution framework would bypass query optimization and create a parallel code path.
- Alternatives considered:
  - In-memory `RecordBatch` merge after scan completes: rejected because it breaks streaming execution, loads entire result sets into memory, and bypasses DataFusion's framework.
  - Modify existing provider scan methods directly: rejected because it couples transaction awareness into the non-transactional hot path and violates the "near-zero-cost for non-transaction requests" goal (Decision 10).

## Decision 16: Reject DDL statements inside explicit transactions

- Decision: `CREATE TABLE`, `DROP TABLE`, `ALTER TABLE`, and other DDL statements are rejected with an error when issued inside an explicit `BEGIN`/`COMMIT` block, for both pg RPC and SQL batch paths.
- Rationale: DDL operations in KalamDB modify system tables, schema registry, and metadata — these have their own atomic semantics separate from user data transactions. Mixing DDL and DML in one transaction creates complex rollback requirements (e.g., rolling back a `CREATE TABLE` after the schema registry already has the entry). PostgreSQL itself handles DDL in transactions through its catalog versioning, but KalamDB's architecture doesn't support catalog rollback. Rejecting DDL keeps the transaction scope simple and correct.
- Alternatives considered:
  - Allow DDL and attempt rollback: rejected because the schema registry, system tables, and metadata paths don't support undo/rollback semantics.
  - Allow DDL but auto-commit: rejected because it would silently break atomicity expectations — users expect everything inside BEGIN/COMMIT to be atomic.
  - Log a warning but allow: rejected because it leads to undefined behavior on rollback.

## Decision 17: Standardize on `origin: TransactionOrigin` for caller identification

- Decision: Use a single `origin: TransactionOrigin` enum field (variants: `PgRpc`, `SqlBatch`, `Internal`) throughout the transaction data model. Do not use `caller_kind` as an alternative name.
- Rationale: The data model had `caller_kind` in `ActiveTransactionMetric` and `origin` in `TransactionHandle` — two names for the same concept. Using one canonical name (`origin`) with one canonical type (`TransactionOrigin`) prevents confusion, simplifies grep/search, and aligns with the pattern of using descriptive enum types rather than raw strings.
- Alternatives considered:
  - Use `caller_kind` everywhere: rejected because `origin` is more semantic and self-describing.
  - Use raw strings: rejected because type-safe enums are a core coding principle.

## Decision 18: Config-scoped sessions as the transaction anchor (IMPLEMENTED ✅)

- Decision: Each PostgreSQL backend process × foreign server config pair gets a unique session, identified by `pg-<pid>-<config_hash_hex>`. For pg-origin transactions, the session_id is the correlation key that links the pg extension state, the gRPC transport, and the server-side SessionRegistry + TransactionCoordinator.
- Rationale: Before this change, the pg extension used a single `OnceLock<RemoteExtensionState>` per backend process, meaning only one foreign server config could be used per PG backend, and a single global session covered all FDW operations. If a PG backend accessed two different KalamDB servers (or different namespaces via different foreign server configs), only the first config's connection would be used. By refactoring to a `RemoteStateRegistry` keyed by `RemoteServerConfig` (with `Hash` derived), each config gets its own `RemoteExtensionState` with a unique session_id, gRPC channel, and transaction tracking. This ensures that transactions from different configs are fully independent and observable as separate sessions in `system.sessions`.
- Implementation details:
  - `RemoteStateRegistry<T>` with dual indexing: `by_config` (HashMap<RemoteServerConfig, Arc<T>>) and `by_session_id` (HashMap<String, Arc<T>>).
  - Session ID generation: `format!("pg-{}-{:016x}", std::process::id(), session_suffix(config))` where `session_suffix` uses `DefaultHasher` on the config.
  - `RemoteServerConfig` derives `Hash` (via `config.rs` change).
  - Process exit handler iterates all registered states, closing each session.
- Alternatives considered:
  - Single global session per process (status quo): rejected because it broke multi-config scenarios and made session observability misleading.
  - Session per table: rejected because overly granular; one session per config is the right granularity matching the gRPC channel reuse model.
  - Random session IDs: rejected because the `pid-hash` format provides deterministic, debuggable session identifiers.

## Decision 19: Per-session transaction tracking in fdw_xact (IMPLEMENTED ✅)

- Decision: The `fdw_xact::CURRENT_TX` static is a `LazyLock<Mutex<HashMap<String, ActiveTransaction>>>` keyed by session_id, not a single `Option<ActiveTransaction>`. The PostgreSQL xact callback iterates all active transactions at commit/abort time, resolving the remote state per-session via `get_remote_extension_state_for_session()`.
- Rationale: With config-scoped sessions (Decision 18), a single PG backend may have multiple active sessions with independent transactions. If `CURRENT_TX` were a single `Option`, committing/rolling back would only handle one session's transaction, leaving others orphaned. The HashMap ensures that each session's transaction is independently tracked and finalized when PostgreSQL commits or aborts.
- Implementation details:
  - `ensure_transaction(session_id)` checks `CURRENT_TX[session_id]` first; if absent, begins a new transaction via `get_remote_extension_state_for_session(session_id)`.
  - `xact_callback()` at COMMIT/ABORT uses `std::mem::take()` to drain all active transactions, then iterates each, resolving state per session_id.
  - Write buffer flush at PRE_COMMIT and discard at ABORT happen before transaction iteration.
- Alternatives considered:
  - Keep single `Option<ActiveTransaction>`: rejected because it breaks multi-config scenarios.
  - One xact callback per session: rejected because PostgreSQL's `RegisterXactCallback` doesn't support per-foreign-server callbacks — the callback is process-global.

## Decision 20: Session close MUST rollback active transactions before removing session state

- Decision: The `close_session` RPC handler and any session cleanup path MUST call `TransactionCoordinator::rollback(session_id)` before removing the session from `SessionRegistry`. This prevents orphaned staged writes.
- Rationale: The current `close_session` implementation simply calls `self.session_registry.remove(session_id)` without checking for an active transaction. Once the TransactionCoordinator is implemented (Phase 2+), this gap would cause staged mutations to be leaked in memory with no cleanup path. The fix is straightforward: before removing the session, check if `session.transaction_id` is present and active, and if so, call the coordinator's rollback method.
- Implementation: Tracked as T046/T047 in Phase 6 (US5). Must be implemented when TransactionCoordinator is available.
- Alternatives considered:
  - Rely on timeout cleanup only: rejected because timeouts can be long (default 5 minutes) and the session is already gone — there's no reason to keep the staged writes around.
  - Auto-commit on close: rejected because it violates PostgreSQL semantics (PG aborts, never auto-commits, on disconnect).

## Decision 21: Expose active transactions through a dedicated lightweight `system.transactions` virtual view

- Decision: Add a new `system.transactions` virtual view that snapshots the `TransactionCoordinator`'s in-memory active transaction handles on demand. Do not route active transaction observability through `system.stats`, persisted system tables, or a background collector.
- Rationale: The existing codebase already uses lightweight virtual views such as `system.sessions`, `system.live`, and `system.stats`, all computed at query time from in-memory state with memoized Arrow schemas. Active transactions are already held in memory by the future `TransactionCoordinator`, so the cheapest correct observability path is to expose exactly that in-memory state as a dedicated view. This satisfies the need to inspect both pg and `/v1/api/sql` transactions without adding write amplification, periodic sampling, or storage pressure.
- Implementation details:
  - Add `SystemTable::Transactions` in `kalamdb-commons/src/system_tables.rs`.
  - Add `TransactionsView` in `backend/crates/kalamdb-views/src/transactions.rs` following the same pattern as `SessionsView`.
  - Wire a callback from `TransactionCoordinator::active_metrics()` through `system_schema_provider.rs`.
  - Memoize the Arrow schema with `OnceLock` and compute rows only when queried.
- Alternatives considered:
  - Extend `system.sessions` to include SQL batch rows: rejected because `system.sessions` is transport-session oriented and SQL batch requests are not long-lived pg sessions.
  - Expose only metrics counters: rejected because operators need per-transaction visibility, not just aggregates.
  - Persist active transactions in a system table: rejected because it adds extra writes and cleanup work for purely in-memory state.

## Decision 22: `/v1/api/sql` explicit transactions use lightweight request owner IDs and appear only in `system.transactions`

- Decision: When `/v1/api/sql` executes an explicit `BEGIN`, the server creates a lightweight request owner identifier, for example `sql-req-<request_id>`, and uses it as the `TransactionHandle.session_id` value for `SqlBatch` origin. These transactions appear in `system.transactions` while active but do not create entries in `SessionRegistry` or `system.sessions`.
- Rationale: SQL batch transactions need the same coordinator and observability path as pg transactions, but they do not correspond to long-lived transport sessions. Reusing the same `session_id` field in `TransactionHandle` as a generic owner ID keeps the coordinator path simple while avoiding the overhead and semantic confusion of populating pg session state for every HTTP request.
- Alternatives considered:
  - Create full `SessionRegistry` rows for every SQL request: rejected because it adds churn and makes `system.sessions` misleading.
  - Add a separate owner abstraction throughout the entire stack: rejected for now because a lightweight request owner ID achieves the same goal with less code and less hot-path complexity.

## Decision 23: Use typed compact execution-owner keys internally and derive human-readable owner labels only at boundaries

- Decision: Inside `TransactionCoordinator`, do not use raw `String` owner identifiers on the hot path. Instead, represent ownership with a typed compact `ExecutionOwnerKey` and derive a human-readable owner label only for RPC responses, logs, and virtual views.
- Rationale: PostgreSQL keeps hot transaction state keyed by compact process/XID structures, and SQLite keeps pager/connection ownership inside typed handles rather than string IDs. KalamDB already needs owner identifiers in multiple paths (`PgRpc`, `SqlBatch`, future `Internal` work). Keeping those keys as raw strings in `DashMap` lookups, transaction state, and timeout sweeps adds hashing and allocation pressure to a path that should stay cheap. The external string form still matters for compatibility and observability, but it should be a boundary concern, not the coordinator's internal identity.
- Alternatives considered:
  - Keep string owner IDs everywhere: rejected because it pushes formatting and string hashing into the hot path and couples internal data structures to transport-specific labels.
  - Create separate coordinators for pg and SQL batch: rejected because it duplicates lifecycle logic and makes cross-origin observability harder.

## Decision 24: Split hot transaction metadata from cold staged-write state and allocate the write set lazily

- Decision: Store transaction hot metadata (`transaction_id`, owner key, state, snapshot, timestamps, counters) separately from the cold staged payload (`Vec<StagedMutation>`, overlay indexes, deferred side effects). `BEGIN` allocates only the hot metadata entry. The write set is allocated on the first staged write.
- Rationale: PostgreSQL separates transaction metadata from `MemoryContext`-owned bulk allocations, and SQLite keeps autocommit/read-only paths cheap until a writer lock is needed. KalamDB needs the same property. Read-only transactions, empty transactions, and transactions that time out before their first write should not pay for an overlay map or staged mutation buffer they never use. This also makes `system.transactions` snapshots cheaper because the view reads only hot metadata.
- Alternatives considered:
  - Keep all staged mutations inline in `TransactionHandle`: rejected because every active transaction would carry cold payload fields even when no write occurs.
  - Pre-build overlay structures at `BEGIN`: rejected because it makes the empty/read-only path too expensive.

## Decision 25: Use an explicit lifecycle state machine that distinguishes read-only, write-active, commit, rollback, timeout, and abort paths

- Decision: Expand the internal transaction lifecycle beyond `Active`/`Committed`/`RolledBack`. The coordinator should distinguish at least `OpenRead`, `OpenWrite`, `Committing`, `RollingBack`, `Committed`, `RolledBack`, `TimedOut`, and `Aborted`.
- Rationale: PostgreSQL's `TransState`/`TBlockState` separation and SQLite's pager/VDBE states both exist for one reason: cleanup and failure handling become unsafe when all in-flight work is compressed into a single "active" bucket. KalamDB needs to reason about races such as `CommitTransaction` vs `CloseSession`, timeout vs explicit rollback, and request-end cleanup vs SQL `COMMIT`. These paths are only deterministic if the coordinator can see which phase the transaction is already in.
- Alternatives considered:
  - Reuse the current 3-state pg enum: rejected because it cannot distinguish in-flight commit/rollback from ordinary active state.
  - Track booleans (`has_writes`, `is_committing`, `timed_out`) instead of a state machine: rejected because combinations become harder to validate and test.

## Decision 26: Commit and rollback must run through ordered cleanup phases

- Decision: Model transaction shutdown as ordered phases instead of one monolithic "remove from map" operation.
- Commit phases:
  1. Validate ownership and move the transaction to `Committing` so no new writes enter.
  2. Perform the durable apply step and collect deferred side effects.
  3. Advance `commit_seq`, detach the owner from the active-owner index, and mark the transaction committed.
  4. Release post-commit side effects (notifications, publisher events, manifest/file updates) outside the coordinator lock.
  5. Drop the cold write set and final metadata.
- Rollback phases:
  1. Move the transaction to `RollingBack` / `TimedOut` / `Aborted` depending on cause.
  2. Detach the owner from the active-owner index so no new operations resolve to it.
  3. Drop the cold write set and overlay state.
  4. Clear transport/request bookkeeping and remove the hot metadata entry.
- Rationale: PostgreSQL separates commit durability, proc-array visibility removal, and resource-owner release; SQLite separates commit phase one/two and statement-vs-transaction rollback. KalamDB needs the same discipline so that retries, disconnect cleanup, and timeout sweeps cannot double-release work or leak staged buffers.
- Alternatives considered:
  - Single-step `DashMap::remove()` cleanup: rejected because it cannot safely order visibility removal against durable apply and side-effect release.
  - Auto-commit on close to simplify cleanup: rejected because it violates PostgreSQL-compatible semantics.

## Decision 27: Release transaction side effects through a post-commit dispatch plan

- Decision: Transaction commit produces a `CommitSideEffectPlan` (notifications, publisher events, manifest/file updates, other deferred outputs) during durable apply, but the plan is executed only after commit durability succeeds and after the coordinator has sealed the transaction. The open transaction keeps one source of truth for changes in `TransactionWriteSet`; the side-effect plan is derived once from that write set instead of maintaining separate per-feature queues while the transaction is open.
- Rationale: PostgreSQL keeps side-effect release ordered after commit visibility transitions, and SQLite avoids exposing committed state before commit phases complete. KalamDB's explicit transaction design must do the same: live query notifications, topic publishing, and file/manifest cleanup cannot fire while the transaction is still logically tentative. Deriving one compact commit-local plan from the staged write set is easier to debug and cheaper than duplicating notification/publisher state for every mutation during the full transaction lifetime. Stream tables remain excluded entirely because they are not part of the persisted explicit-transaction model.
- Alternatives considered:
  - Emit notifications while replaying staged mutations inside the commit loop: rejected because a later commit failure would have leaked externally visible effects.
  - Re-run the full DML side-effect path after commit by scanning storage: rejected because it duplicates work and delays notifications unnecessarily.

## Decision 28: Keep live query fanout node-local and candidate-scoped; do not persist a subscription catalog like Supabase Realtime

- Decision: Keep KalamDB live query subscription routing in memory, keyed by table and ownership scope (`user_id` + `table_id` for user tables, `table_id` for shared tables). Do not add a persistent subscription table or cross-node fanout layer solely to support transaction commit notifications.
- Rationale: Supabase Realtime persists subscriptions in `realtime.subscription` because it polls PostgreSQL WAL centrally, applies RLS there, and then fans out to Phoenix channels across nodes. KalamDB is different: the write path already applies locally on each node through the provider stack and already has an in-memory `ConnectionsManager` index. Persisting ephemeral live subscriptions would add write amplification and cleanup complexity without giving KalamDB the same benefit Supabase gets. The useful lesson is not the persistent table; it is the narrower candidate set before transport fanout.
- Alternatives considered:
  - Add a persistent subscription system table: rejected because active live subscriptions are already intentionally in-memory (`system.live`) and query-time.
  - Broadcast change notifications across nodes just to locate subscribers: rejected because Raft apply already happens locally and each node can notify its own subscribers after apply.

## Decision 29: Reuse payload work by projection and serialization group, and keep slow-subscriber buffers bounded

- Decision: Continue grouping candidate subscribers by shared projection requirements, and extend the design to treat wire-format/serialization requirements as part of the same cache key where needed. Keep bounded per-worker queues, per-connection channels, and per-subscription initial-load buffers; do not allow unbounded lagging-client growth.
- Rationale: Supabase Realtime's `MessageDispatcher` uses Phoenix fastlane metadata so one encoded payload can be reused across many recipients. KalamDB already applies a similar idea one level earlier by caching `SharedChangePayload` per projection group inside `notification.rs`, and it already bounds notification channels and initial-load buffers. The production-ready design should preserve those bounds and make projection/serialization reuse explicit in the transaction commit path so high-fanout commits do not serialize the same row N times.
- Alternatives considered:
  - Build and serialize one payload per subscriber: rejected because it scales linearly with fanout even when every subscriber requested the same shape.
  - Use unbounded channels to avoid drops: rejected because it converts slow subscribers into a memory leak.

## Decision 30: Require race, fault-injection, and hot-table fanout coverage before calling the design production-ready

- Decision: The implementation plan must include repeated race tests for `COMMIT` vs `ROLLBACK`, session close, timeout, and request-end cleanup, plus live fanout stress cases with hot tables, slow subscribers, and large subscriber counts.
- Rationale: PostgreSQL and SQLite earned their stability through aggressive lifecycle hardening, not just API surface design. Supabase Realtime's production code also carries explicit rate limits, heap caps, subscription cleanup, and throughput tests. KalamDB is not production-ready until it proves that the coordinator never leaks active state and the live fanout path stays bounded under skewed workloads.
- Alternatives considered:
  - Rely on happy-path integration tests only: rejected because the risky behavior lives in races and overload, not in single-transaction correctness.

## Decision 31: In Raft cluster mode, keep open explicit transactions leader-local until commit

- Decision: If explicit transactions are enabled in Raft cluster mode before a distributed transaction protocol exists, the open transaction handle, overlay, and staged write set remain leader-local in memory until `COMMIT`. They are not replicated to followers before commit. In cluster mode, a transaction may begin unbound, but its first table access must bind it to one data-group leader for the remainder of its lifetime.
- Rationale: KalamDB already routes writes to Raft leaders and returns success after quorum replication plus leader apply; followers apply later. The transaction overlay also exists only in coordinator memory. Replicating every staged mutation before commit would effectively introduce replicated intents, recovery cleanup, and transaction-record semantics similar to CockroachDB or YugabyteDB distributed transactions, which is out of scope here. PostgreSQL primary/standby behaves similarly at a higher level: the primary owns the open transaction, standbys replay WAL, and uncommitted transactions do not survive failover.
- Alternatives considered:
  - Allow any follower to host the open transaction state and only forward `COMMIT`: rejected because it splits strong-read ownership from the node that can actually commit and makes failover semantics even harder to reason about.
  - Replicate staged writes incrementally before `COMMIT`: rejected because it introduces distributed-intent cleanup and recovery complexity far beyond the current design.
  - Collapse to etcd-style single-request transactions only: rejected because pg_kalam requires session-spanning `BEGIN`/`COMMIT` semantics.

## Decision 32: Reject cross-Raft-group explicit transactions in cluster mode for this phase

- Decision: In cluster mode, explicit transactions may touch only one data Raft group for their full lifetime. If a later read or write resolves to a different `GroupId`, the transaction is rejected with a clear error instead of attempting a partial or best-effort commit.
- Rationale: KalamDB routes user and shared data deterministically to different Raft groups. Atomic commit across groups would require a real distributed transaction protocol such as two-phase commit with a replicated transaction record, prepared intents, and crash recovery. PostgreSQL physical replication is not an example of this problem because it has one writable primary, not multiple independently led shards. CockroachDB and YugabyteDB solve the cross-range case with additional replicated transaction metadata beyond a plain Raft proposal. KalamDB should reject the unsupported case rather than fake atomicity.
- Alternatives considered:
  - Replay one transaction as multiple per-group commits: rejected because a crash or leader change between groups would violate atomicity.
  - Silently auto-commit per group: rejected because it breaks the user's PostgreSQL-compatible expectations.

## Decision 33: Leader change aborts any still-open explicit transaction

- Decision: If the leader for the transaction's bound data group changes before `COMMIT` or `ROLLBACK` finishes, the transaction is aborted and the client must retry from a new transaction.
- Rationale: Open transaction state is leader-local and not replicated before commit. KalamDB's current leader-forwarding logic can retry proposals against a new leader, but that only helps for requests that have not accumulated private in-memory transaction state. Once a transaction has an overlay and staged writes, there is nothing durable to migrate. PostgreSQL failover also discards uncommitted transactions; only committed WAL survives promotion.
- Alternatives considered:
  - Migrate the in-memory transaction to the new leader: rejected because there is no replicated transaction record, overlay snapshot, or lock/intent state to transfer safely.
  - Keep the old leader serving the transaction after step-down: rejected because followers must not keep accepting writes once leadership is lost.

## Decision 34: Active explicit transactions require leader-affine reads and writes

- Decision: While an explicit transaction is active in cluster mode, all transactional reads, writes, and `COMMIT`/`ROLLBACK` operations must execute on, or be forwarded to, the transaction's bound leader. Follower-local reads are out of scope for active transactions.
- Rationale: Read-your-writes depends on the leader-local overlay. Followers may lag behind leader apply, and they never own the uncommitted overlay state. CockroachDB and YugabyteDB both colocate strong reads with the leaseholder/leader by default and treat follower reads as a separate stale/safe-timestamp feature. KalamDB should follow the same rule.
- Alternatives considered:
  - Serve active-transaction reads from followers after a local apply wait: rejected because followers still lack the uncommitted overlay and may not even know the transaction exists.
  - Broadcast the overlay to followers for each read: rejected because it is effectively a distributed transaction cache.

## Decision 35: Keep commit acknowledgment at quorum replication plus leader apply, not "all followers applied"

- Decision: Successful explicit-transaction `COMMIT` in Raft mode continues to mean normal Raft success: the `TransactionCommit` entry is replicated to a quorum and applied on the leader. It does not wait for every follower to apply before replying.
- Rationale: That is the existing OpenRaft `client_write()` contract in KalamDB today. Waiting for all followers to apply would turn normal Raft writes into a much slower and less available protocol. PostgreSQL exposes this distinction explicitly with `synchronous_commit` levels such as `on`, `remote_write`, and `remote_apply`; CockroachDB uses leader/leaseholder reads for strong consistency and safe-time follower reads separately; etcd also treats the committed revision as the durable boundary while watch visibility can lag. KalamDB should preserve quorum semantics and treat stronger follower-visible guarantees as a separate feature.
- Alternatives considered:
  - Wait for all followers to apply on every transaction commit: rejected because one slow follower would stall every write transaction.
  - Claim follower-visible read-after-write from any node immediately after commit: rejected because it is false under current Raft apply semantics.

## Decision 36: Put the transaction query seam in a shared crate, not only in `kalamdb-core`

- Decision: The query-facing transaction surface (`TransactionOverlay`, `TransactionOverlayExec`, and the transaction-specific DataFusion session extension/traits used by table providers) must live in a crate consumable by both `kalamdb-core` and `kalamdb-tables`, rather than only inside `kalamdb-core`.
- Rationale: The current crate graph already has `kalamdb-core -> kalamdb-tables`, while `kalamdb-tables` extracts only session extensions (`SessionUserContext`) and scan helpers from shared crates. Putting `TransactionOverlayExec` only in `kalamdb-core` but asking `kalamdb-tables` providers to wrap scans with it creates the wrong dependency direction. Copying the full overlay into `SessionUserContext` would also bloat every query context. A shared crate/seam keeps the layering clean and the per-query context lightweight.
- Alternatives considered:
  - Let `kalamdb-tables` depend on `kalamdb-core`: rejected because it creates a cycle and breaks crate ownership boundaries.
  - Clone the full overlay into `SessionUserContext`: rejected because it increases memory cost per query and mixes transaction state into a general identity context.

## Decision 37: Persist `commit_seq` as hidden row visibility metadata

- Decision: Every committed user/shared row version carries a hidden commit-order marker (for example `_commit_seq`) in hot storage, Parquet output, and row serialization.
- Rationale: The current row shape and system-column list carry `_seq` and `_deleted`, but no persisted commit-order marker. Snapshot isolation cannot be enforced cheaply unless visibility metadata travels with each row version. A hidden per-row marker is the lightest correct option because it avoids a second lookup table or storage join on every snapshot read.
- Alternatives considered:
  - Keep `commit_seq` only in memory: rejected because snapshot reads on persisted rows and restart recovery would have no durable visibility metadata.
  - Store `commit_seq` in a separate side table keyed by row version: rejected because it adds extra lookups and breaks the current hot/cold row-resolution pipeline.

## Decision 38: Resolve the latest visible version by `commit_seq` first, then `_seq`

- Decision: Snapshot reads must choose the latest committed version where `commit_seq <= snapshot_commit_seq`, then use `_seq` only as an intra-commit tie-break. They must not resolve `MAX(_seq)` first and then discard rows newer than the snapshot boundary.
- Rationale: The current `kalamdb-tables` version-resolution helper keeps the row with the highest `_seq` per logical row. If snapshot filtering is bolted on afterward, a row whose newest committed version is post-snapshot would disappear entirely even when an older committed version is still visible. Visibility must therefore be applied before, or as part of, version resolution.
- Alternatives considered:
  - Post-filter rows after `MAX(_seq)` resolution: rejected because it can produce false "row missing" results for snapshot reads.
  - Ignore older visible versions and require read committed only: rejected because the spec requires snapshot-style isolation.

## Decision 39: Allocate `commit_seq` in the durable apply path used by both autocommit and explicit commits

- Decision: The same durable apply-path source allocates/stamps `commit_seq` for autocommit writes and explicit-transaction commits. The coordinator reads the latest committed value at `BEGIN`, but it does not own a private authoritative counter for all writes.
- Rationale: The current plan said the coordinator could increment `commit_seq`, but autocommit writes already bypass the coordinator and the row writer needs the committed value while writing the row version. The correct place for the authoritative sequence is the write/apply path that already owns durable ordering for both autocommit and `TransactionCommit` replay.
- Alternatives considered:
  - Keep a coordinator-local `AtomicU64` as the only source: rejected because autocommit writes would diverge and row stamping would happen too late.
  - Derive snapshot order only from `_seq`: rejected for the reasons in Decision 1 and Decision 38.

## Decision 40: Never hold shared coordinator guards across awaits, and avoid one timer task per transaction

- Decision: The coordinator may use `DashMap`/per-transaction state transitions, but it must drop shared guards before awaiting Raft apply, file cleanup, storage I/O, or fanout work. Timeout enforcement should use a bounded periodic sweep (or similar shared mechanism), not one spawned timer per active transaction.
- Rationale: The current codebase relies on bounded worker loops, sharded queues, and `spawn_blocking`/shared background tasks to stay scalable. Holding guards across awaits would serialize unrelated transactions, and per-transaction timers would create avoidable scheduling/memory overhead at scale. The design must preserve non-blocking behavior except at the explicit durability boundary.
- Alternatives considered:
  - Hold the active transaction entry locked through commit: rejected because one slow commit would stall unrelated lifecycle operations.
  - Spawn a timeout task per transaction: rejected because thousands of mostly-idle read transactions would pay unnecessary scheduler and memory overhead.

## Decision 41: All DML entry points must honor staging when a transaction is active

- Decision: Every DML entry point that can write rows — typed pg operations, DataFusion DML, and SQL fast paths such as `fast_insert` / `fast_point_dml` — must route through transaction staging when an explicit transaction is active.
- Rationale: The current codebase has more than one DML entry path. In particular, SQL fast-path helpers call the applier directly today. If those paths are not transaction-aware, explicit SQL transactions will silently bypass staging and break rollback/read-your-writes semantics.
- Alternatives considered:
  - Disable fast paths whenever a transaction is active: acceptable as an emergency fallback, but rejected as the target design because it adds avoidable performance cliffs and duplicated branching.
  - Leave the fast paths untouched: rejected because it would create correctness holes.

## Cluster and Replication Comparison

- PostgreSQL HA: one primary accepts writes, standbys replay WAL, and commit visibility on standbys depends on replication and `synchronous_commit` settings (`remote_write`, `on`, `remote_apply`). Uncommitted transactions do not survive failover.
- CockroachDB and YugabyteDB: strong reads and writes are leader/leaseholder-affine by default. Cross-range distributed transactions require extra replicated transaction state, not just a final Raft batch.
- etcd: transactions are single consensus-backed API requests that commit at one revision. etcd is a good model for atomic one-shot transactions, but not for multi-request PostgreSQL-style `BEGIN` / `COMMIT` sessions.

## Live Query Fanout Review: Supabase Realtime

### Relevant Supabase patterns observed

- `Extensions.PostgresCdcRls.ReplicationPoller` narrows recipients before websocket fanout by carrying a `subscription_ids` set with each change, then routing that set to the correct node before dispatch.
- `Extensions.PostgresCdcRls.MessageDispatcher` caches encoded messages by payload/message shape so the same broadcast is serialized once and reused across many subscribers.
- `RealtimeWeb.RealtimeChannel.MessageDispatcher` uses Phoenix fastlane metadata to send directly to transport processes while skipping already-replayed messages and incrementing rate counters.
- `SubscriptionManager` aggressively cleans up dead subscribers, batches deletion of orphaned subscriptions, and stops idle tenant workers after inactivity.
- The system relies on hard rate limits (`max_channels_per_client`, `max_events_per_second`, `max_joins_per_second`) and bounded runtime settings (`websocket_max_heap_size`, pool sizes) as part of normal production posture.

### What KalamDB should adopt

- Keep the candidate subscriber set narrow before fanout. KalamDB already has the first-stage routing indices in `ConnectionsManager`; commit fanout should continue to use those table-scoped indices and never fall back to scanning all connections.
- Keep payload reuse explicit. KalamDB already caches shared payloads by projection group in `notification.rs`; the transaction design should preserve that and extend it to serialization group boundaries where necessary.
- Keep cleanup first-class. Disconnect cleanup, idle cleanup, and slow-subscriber behavior must remain bounded and tested, not best-effort.

### What KalamDB should not copy directly

- Do not persist ephemeral live subscriptions in a database table just to emulate Supabase's `realtime.subscription`; KalamDB's live subscriptions are intentionally in-memory and node-local.
- Do not add cluster broadcast for live fanout unless the apply path itself stops being node-local. Supabase needs cluster routing because CDC enters through a separate replication/poller path; KalamDB already notifies on local apply.
- Do not move filter/projection evaluation into SQL rewrite or storage persistence purely for targeting. KalamDB's current compiled in-memory filter path is consistent with the rest of the architecture and cheaper to evolve incrementally.

### Current KalamDB strengths confirmed by code review

- `ConnectionsManager` already maintains table-scoped secondary indices for user and shared table subscriptions.
- `NotificationService` already shards work by deterministic table hash, preserving per-table ordering while allowing cross-table parallelism.
- `notification.rs` already reuses projected payloads across subscribers with identical projections.
- Connection and subscription flow control is already bounded through notification channel limits and initial-load buffers.

### Remaining hardening gap

- The commit path still needs to make the post-commit fanout contract explicit: build a dispatch plan after durable commit, release it outside coordinator locks, and add stress coverage for a hot table with many subscribers and slow consumers.
