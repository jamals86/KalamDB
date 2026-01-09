# Spec 019: Unified Command Applier - Implementation Tasks

## Status: � Ready to Start
## Created: 2026-01-09
## Updated: 2026-01-09

---

## Critical Rules

> **MUST follow these rules during implementation:**

1. **Single Mutation Point**: All mutations happen in `RaftStateMachine::apply()` → `CommandExecutorImpl`
2. **No Code Duplication**: Logic exists in exactly ONE place
3. **No Mode Branching**: Zero `is_cluster_mode()` in handlers
4. **Delete Old Code**: Remove deprecated code immediately, no backward compat
5. **Small Files**: Each file < 300 lines, methods < 50 lines
6. **Trust OpenRaft Completely**: 
   - Use `raft.client_write()` for proposals
   - Use `raft.metrics()` for leader detection
   - NEVER implement custom replication logic
   - NEVER wait for "all followers" — quorum is enough
   - NEVER track replication progress — OpenRaft does this

---

## Overview

This document tracks the implementation of the Unified Command Applier architecture, which consolidates all command execution into a single code path regardless of standalone or cluster mode.

**Key Principle**: "All commands flow through the Applier, regardless of mode."

---

## Phase 1: Foundation
**Status: 🔴 Not Started** | **Est: 2-3 days**

### Task 1.1: Create Module Structure
- [ ] Create `backend/crates/kalamdb-core/src/applier/` directory
- [ ] Create `mod.rs` with module declarations (< 50 lines)
- [ ] Add to `kalamdb-core/src/lib.rs` exports

### Task 1.2: Define Traits (`traits.rs`)
- [ ] Create `UnifiedApplier` trait
- [ ] Define `apply<C: Command>()` method signature
- [ ] Define `can_accept_writes()` and `get_leader_for()` methods
- [ ] Keep file < 100 lines

### Task 1.3: Define Commands (`command.rs`)
- [ ] Create `Command` trait with `validate`, `serialize`, `deserialize`
- [ ] Create `CommandType` enum (DDL, DML, User, Job categories)
- [ ] Create typed command structs: `CreateTableCommand`, `AlterTableCommand`, etc.
- [ ] Keep file < 150 lines

### Task 1.4: Define Errors (`error.rs`)
- [ ] Create `ApplierError` enum
- [ ] Include variants: `Validation`, `Serialization`, `NoLeader`, `Forward`, `Execution`
- [ ] Implement `From` conversions for `KalamDbError`, `RaftError`
- [ ] Keep file < 100 lines

---

## Phase 2: Command Executor
**Status: 🔴 Not Started** | **Est: 3-4 days**

### Task 2.1: Create Executor Module (`executor/mod.rs`)
- [ ] Create `CommandExecutorImpl` struct with `Arc<AppContext>`
- [ ] Define execution method signatures
- [ ] Keep file < 100 lines

### Task 2.2: DDL Execution (`executor/ddl.rs`)
- [ ] Implement `execute_create_table()` with ALL logic:
  - Persist to system.tables
  - Prime schema cache with `CachedTableData`
  - Register DataFusion provider
  - Emit `TableCreated` event
- [ ] Implement `execute_alter_table()` with same pattern
- [ ] Implement `execute_drop_table()`
- [ ] Keep file < 200 lines

### Task 2.3: Namespace Execution (`executor/namespace.rs`)
- [ ] Implement `execute_create_namespace()`
- [ ] Implement `execute_drop_namespace()`
- [ ] Keep file < 100 lines

### Task 2.4: Storage Execution (`executor/storage.rs`)
- [ ] Implement `execute_create_storage()`
- [ ] Implement `execute_drop_storage()`
- [ ] Keep file < 100 lines

### Task 2.5: User Execution (`executor/user.rs`)
- [ ] Implement `execute_create_user()`
- [ ] Implement `execute_alter_user()`
- [ ] Implement `execute_drop_user()`
- [ ] Keep file < 150 lines

### Task 2.6: DML Execution (`executor/dml.rs`)
- [ ] Implement `execute_insert()`
- [ ] Implement `execute_update()`
- [ ] Implement `execute_delete()`
- [ ] Keep file < 200 lines

---

## Phase 3: Applier Implementations
**Status: 🔴 Not Started** | **Est: 3-4 days**

### Task 3.1: Standalone Applier (`standalone.rs`)
- [ ] Implement `StandaloneApplier` struct
- [ ] Implement `UnifiedApplier` trait
- [ ] Validate command → Execute directly via `CommandExecutorImpl`
- [ ] Keep file < 100 lines

### Task 3.2: Cluster Applier (`cluster.rs`)
- [ ] Implement `ClusterApplier` struct with `Arc<RaftManager>`
- [ ] Route to correct Raft group via `GroupId`
- [ ] **Leader path**: Call `raft.client_write(serialized_cmd)` 
  - OpenRaft handles parallel replication automatically
  - Wait for `ClientWriteResponse` (means quorum committed + leader applied)
- [ ] **Follower path**: Check `raft.metrics().current_leader`, forward via `CommandForwarder`
- [ ] Handle `ClientWriteError::ForwardToLeader` — retry with new leader
- [ ] Keep file < 200 lines

### Task 3.3: Command Forwarder (`forwarder.rs`)
- [ ] Implement `CommandForwarder` with gRPC client (tonic)
- [ ] Reuse gRPC channel for connection pooling
- [ ] Forward serialized command to leader's gRPC endpoint
- [ ] Handle timeout and retry (3 attempts, exponential backoff)
- [ ] Keep file < 150 lines

### Task 3.4: Update RaftStateMachine::apply()
- [ ] Integrate `CommandExecutorImpl` into state machine's `apply()` method
- [ ] Deserialize command from entry payload
- [ ] Call `executor.execute(cmd)` and serialize result
- [ ] Return result in `ClientWriteResponse.data`

### Task 3.5: Wire to AppContext
- [ ] Add `applier: Arc<dyn UnifiedApplier>` to `AppContext`
- [ ] Add `applier()` accessor method
- [ ] Factory: `StandaloneApplier` if no cluster, else `ClusterApplier`

---

## Phase 4: Handler Simplification
**Status: 🔴 Not Started** | **Est: 2-3 days**

### Task 4.1: Simplify CREATE TABLE Handler
- [ ] Remove ALL execution logic
- [ ] Build `CreateTableCommand` from statement
- [ ] Call `applier().apply(cmd)`
- [ ] Return result
- [ ] **DELETE** `is_cluster_mode()` branch
- [ ] Target: < 80 lines

### Task 4.2: Simplify ALTER TABLE Handler
- [ ] Remove `build_altered_table_definition()`
- [ ] Remove `apply_altered_table_locally()`
- [ ] Build `AlterTableCommand` from statement
- [ ] Call `applier().apply(cmd)`
- [ ] **DELETE** `is_cluster_mode()` branch
- [ ] Target: < 80 lines

### Task 4.3: Simplify DROP TABLE Handler
- [ ] Build `DropTableCommand` from statement
- [ ] Call `applier().apply(cmd)`
- [ ] **DELETE** `is_cluster_mode()` branch
- [ ] Target: < 80 lines

### Task 4.4: Simplify All Remaining Handlers
- [ ] Namespace handlers (CREATE/DROP)
- [ ] Storage handlers (CREATE/DROP)
- [ ] User handlers (CREATE/ALTER/DROP)
- [ ] All must use: `applier().apply(cmd)` pattern
- [ ] **DELETE** all `is_cluster_mode()` checks

---

## Phase 5: Delete Old Code
**Status: 🔴 Not Started** | **Est: 2-3 days**

> **CRITICAL: Remove ALL deprecated code. No backward compatibility.**

### Task 5.1: Delete ProviderMetaApplier
- [ ] **DELETE** `backend/crates/kalamdb-core/src/applier/provider_meta_applier.rs`
- [ ] Remove from `mod.rs` exports
- [ ] Update Raft state machine to use new `CommandExecutorImpl`

### Task 5.2: Delete table_creation Helpers
- [ ] **DELETE** `table_creation::create_table()` function
- [ ] **DELETE** `table_creation::build_table_definition()` function
- [ ] **DELETE** `create_user_table()`, `create_shared_table()`, `create_stream_table()`
- [ ] Keep only utility functions if needed
- [ ] If file becomes < 50 lines, merge into executor

### Task 5.3: Delete Mode-Specific Code
- [ ] **DELETE** all `is_cluster_mode()` checks from handlers
- [ ] **DELETE** `execute_meta()` calls from handlers (now in applier)
- [ ] **DELETE** `apply_altered_table_locally()` method

### Task 5.4: Delete Unused Imports and Functions
- [ ] Run `cargo clippy --all-features` 
- [ ] Fix all unused import warnings
- [ ] Fix all dead code warnings
- [ ] Remove any remaining TODO comments about "old way"

### Task 5.5: Verify Clean Build
- [ ] Run `cargo build --release` with no warnings
- [ ] Run `cargo fmt --check`
- [ ] Run `cargo clippy` with no warnings

---

## Phase 6: Event System
**Status: 🔴 Not Started** | **Est: 1-2 days**

### Task 6.1: Create Event Module (`events/mod.rs`)
- [ ] Define `DatabaseEvent` enum (TableCreated, TableAltered, RowsInserted, etc.)
- [ ] Keep file < 100 lines

### Task 6.2: Create Broadcast System (`events/broadcast.rs`)
- [ ] Set up `tokio::sync::broadcast` channel
- [ ] Create `EventBroadcaster` struct
- [ ] Wire to `CommandExecutorImpl`
- [ ] Keep file < 50 lines

### Task 6.3: Audit Log Handler (`events/handlers/audit.rs`)
- [ ] Subscribe to all DDL/DML events
- [ ] Write to `system.audit_logs`
- [ ] **DELETE** inline audit logging from handlers
- [ ] Keep file < 100 lines

### Task 6.4: Live Query Handler (`events/handlers/live_query.rs`)
- [ ] Subscribe to DML events (Insert/Update/Delete)
- [ ] Notify WebSocket subscribers
- [ ] Keep file < 100 lines

---

## Phase 7: Testing & Validation
**Status: 🔴 Not Started** | **Est: 2-3 days**

### Task 7.1: Run All Tests
- [ ] Run `cargo test --test smoke` — must pass 99 tests
- [ ] Run `cargo test --test cluster` — must pass 47 tests
- [ ] Fix any failures

### Task 7.2: Add Unit Tests for Executor
- [ ] Test `execute_create_table()` in isolation
- [ ] Test `execute_alter_table()` in isolation
- [ ] Test command validation logic

### Task 7.3: Benchmark Performance
- [ ] Benchmark CREATE TABLE latency (standalone)
- [ ] Benchmark CREATE TABLE latency (cluster)
- [ ] Ensure < 5% regression from baseline

### Task 7.4: Delete Obsolete Tests
- [ ] **DELETE** tests for removed functions
- [ ] **DELETE** tests for old code paths
- [ ] Ensure no tests reference deleted code

---

## Implementation Progress

| Phase | Description | Status | Tasks Done |
|-------|-------------|--------|------------|
| 1 | Foundation | 🔴 | 0/4 |
| 2 | Command Executor | 🔴 | 0/6 |
| 3 | Applier Implementations | 🔴 | 0/5 |
| 4 | Handler Simplification | 🔴 | 0/4 |
| 5 | Delete Old Code | 🔴 | 0/5 |
| 6 | Event System | 🔴 | 0/4 |
| 7 | Testing & Validation | 🔴 | 0/4 |

**Total Progress: 0/32 tasks**

---

## Code Deletion Tracker

| File/Function | Status | Reason |
|--------------|--------|--------|
| `provider_meta_applier.rs` | ❌ To Delete | Replaced by `CommandExecutorImpl` |
| `table_creation::create_table()` | ❌ To Delete | Logic in `executor/ddl.rs` |
| `table_creation::build_table_definition()` | ❌ To Delete | Logic in `executor/ddl.rs` |
| `table_creation::create_user_table()` | ❌ To Delete | Logic in `executor/ddl.rs` |
| `table_creation::create_shared_table()` | ❌ To Delete | Logic in `executor/ddl.rs` |
| `table_creation::create_stream_table()` | ❌ To Delete | Logic in `executor/ddl.rs` |
| `alter.rs::apply_altered_table_locally()` | ❌ To Delete | Logic in `executor/ddl.rs` |
| `alter.rs::build_altered_table_definition()` | ❌ To Delete | Logic in command builder |
| All handler `is_cluster_mode()` | ❌ To Delete | No mode branching |
| Handler inline audit logging | ❌ To Delete | Moved to event handler |

---

## Success Criteria

After implementation, verify:

- [ ] Zero `is_cluster_mode()` calls in any handler
- [ ] Zero duplicate execution logic (one `CommandExecutorImpl`)
- [ ] All 99 smoke tests pass
- [ ] All 47 cluster tests pass
- [ ] `provider_meta_applier.rs` is deleted
- [ ] Handler files are < 100 lines each
- [ ] Executor files are < 200 lines each
- [ ] `cargo clippy` passes with no warnings
- [ ] Latency regression < 5%

---

## Dependencies

### Required Before Starting
- ✅ Raft-first pattern implemented (CREATE/ALTER TABLE)
- ✅ All cluster tests passing
- ✅ All smoke tests passing
- ✅ Spec 019 finalized with invariants

### No New External Dependencies
- Using existing `tokio::sync::broadcast` for events
- Using existing `RaftManager` for consensus
- Using existing gRPC (tonic) for forwarding

---

## Notes

1. **No Feature Flags**: Direct replacement, delete old code immediately
2. **No Backward Compatibility**: Clean break, simpler codebase
3. **Trust OpenRaft Completely**: Never re-implement Raft internals
4. **Small Files**: If a file exceeds limits, split it immediately
5. **Test After Each Phase**: Run full test suite before moving to next phase

---

## OpenRaft API Quick Reference

```rust
// PROPOSING WRITES (Leader only)
let response = raft.client_write(command_bytes).await?;
// response.log_id: LogId — position in log
// response.data: Vec<u8> — result from apply()

// LEADER DETECTION
let metrics = raft.metrics().borrow().clone();
let am_leader = metrics.current_leader == Some(my_node_id);

// CONSISTENT READS
let read_log_id = raft.ensure_linearizable().await?;
// Now safe to read local state machine

// ERROR HANDLING
match raft.client_write(cmd).await {
    Err(RaftError::APIError(ClientWriteError::ForwardToLeader(info))) => {
        // Forward to info.leader_node
    }
    // ...
}
```

**DO NOT** implement:
- Custom quorum counting
- Manual log replication
- Follower progress tracking
- Custom heartbeat logic
