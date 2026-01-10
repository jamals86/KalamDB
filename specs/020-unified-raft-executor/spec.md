# 020 — Unified Raft Executor (Single-Node and Cluster)

## Overview

Eliminate the separate "standalone" code path and use OpenRaft for **all** modes. In standalone mode, we run a single-node Raft cluster. This ensures:
- Same code path tested in both modes
- No duplicate executor implementations
- OpenRaft handles single-node efficiently (no network overhead)

## Current State (Problems)

### Duplicate Executors
1. `kalamdb-raft/src/executor/direct.rs` - Stub DirectExecutor (placeholder)
2. `kalamdb-core/src/executor/standalone.rs` - StandaloneExecutor (500+ lines)
3. `kalamdb-core/src/applier/executor/` - CommandExecutorImpl (actual logic)

### Duplicate Command Types
1. `kalamdb-raft/src/commands/meta.rs` - MetaCommand enum
2. `kalamdb-core/src/applier/commands/types.rs` - Separate structs

### Duplicate Applier Logic
- Standalone mode: Direct provider calls in SQL handlers
- Cluster mode: Raft → Applier → Executor → Provider

## Target State

```text
┌─────────────────────────────────────────────────────────────────────┐
│                          SQL Handler                                 │
└─────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        RaftExecutor                                  │
│  (Routes to Meta group or Data shard based on command type)         │
└─────────────────────────────────────────────────────────────────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    ▼                              ▼
           ┌────────────────┐             ┌────────────────┐
           │   Meta Group   │             │  Data Shards   │
           │  (1 Raft node  │             │ (32 user + 1   │
           │   or cluster)  │             │  shared shard) │
           └────────────────┘             └────────────────┘
                    │                              │
                    ▼                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     Raft State Machines                              │
│  ┌─────────────────┐  ┌───────────────────┐  ┌──────────────────┐   │
│  │MetaStateMachine │  │UserDataStateMachine│  │SharedDataStateMachine│
│  └────────┬────────┘  └─────────┬─────────┘  └─────────┬────────┘   │
└───────────┼─────────────────────┼──────────────────────┼────────────┘
            │                     │                      │
            ▼                     ▼                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     Appliers (Trait Implementations)                 │
│  ┌─────────────────┐  ┌───────────────────┐  ┌──────────────────┐   │
│  │ProviderMetaApplier│ProviderUserDataApplier│ProviderSharedDataApplier│
│  └────────┬────────┘  └─────────┬─────────┘  └─────────┬────────┘   │
└───────────┼─────────────────────┼──────────────────────┼────────────┘
            │                     │                      │
            ▼                     ▼                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     CommandExecutorImpl                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐ │
│  │DdlExecutor│  │DmlExecutor│  │NamespaceExec│StorageExec│UserExec│ │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘  └────────┘ │
└─────────────────────────────────────────────────────────────────────┘
            │                     │                      │
            ▼                     ▼                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     Providers (RocksDB / Parquet)                    │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Changes

### 1. Always Use RaftExecutor
- Remove `StandaloneExecutor` from kalamdb-core
- Remove `DirectExecutor` from kalamdb-raft  
- Configure OpenRaft with `voters: [self]` for single-node mode

### 2. Single-Node Raft Configuration
```rust
// Single node = cluster of 1
let config = RaftConfig {
    cluster_name: "standalone",
    node_id: 1,
    initial_members: vec![1], // Just self
    ..Default::default()
};
```

### 3. Remove Duplicate Command Types
- Keep `kalamdb-raft/src/commands/` as the source of truth
- Delete `kalamdb-core/src/applier/commands/types.rs`
- Update applier to use Raft command types directly

### 4. Unify SQL Handler DML Path
- Always route INSERT/UPDATE/DELETE through RaftExecutor
- Remove direct provider calls from handlers

---

## Tasks

### Phase 1: Configuration Changes
- [x] Task 1.1: Update RaftManager to support single-node initialization
- [x] Task 1.2: Update AppContext to always create RaftExecutor
- [x] Task 1.3: Remove is_cluster_mode() branching from applier creation

### Phase 2: Remove Duplicate Executors
- [x] Task 2.1: Delete `kalamdb-core/src/executor/standalone.rs`
- [x] Task 2.2: Delete `kalamdb-raft/src/executor/direct.rs`
- [x] Task 2.3: Update all imports and re-exports

### Phase 3: Documentation & Cleanup
- [x] Task 3.1: Update lib.rs documentation in kalamdb-raft
- [x] Task 3.2: Update executor trait documentation
- [x] Task 3.3: Verify all crates compile successfully

### Phase 4: Applier Simplification
- [x] Task 4.1: Remove StandaloneApplier - keep only RaftApplier
- [x] Task 4.2: Delete `applier/cluster.rs` and `applier/forwarder.rs`
- [x] Task 4.3: Add DML commands to `applier/commands/types.rs` (Insert/Update/Delete for User and Shared data)
- [x] Task 4.4: Add DML methods to `UnifiedApplier` trait
- [x] Task 4.5: Update `create_applier()` to take no parameters (always returns RaftApplier)
- [x] Task 4.6: Verify all crates compile successfully

### Phase 5: Command Deduplication ✅
- [x] Task 5.1: Delete `kalamdb-core/src/applier/commands/` directory entirely
- [x] Task 5.2: Update `UnifiedApplier` trait to accept raw parameters instead of command structs
- [x] Task 5.3: Build `MetaCommand`, `UserDataCommand`, `SharedDataCommand` enums directly in RaftApplier
- [x] Task 5.4: Update all SQL handlers to pass raw parameters to applier methods
- [x] Task 5.5: Verify all crates compile successfully

### Phase 6: DML Handler Unification ✅
- [x] Task 6.1: Delete `kalamdb-core/src/executor/` directory (unused re-exports)
- [x] Task 6.2: Remove `pub mod executor` from `lib.rs`
- [x] Task 6.3: Remove `is_cluster_mode()` checks from INSERT handler - always route through Raft
- [x] Task 6.4: Remove `is_cluster_mode()` checks from UPDATE handler - always route through Raft  
- [x] Task 6.5: Remove `is_cluster_mode()` checks from DELETE handler - always route through Raft
- [x] Task 6.6: Simplify SQL forwarding logic in `kalamdb-api` - remove unnecessary cluster mode check
- [x] Task 6.7: Clean up standalone/cluster mode comments throughout codebase
- [x] Task 6.8: Verify all crates compile successfully

---

## Implementation Notes

### OpenRaft Single-Node Mode
OpenRaft efficiently handles single-node clusters:
- No network calls when there's only one voter
- Logs are applied immediately after writing
- Leader election is instant (self-vote)

### Backward Compatibility
- Config with no `[cluster]` section → single-node Raft
- Config with `[cluster]` section → multi-node Raft
- Same RaftExecutor used for both

### Testing Benefits
- Unit tests use single-node Raft (fast, no network)
- Integration tests can test cluster mode
- Same code path guaranteed
