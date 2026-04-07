# Implementation Plan: PostgreSQL-Style Transactions for KalamDB

**Branch**: `027-pg-transactions` | **Date**: 2025-01-20 | **Last Updated**: 2026-04-07 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/027-pg-transactions/spec.md`

## Summary

Add explicit `BEGIN`/`COMMIT`/`ROLLBACK` transaction support to KalamDB, enabling pg_kalam (PostgreSQL FDW) and the SQL REST endpoint to stage multi-statement writes with read-your-writes visibility and atomic commit. **Sessions remain central for pg-origin transactions**, while `/v1/api/sql` uses a lightweight request-scoped owner identifier and the same coordinator. Cross-origin observability is provided by a new `system.transactions` virtual view, while `system.sessions` remains focused on live pg transport sessions. Transactions use snapshot isolation based on a global `commit_seq` counter that is stamped into committed row versions as a hidden visibility marker (for example `_commit_seq`) by the same durable apply path used for autocommit writes. A shared transaction/query seam lives outside `kalamdb-core` so `kalamdb-tables` can consume overlay context without a crate cycle. The `TransactionCoordinator` in `kalamdb-core` stages mutations in memory and commits atomically through a single `TransactionCommit` Raft command, preserving the existing write path while requiring coordinator state transitions to drop shared guards before awaited work. Non-transactional autocommit path remains unchanged with near-zero overhead. The production-hardening delta adds typed internal owner keys, a hot/cold state split, ordered cleanup phases, an explicit post-commit live-fanout plan, and Raft-aligned cluster semantics: open transactions stay leader-local, bind to one data group, and abort on leader change rather than pretending distributed transaction recovery already exists.

## Session Foundation (Completed ✅)

The session infrastructure is **already in place** and forms the anchor for all transaction work. The following capabilities are operational:

### What Is Built

| Component | Location | Status |
|-----------|----------|--------|
| **Config-scoped session IDs** | `pg/src/remote_state.rs` | ✅ Format: `pg-<pid>-<config_hash_hex>` — unique per PG backend × foreign server config |
| **Per-session transaction tracking** | `pg/src/fdw_xact.rs` | ✅ `HashMap<String, ActiveTransaction>` keyed by session_id |
| **Remote state registry** | `pg/src/remote_state.rs` | ✅ `RemoteStateRegistry` keyed by `RemoteServerConfig`, with session-id index |
| **Session metadata registry** | `backend/crates/kalamdb-pg/src/session_registry.rs` | ✅ `SessionRegistry` with `DashMap<session_id, RemotePgSession>` |
| **Transaction lifecycle RPCs** | `backend/crates/kalamdb-pg/src/service.rs` | ✅ `begin_transaction`, `commit_transaction`, `rollback_transaction` handlers |
| **Transaction state in sessions** | `session_registry.rs` | ✅ `transaction_id`, `transaction_state`, `transaction_has_writes` fields |
| **Write tracking** | `service.rs` | ✅ `mark_transaction_writes()` called on insert/update/delete |
| **PG xact callbacks** | `pg/src/fdw_xact.rs` | ✅ PRE_COMMIT flush, COMMIT → remote commit, ABORT → remote rollback |
| **Lazy transaction start** | `pg/src/fdw_xact.rs` | ✅ `ensure_transaction()` on first FDW scan/modify operation |
| **Session exit cleanup** | `pg/src/remote_state.rs` | ✅ `on_proc_exit_close_sessions` iterates all states, closes sessions |
| **system.sessions view** | `backend/crates/kalamdb-views/src/sessions.rs` | ✅ Virtual view exposing session + transaction metadata |
| **gRPC connection reuse** | `pg/src/remote_state.rs` | ✅ Single `Channel` per config, cloned per RPC |

### Cross-Origin Transaction Observability (Planned)

`system.sessions` is intentionally pg-session-focused. To cover active transactions opened through `/v1/api/sql` as well, the design adds a separate `system.transactions` virtual view with these constraints:

- Source only from in-memory `TransactionCoordinator` active handles.
- Include active transactions from `PgRpc`, `SqlBatch`, and `Internal` origins.
- For `SqlBatch`, use a lightweight request owner ID such as `sql-req-<request_id>` instead of creating a `SessionRegistry` row.
- Avoid joins, background collectors, and persisted observability state.
- Reuse the existing virtual-view pattern (`system.sessions`, `system.live`, `system.stats`): memoized schema plus a query-time callback snapshot.

### Session-Transaction Ownership Model

```
PostgreSQL Backend Process (PID)
    │
    ├── Foreign Server Config A ──→ RemoteExtensionState
    │       │                          ├── session_id: "pg-<pid>-<hash_A>"
    │       │                          ├── gRPC Channel (reused)
    │       │                          └── tokio Runtime
    │       │
    │       └── fdw_xact::CURRENT_TX["pg-<pid>-<hash_A>"]
    │               └── ActiveTransaction { session_id, transaction_id }
    │
    ├── Foreign Server Config B ──→ RemoteExtensionState
    │       │                          ├── session_id: "pg-<pid>-<hash_B>"
    │       │                          ├── gRPC Channel (reused)
    │       │                          └── tokio Runtime
    │       │
    │       └── fdw_xact::CURRENT_TX["pg-<pid>-<hash_B>"]
    │               └── ActiveTransaction { session_id, transaction_id }
    │
    └── On process exit: close ALL sessions → server rollbacks any active tx
```

**On the KalamDB server side:**

```
SessionRegistry (DashMap<session_id, RemotePgSession>)
    │
    ├── "pg-1234-a1b2c3d4..." → RemotePgSession
    │       ├── transaction_id: Some("tx-pg-1234-a1b2c3d4...-0")
    │       ├── transaction_state: Some(Active)
    │       ├── transaction_has_writes: true
    │       ├── client_addr, last_method, timestamps...
    │       └── current_schema: Some("public")
    │
    ├── "pg-1234-e5f6g7h8..." → RemotePgSession (different config, same PG backend)
    │       └── transaction_id: None (no active tx)
    │
    └── "pg-5678-a1b2c3d4..." → RemotePgSession (different PG backend, same config)
            └── transaction_id: Some("tx-pg-5678-a1b2c3d4...-0")
```

### Known Gap: `close_session` Does Not Rollback

**⚠️ BUG**: The current `close_session` RPC handler in `service.rs` calls `self.session_registry.remove(session_id)` and returns — it does **not** rollback any active transaction first. This means if a session closes with an active transaction, the staged writes (once implemented) would be orphaned. **This must be fixed in Phase 6 (US5) as T046/T047.**

## Technical Context

**Language/Version**: Rust 1.90+ (edition 2021)
**Primary Dependencies**: DataFusion 40.0, Apache Arrow 52.0, RocksDB 0.24, Actix-Web 4.4, DashMap 5, serde 1.0, tokio 1.48
**Storage**: RocksDB for write path (<1ms), Parquet for flushed segments. Transaction staged writes are in-memory only until commit.
**Testing**: `cargo nextest run` for unit/integration tests; `cargo test --test smoke` for CLI smoke tests
**Target Platform**: Linux/macOS server (aarch64, x86_64)
**Project Type**: Database engine (multi-crate Rust workspace)
**Performance Goals**: Autocommit hot path must not regress >5%. Transaction commit latency target: <10ms for typical write sets (<1000 mutations). TransactionCoordinator presence check on non-transactional path: <100ns (single DashMap lookup). Empty/read-only transactions should avoid write-set allocation entirely. Hot-table fanout after commit must route from pre-indexed candidates only and remain memory-bounded under slow subscribers.
**Constraints**: Transaction memory budget configurable (default 100MB). Transaction timeout configurable (default 300s). Phase 1 GA remains single-node; any later cluster-mode enablement before distributed-transaction work is limited to one data Raft group, leader-local open state, and abort-on-failover semantics. Live subscriptions remain in-memory and node-local; this feature does not add a persisted subscription catalog.
**Scale/Scope**: Concurrent transactions bounded by memory budget. Affects `kalamdb-commons`, `kalamdb-configs`, `kalamdb-core`, `kalamdb-transactions`, `kalamdb-tables`, `kalamdb-pg`, `kalamdb-sql`, `kalamdb-api`, and `kalamdb-views`. ~86 tasks across 10 phases including completed session foundation and Raft-alignment follow-up.

## Architecture Decisions Summary

Key decisions from [research.md](research.md):

1. **Snapshot isolation via `commit_seq`** (Decision 1): Global monotonic commit-order source, NOT per-row `_seq` Snowflake ID. Transactions read `commit_seq <= snapshot_commit_seq`.
2. **Coordinator in `kalamdb-core`** (Decision 2): `TransactionCoordinator` with `Arc<AppContext>` dependency (Decision 7A), not `StorageBackend`.
3. **Commit through Raft** (Decision 7/7B): Single `TransactionCommit` Raft command for atomic apply through `DmlExecutor`. Never bypass to raw `StorageBackend::batch()`.
4. **Future-proof Raft commands** (Decision 7C): `Option<TransactionId>` in Raft data commands now to avoid protocol migration later.
5. **TransactionOverlayExec** (Decision 15): Custom DataFusion `ExecutionPlan` for overlay scan merging.
6. **DDL rejected in transactions** (Decision 16): Schema operations don't support rollback semantics.
7. **Type-safe TransactionId** (Decision 13): Newtype following established ID pattern in `kalamdb-commons/src/models/ids/`.
8. **Unified TransactionState** (Decision 14): Single enum in `kalamdb-commons` replacing pg crate's version.
9. **fdw_xact at XACT_EVENT_COMMIT** (Decision 8): Aligned with existing `fdw_xact.rs` code, not PRE_COMMIT.
10. **Config-scoped sessions as transaction anchor** (Decision 18): Each PG backend × foreign server config pair gets a unique session (`pg-<pid>-<config_hash>`). Pg-origin transactions are scoped to that session, and multi-config backends maintain independent transaction lifecycles.
11. **Per-session transaction tracking in fdw_xact** (Decision 19): `fdw_xact::CURRENT_TX` is a `HashMap<session_id, ActiveTransaction>`, not a single `Option`. PG xact callbacks iterate all active sessions at commit/abort.
12. **Session close must rollback active transactions** (Decision 20): `close_session` on the server MUST call `TransactionCoordinator::rollback()` before removing session state, to prevent orphaned staged writes.
13. **Lightweight `system.transactions` view** (Decision 21): active transactions are exposed through a dedicated virtual view sourced directly from in-memory transaction handles, not via persisted state or system.stats.
14. **SQL batch uses request owner IDs, not pg session rows** (Decision 22): `/v1/api/sql` explicit transactions reuse the coordinator path with a lightweight owner ID and appear in `system.transactions`, but do not inflate `system.sessions`.
15. **Typed internal owner keys** (Decision 23): the coordinator uses a compact `ExecutionOwnerKey` internally and derives human-readable owner labels only for views, logs, and wire contracts.
16. **Hot/cold transaction split** (Decision 24): hot metadata and cold staged-write state are stored separately, and the write set is allocated lazily on first write.
17. **Explicit lifecycle state machine** (Decision 25): read-only, write-active, committing, rolling back, timed-out, and terminal phases are distinct states.
18. **Ordered cleanup phases** (Decision 26): commit and rollback are multi-phase transitions so disconnect/timeout races stay idempotent.
19. **Post-commit side-effect plan** (Decision 27): live query notifications, publisher events, and manifest/file work are released only after durable commit succeeds and outside coordinator locks.
20. **Candidate-scoped, node-local live fanout** (Decision 28): commit fanout keeps using in-memory table-scoped indices instead of a persisted subscription catalog.
21. **Projection/serialization reuse plus bounded slow-consumer buffers** (Decision 29): high-fanout delivery reuses payload work across subscriber groups and preserves bounded queues.
22. **Race and hot-table hardening tests are mandatory** (Decision 30): lifecycle races and high-cardinality fanout must be stress-tested before the design is treated as production-ready.
23. **Cluster-mode open transactions are leader-local until commit** (Decision 31): staged writes and overlays are not replicated before `COMMIT`.
24. **Cluster-mode explicit transactions are single-group only** (Decision 32): any access that crosses to a second Raft data group is rejected in this phase.
25. **Leader change aborts open transactions** (Decision 33): failover requires the client to retry from a new transaction.
26. **Active transactions are leader-affine** (Decision 34): transactional reads and writes must execute on the bound leader, not on followers.
27. **Commit acknowledgment remains quorum + leader apply** (Decision 35): do not wait for all followers to apply before replying.
28. **Shared transaction/query seam outside `kalamdb-core`** (Decision 36): overlay execution and transaction-specific DataFusion session context must live in a crate consumable by both `kalamdb-core` and `kalamdb-tables` without a dependency cycle.
29. **Persist hidden `_commit_seq` visibility metadata** (Decision 37): committed user/shared row versions carry a hidden commit-order marker in hot storage and Parquet representations.
30. **Resolve latest visible version by `commit_seq` before `_seq`** (Decision 38): snapshot reads choose the newest committed version at or below the snapshot boundary, then use `_seq` only as a tie-break.
31. **Allocate `commit_seq` in the durable apply path** (Decision 39): autocommit writes and explicit-transaction commits share one durable commit-sequence source instead of coordinator-local counters.
32. **Never hold shared coordinator guards across awaits** (Decision 40): Raft waits, storage I/O, and fanout happen after state is sealed and guards are dropped; timeout protection uses bounded sweeps instead of one timer per transaction.
33. **All DML entry points must honor staging** (Decision 41): typed PG DML, DataFusion DML, and SQL fast-path insert/update/delete helpers must all route through the coordinator when an explicit transaction is active.

## Production Hardening Checklist

- `backend/crates/kalamdb-core/src/transactions/`: introduce a typed `ExecutionOwnerKey`, split hot metadata from cold write-set storage, and make `BEGIN` lazy for read-only and empty transactions.
- `backend/crates/kalamdb-transactions/src/`: hold transaction-specific DataFusion/session extensions, overlay types, and `TransactionOverlayExec` so `kalamdb-core` and `kalamdb-tables` share one transaction-query seam without a `kalamdb-tables -> kalamdb-core` dependency.
- `backend/crates/kalamdb-core/src/transactions/coordinator.rs`: seal transactions through explicit lifecycle states and ordered cleanup phases for commit, rollback, timeout, and owner-loss cleanup, and drop all shared guards before awaited work.
- `backend/crates/kalamdb-commons/src/{constants.rs,models/rows/**,serialization/row_codec.rs}` plus `backend/crates/kalamdb-tables/src/utils/version_resolution.rs`: add the hidden `_commit_seq` visibility marker and resolve the newest visible committed version before `_seq` tie-breaks.
- `backend/crates/kalamdb-pg/src/session_registry.rs` and `backend/crates/kalamdb-pg/src/service.rs`: map pg-visible transaction state onto the expanded lifecycle and make rollback-before-remove idempotent when `close_session` races with commit or timeout cleanup.
- `backend/crates/kalamdb-core/src/operations/service.rs`, `backend/crates/kalamdb-core/src/sql/executor/sql_executor.rs`, `backend/crates/kalamdb-core/src/sql/executor/fast_insert.rs`, and `backend/crates/kalamdb-core/src/sql/executor/fast_point_dml.rs`: resolve active transactions through a fast owner-key lookup, allocate cold write-set state only when staging the first mutation, and ensure no SQL DML entry path bypasses staging.
- `backend/crates/kalamdb-core/src/live/notification.rs`: accept post-commit dispatch plans, route from existing table-scoped candidate sets, reuse projection and serialization work, and never hold coordinator locks while fanout runs.
- `backend/crates/kalamdb-core/src/transactions/write_set.rs` plus `commit_result.rs`: keep staged user/shared insert/update/delete changes in one write-set source of truth while the transaction is open, then derive one compact `CommitSideEffectPlan` at commit time instead of maintaining duplicate live/publisher queues.
- `backend/crates/kalamdb-core/src/live/manager/connections_manager.rs` and `backend/crates/kalamdb-core/src/live/models/connection.rs`: preserve bounded connection/subscription buffers and validate slow-subscriber behavior under commit bursts.
- `backend/crates/kalamdb-core/tests/` and `backend/crates/kalamdb-pg/tests/`: add repeated race/fault-injection coverage for commit vs rollback/session-close/timeout plus hot-table live-fanout stress cases.
- `backend/crates/kalamdb-core/src/transactions/` plus routing surfaces in `backend/crates/kalamdb-api/src/http/sql/forward.rs` and `backend/crates/kalamdb-pg/src/service.rs`: bind cluster-mode transactions to one data-group leader, reject cross-group access, and abort on leader change.

## Critical Write Path (Session-Anchored)

```
pg_kalam FDW operation (scan/modify)
    │
    ▼
ensure_remote_extension_state(config) → Arc<RemoteExtensionState>
    │                                     └── session_id: "pg-<pid>-<config_hash>"
    ▼
ensure_transaction(session_id)        ← lazily begins tx on first FDW op per PG xact
    │                                    looks up CURRENT_TX[session_id] first
    │                                    if absent: calls server BeginTransaction RPC
    ▼
Server: SessionRegistry.begin_transaction(session_id)
    │     └── creates tx_id: "tx-{session_id}-{counter}"
    │     └── sets RemotePgSession.transaction_state = Active
    │
    ▼ (DML operations via gRPC)
Server: OperationService checks session for active tx
    │     └── if tx active → TransactionCoordinator::stage(StagedMutation)
    │     └── if no tx → autocommit (existing immediate apply path)
    │
    ▼ (PG COMMIT fires xact_callback)
pg_kalam: write_buffer::flush_all()   ← at PRE_COMMIT
pg_kalam: client.commit_transaction() ← at XACT_EVENT_COMMIT (per session)
    │
    ▼
Server: TransactionCoordinator::commit()
    │
    ▼
UnifiedApplier::apply(TransactionCommit { tx_id, mutations })
    │
    ▼
RaftApplier → Raft consensus → state machine
    │
    ▼
CommandExecutorImpl::dml() → DmlExecutor
    │
    ▼
Table providers: _seq generation, index updates,
                 `_commit_seq` stamping,
                 deferred side-effect plan construction
                 from committed user/shared staged changes,
                 manifest updates, file refs
    │
    ▼
TransactionCoordinator observes committed commit_seq,
detaches owner, then releases fanout/publisher work
    │
    ▼
SessionRegistry: clear transaction state on session
```

**Session is the anchor**: The session_id is the key that connects the PG backend process, the gRPC channel, the remote state registry, the fdw_xact transaction tracking, and the server-side SessionRegistry + TransactionCoordinator. A transaction cannot exist without a session.

**CRITICAL**: Direct `StorageBackend::batch()` is NEVER used for transaction commit. It would bypass `_seq`, indexes, notifications, Raft replication, and publisher events.

## Constitution Check

*GATE: Passed*

- Model separation: Each transaction entity in its own file ✓
- AppContext-First: TransactionCoordinator depends on `Arc<AppContext>` ✓
- Type-safe IDs: `TransactionId` newtype in commons ✓
- Performance: Near-zero-cost for non-transactional path ✓
- Filesystem vs RocksDB separation: Transaction logic in `kalamdb-core`, not `kalamdb-store` ✓
- Crate boundaries: shared transaction/query types stay outside the `kalamdb-core` → `kalamdb-tables` dependency direction ✓
- No SQL rewrite in hot paths: Overlay via DataFusion ExecutionPlan, not SQL rewrite ✓
- Session-anchored: Transactions never exist without a session; session_id is the correlation key across pg extension, gRPC, and server-side coordinator ✓
- Config isolation: Different foreign server configs get independent sessions and transactions ✓

## Project Structure

### Documentation (this feature)

```text
specs/027-pg-transactions/
├── plan.md              # This file
├── spec.md              # Feature specification (6 user stories, 46 FRs)
├── research.md          # 41 research decisions (original + session/observability + hardening/fanout + Raft alignment + storage/cycle/non-blocking corrections)
├── data-model.md        # external entities + internal coordinator/fanout structures + state transitions
├── quickstart.md        # 11 implementation scenarios
├── contracts/
│   ├── pg-transaction-rpc.md     # pg gRPC contract (session-scoped)
│   └── sql-transaction-batch.md  # SQL REST contract (request-scoped)
├── tasks.md             # ~86 tasks across 10 phases (Phase 0 completed)
└── checklists/
    └── requirements.md  # Requirements checklist
```

### Source Code — Session Foundation (COMPLETED ✅)

```text
pg/src/
├── remote_state.rs                  # RemoteStateRegistry: config-keyed, session-id indexed
├── fdw_xact.rs                      # Per-session tx tracking, PG xact callbacks
├── fdw_scan.rs                      # ensure_transaction(session_id) on scan
├── fdw_modify.rs                    # ensure_transaction(session_id) on modify
└── fdw_ddl.rs                       # Config-aware state resolution

pg/crates/kalam-pg-common/src/
└── config.rs                        # RemoteServerConfig with Hash derive

backend/crates/kalamdb-pg/src/
├── session_registry.rs              # SessionRegistry + RemotePgSession + TransactionState
└── service.rs                       # RPC handlers (begin/commit/rollback)

backend/crates/kalamdb-views/src/
└── sessions.rs                      # system.sessions virtual view
```

### Source Code — Transaction Engine (TO BUILD)

```text
backend/crates/
├── kalamdb-commons/src/models/
│   ├── ids/transaction_id.rs        # TransactionId newtype (NEW)
│   └── transaction.rs               # TransactionState, TransactionOrigin, OperationKind (NEW)
├── kalamdb-commons/src/{constants.rs,models/rows/**,serialization/row_codec.rs}  # hidden `_commit_seq` system-column support
├── kalamdb-commons/src/system_tables.rs  # Add system.transactions virtual view enum entry
├── kalamdb-configs/src/lib.rs       # transaction_timeout_secs, max_buffer_bytes fields
├── kalamdb-transactions/src/         # NEW CRATE
│   ├── lib.rs
│   ├── overlay.rs                   # TransactionOverlay shared with query providers
│   ├── overlay_exec.rs              # TransactionOverlayExec (DataFusion ExecutionPlan)
│   ├── query_context.rs             # transaction-specific DataFusion extension / traits
│   └── commit_sequence.rs           # shared commit-sequence traits/types
├── kalamdb-core/src/
│   ├── transactions/                # NEW MODULE
│   │   ├── mod.rs
│   │   ├── coordinator.rs           # TransactionCoordinator (active_by_id, active_by_owner, write_sets)
│   │   ├── owner.rs                 # ExecutionOwnerKey / owner-label helpers
│   │   ├── handle.rs                # TransactionHandle / active hot metadata
│   │   ├── write_set.rs             # TransactionWriteSet (cold staged payload)
│   │   ├── staged_mutation.rs       # StagedMutation
│   │   ├── commit_sequence.rs       # durable apply-path commit-sequence tracker
│   │   ├── commit_result.rs         # TransactionCommitResult, TransactionSideEffects, CommitSideEffectPlan
│   │   └── metrics.rs               # ActiveTransactionMetric
│   ├── app_context.rs               # Wire TransactionCoordinator
│   ├── applier/                     # Add TransactionCommit Raft command variant
│   ├── operations/service.rs        # Route DML through coordinator when tx active
│   ├── sql/executor/                # BEGIN/COMMIT/ROLLBACK handlers
│   └── views/system_schema_provider.rs  # Wire system.transactions callback/provider
├── kalamdb-tables/src/
│   ├── shared_tables/shared_table_provider.rs  # Overlay + commit-seq visibility filter
│   └── user_tables/user_table_provider.rs      # Overlay + commit-seq visibility filter
├── kalamdb-pg/src/
│   ├── service.rs                   # Wire RPC to coordinator
│   └── session_registry.rs          # Replace local TransactionState with commons
├── kalamdb-views/src/
│   └── transactions.rs              # system.transactions virtual view
├── kalamdb-sql/src/
│   └── classifier/types.rs          # Already has transaction variants ✓
└── kalamdb-api/src/http/sql/execute.rs  # Request-end cleanup

pg/src/
└── fdw_xact.rs                      # Align Abort callback, keep XACT_EVENT_COMMIT

Tests:
├── backend/crates/kalamdb-pg/tests/             # pg transaction integration tests
├── backend/crates/kalamdb-core/tests/            # Core transaction tests
└── cli/tests/                                    # Smoke tests
```

## Live Query Fanout Alignment

- KalamDB already has the correct first-stage routing structure for commit fanout: `ConnectionsManager` indexes subscribers by `(user_id, table_id)` and `table_id`, and `NotificationService` preserves per-table ordering through deterministic worker sharding.
- The Supabase Realtime review adds one concrete design requirement: transaction commit must hand a post-commit dispatch plan to the existing notification workers instead of doing row-by-row fanout while transaction cleanup is still in progress.
- KalamDB should adopt Supabase's "narrow recipients before transport fanout" principle, but it should not copy Supabase's persisted `realtime.subscription` catalog or cluster PubSub broadcast path because KalamDB's apply-driven live notifications are already node-local.
- Payload reuse remains explicit: keep sharing `SharedChangePayload` across identical projection groups and extend the cache boundary to wire-format grouping where protocol negotiation makes that necessary.

## Raft / Cluster Alignment

- Current KalamDB semantics already match standard Raft write behavior: proposals go to the current group leader, the client sees success after quorum replication plus leader apply, and followers may still be catching up locally.
- Explicit transactions should not try to outsmart that model. In cluster mode, the open transaction, overlay, and staged writes remain leader-local and bind to one data Raft group on first table access.
- Cross-group explicit transactions are intentionally rejected until KalamDB has a real distributed transaction protocol. A transaction that touches a second data group after binding should fail fast rather than risk a fake atomicity story.
- Leader change while a transaction is open aborts the transaction. This matches PostgreSQL failover semantics for uncommitted work and avoids inventing ad hoc migration of private in-memory coordinator state.
- PostgreSQL HA is the closest behavioral analog for the open-transaction part: primary-owned transaction state, replicated commit durability knobs, and no survival of uncommitted work after failover. CockroachDB and YugabyteDB are the better analogs for the future cross-shard problem because they rely on extra replicated transaction metadata, not only a final Raft batch.

## Complexity Tracking

No constitution violations requiring justification. All decisions follow established KalamDB patterns (AppContext dependency injection, type-safe IDs, Raft consensus for writes, leader-forwarded cluster writes, DataFusion execution plans for query processing).

## Implementation Strategy

### Session Foundation First (DONE ✅)

The session infrastructure is the prerequisite for all transaction work and is **already completed**:
- Config-scoped session IDs with unique per-backend-per-config identifiers
- Per-session transaction tracking in fdw_xact (HashMap, not single Option)
- Session metadata registry on the server with transaction fields
- gRPC connection reuse and session lifecycle management
- system.sessions virtual view for pg-session observability
- The next transaction phases add `system.transactions` for active transactions across pg and SQL batch origins without creating extra background pressure

### MVP (Phase 1-3): Build on Session Foundation

1. Complete Phase 1: Setup — type definitions and configuration
2. Complete Phase 2: Foundational — transaction coordinator engine (references sessions via session_id)
3. Complete Phase 3: US1+US2 — pg_kalam atomic transactions with read-your-writes
4. **STOP and VALIDATE**: Run quickstart scenarios 1–3, verify pg_kalam e2e tests pass
5. Deploy/demo — this is the primary motivation for the feature

### Incremental Delivery

0. **Session Foundation** → ✅ Complete (sessions, session IDs, tx lifecycle RPCs, xact callbacks)
1. **Setup + Foundational** → Transaction infrastructure ready (TransactionCoordinator wired to sessions)
2. **US1+US2 (Phase 3)** → pg_kalam transactions work → **MVP Deploy** (SC-001 through SC-003)
3. **US3 (Phase 4)** → Snapshot isolation → Production multi-user safety (SC-004)
4. **US4 (Phase 5)** → SQL REST transactions → Full SQL surface coverage plus request-scoped owner IDs for observability (SC-009, SC-012)
5. **US5 (Phase 6)** → Disconnect cleanup → Production reliability (SC-005) — **fixes close_session gap**
6. **US6 (Phase 7)** → Timeout protection → Defensive stability (SC-006)
7. **Polish (Phase 8)** → `system.transactions`, perf regression, edge cases → Production readiness
8. **Raft Alignment (Phase 9)** → single-group cluster semantics, leader affinity, failover aborts → required before cluster-mode enablement
