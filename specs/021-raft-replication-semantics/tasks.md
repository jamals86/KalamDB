# Spec 021: Implementation Tasks (REVISED v2)

## Executive Summary

**Current State**: 
- Raft log replication works correctly
- Live query notifications already fire locally on each node after apply
- Performance is 4x slower than pre-Raft due to watermark waiting + serialization overhead

**Goals**:
1. Fix stale reads for client queries (route to leader)
2. Keep live queries working as-is (notifications are already local)
3. Improve leader apply performance (reduce 4x slowdown)

---

## Live Query Design Confirmation ✅

**The current design is already correct for live queries:**

```
Client INSERT on Node 1 (Leader)
       │
       ▼
┌─────────────────────────────────────┐
│  Raft Consensus (log replication)   │
└─────────────────────────────────────┘
       │                    │
       ▼                    ▼
   Node 1 Apply         Node 2 Apply (async)
       │                    │
       ▼                    ▼
  RocksDB Write        RocksDB Write
       │                    │
       ▼                    ▼
  notify_table_change_async()   notify_table_change_async()
       │                    │
       ▼                    ▼
  Push to WS clients    Push to WS clients
  connected to Node 1   connected to Node 2
```

**Key insight**: Each node notifies its own connected clients after local apply.
- Client on Node 1 gets notification immediately after leader applies
- Client on Node 2 gets notification after follower applies (few ms later)
- NO cross-node notification broadcast needed

---

## Performance Analysis (4x Slowdown)

### Bottleneck 1: Watermark Waiting
```rust
// In user_data.rs:apply()
if required_meta > current_meta {
    get_coordinator().wait_for(required_meta).await;  // BLOCKING
}
```
**Problem**: Every data command waits for Meta group to catch up.
**Fix**: Remove watermark waiting for DML (only DDL needs it).

### Bottleneck 2: Serialization Overhead
```rust
// Every command is serialized twice (encode) and deserialized twice (decode)
let cmd: UserDataCommand = decode(command)?;  // Hot path
let response_data = encode(&response)?;       // Hot path
```
**Fix**: Use zero-copy serialization (rkyv) or pool serialization buffers.

### Bottleneck 3: Lock Contention
```rust
let applier = {
    let guard = self.applier.read();  // RwLock on EVERY apply
    guard.clone()
};
```
**Fix**: Cache applier in `Arc` without RwLock (it never changes after init).

---

## Revised Implementation Plan

### Phase 1: Leader Apply Performance (P0 - Critical)

Make the leader apply path fast. This is the critical path for write latency.

| Task | Effort | Impact |
|------|--------|--------|
| 1.1: Remove watermark wait for DML commands | 2h | High |
| 1.2: Cache applier without RwLock | 1h | Medium |
| 1.3: Add apply latency metrics | 1h | Debug |

### Phase 2: Read Routing (P0 - Critical)

Route client reads to leader for consistency.

| Task | Effort | Impact |
|------|--------|--------|
| 2.1: Add `ReadContext` enum (simple version) | 1h | Foundation |
| 2.2: Add `is_leader_for_user()` to AppContext | 1h | Foundation |
| 2.3: Check ReadContext in UserTableProvider.scan() | 2h | High |
| 2.4: Return NOT_LEADER error with leader hint | 1h | UX |

### Phase 3: Verification (P1 - Important)

Verify existing systems work correctly.

| Task | Effort | Impact |
|------|--------|--------|
| 3.1: Add test: live query notification on follower | 2h | Confidence |
| 3.2: Add test: stale read returns NOT_LEADER | 2h | Correctness |
| 3.3: Add benchmark: write latency pre/post fix | 1h | Metrics |

---

## Detailed Task Implementations

### Task 1.1: Remove Watermark Wait for DML

**Problem**: DML commands (INSERT/UPDATE/DELETE) don't need Meta synchronization.
The watermark was added for DDL safety but is applied to ALL commands.

**File**: `backend/crates/kalamdb-raft/src/state_machine/user_data.rs`

```rust
// BEFORE: All commands wait for watermark
async fn apply(&self, index: u64, term: u64, command: &[u8]) -> Result<ApplyResult, RaftError> {
    let cmd: UserDataCommand = decode(command)?;
    let required_meta = cmd.required_meta_index();
    if required_meta > current_meta {
        get_coordinator().wait_for(required_meta).await;  // SLOW!
    }
    // ...
}

// AFTER: Only wait when required_meta > 0 (DDL-dependent commands)
async fn apply(&self, index: u64, term: u64, command: &[u8]) -> Result<ApplyResult, RaftError> {
    let cmd: UserDataCommand = decode(command)?;
    let required_meta = cmd.required_meta_index();
    // Only wait if this command explicitly depends on Meta
    // DML commands set required_meta_index = 0
    if required_meta > 0 && required_meta > current_meta {
        get_coordinator().wait_for(required_meta).await;
    }
    // ...
}
```

**Also update DML command creation** to set `required_meta_index: 0`:

```rust
// In UserDataCommand creation for INSERT/UPDATE/DELETE
UserDataCommand::Insert {
    table_id,
    user_id,
    rows,
    required_meta_index: 0,  // DML doesn't need Meta sync
}
```

### Task 1.2: Cache Applier Without RwLock

**File**: `backend/crates/kalamdb-raft/src/state_machine/user_data.rs`

```rust
// BEFORE: RwLock on every apply
pub struct UserDataStateMachine {
    applier: RwLock<Option<Arc<dyn UserDataApplier>>>,
}

// Hot path:
let applier = {
    let guard = self.applier.read();  // Lock contention!
    guard.clone()
};

// AFTER: OnceCell for one-time init, then direct access
use std::sync::OnceLock;

pub struct UserDataStateMachine {
    applier: OnceLock<Arc<dyn UserDataApplier>>,
}

impl UserDataStateMachine {
    pub fn set_applier(&self, applier: Arc<dyn UserDataApplier>) {
        let _ = self.applier.set(applier);  // First call wins
    }
    
    fn get_applier(&self) -> Option<&Arc<dyn UserDataApplier>> {
        self.applier.get()  // No lock!
    }
}
```

### Task 2.1: Simple ReadContext Enum

**File**: `backend/crates/kalamdb-commons/src/models/read_context.rs`

Keep it simple - only 2 variants that matter:

```rust
/// Read routing context
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadContext {
    /// Client SQL query - must go to leader for consistency
    #[default]
    Client,
    
    /// Internal operation (jobs, notifications) - always local
    Internal,
}

impl ReadContext {
    pub fn requires_leader(&self) -> bool {
        matches!(self, ReadContext::Client)
    }
}
```

### Task 2.3: Check ReadContext in Scan

**File**: `backend/crates/kalamdb-core/src/providers/users.rs`

```rust
// Add to UserTableProvider
fn check_read_routing(&self, read_context: ReadContext) -> Result<(), KalamDbError> {
    if read_context.requires_leader() {
        // For client reads, must be on leader
        if !self.core.app_context.is_leader_for_user(&self.user_id()) {
            let leader_addr = self.core.app_context.leader_for_user(&self.user_id());
            return Err(KalamDbError::NotLeader { leader_addr });
        }
    }
    // Internal reads always proceed locally
    Ok(())
}
```

---

## What We DON'T Need to Change

1. **Live query notifications** - Already fire locally on each node ✅
2. **Live query subscriptions** - Node-local by design ✅
3. **Job executors** - Already read local data ✅
4. **Flush/compaction** - Only runs on leader anyway ✅

---

## Testing Checklist

- [ ] Write latency < 10ms (was ~40ms due to 4x slowdown)
- [ ] Client read on follower returns NOT_LEADER
- [ ] Client read on leader succeeds
- [ ] Live query client on follower gets notification after follower apply
- [ ] Internal job (FlushExecutor) reads local data, never forwards

---

## Summary

The design is simpler than initially proposed:

1. **Performance fix**: Remove unnecessary watermark waiting for DML
2. **Read routing**: Simple `Client` vs `Internal` context
3. **Live queries**: Already work correctly, no changes needed

Total estimated effort: ~12 hours (vs original 40+ hours)
