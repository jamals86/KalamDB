# Live Queries Architecture Refactoring Tasks

**Status**: Proposed architectural improvements based on code review
**Date**: January 23, 2025
**Context**: Post Phase 3 (US1 Complete) - Before implementing US2

## Overview

This document captures architectural improvements needed for the Live Queries system based on recent implementation review. These refactors should be completed before Phase 4 (US2) to ensure clean, maintainable code.

---

## R1: Data Model Simplification

### R1.1: Remove redundant `query_id` field from `LiveQuery` model

**Current State**:
```rust
pub struct LiveQuery {
    pub live_id: LiveQueryId,        // Full composite: user-conn-table-query
    pub connection_id: String,
    pub subscription_id: String,     // Client-provided ID
    pub query_id: String,            // ❌ REDUNDANT - duplicates subscription_id
    // ...
}
```

**Problem**: `query_id` and `subscription_id` serve the same purpose. The client provides `subscription_id`, and we should use that consistently.

**Solution**:
- Remove `query_id` field from `LiveQuery` struct in `backend/crates/kalamdb-commons/src/system/mod.rs`
- Remove `query_id` column from `live_queries_table_definition()` in `backend/crates/kalamdb-system/src/system_table_definitions/live_queries.rs`
- Update `LiveQueriesTableProvider` to remove `query_id` handling in `backend/crates/kalamdb-system/src/providers/live_queries/live_queries_provider.rs`
- Update all call sites that reference `query_id` to use `subscription_id` instead

**Files to modify**:
- `backend/crates/kalamdb-commons/src/system/mod.rs`
- `backend/crates/kalamdb-system/src/system_table_definitions/live_queries.rs`
- `backend/crates/kalamdb-system/src/providers/live_queries/live_queries_provider.rs`
- `backend/crates/kalamdb-core/src/live/subscription.rs`

**Validation**: Run `cargo test -p kalamdb-system` and `cargo test -p kalamdb-core --test live_multi_subscription`

---

### R1.2: Update `LiveQueryId` to include `subscription_id` instead of `query_id`

**Current State**:
```rust
// LiveId format: {user_id}-{unique_conn_id}-{table_id}-{query_id}
pub struct LiveId {
    pub connection_id: ConnectionId,
    pub table_id: TableId,
    pub query_id: String,  // ❌ Should be subscription_id
}
```

**Problem**: The `LiveId` composite key uses `query_id` but should use `subscription_id` for consistency with client-provided identifiers.

**Solution**:
- Rename `query_id` field to `subscription_id` in `LiveId` struct
- Update `LiveId::new()` constructor parameter name
- Update all parsing/formatting logic to use "subscription_id" terminology
- Update composite key format documentation

**Files to modify**:
- `backend/crates/kalamdb-commons/src/models/live_id.rs` (or wherever `LiveId` is defined)
- `backend/crates/kalamdb-core/src/live/connection_registry.rs`
- `backend/crates/kalamdb-core/src/live/subscription.rs`

**Validation**: Run `cargo test -p kalamdb-commons` and verify all LiveId tests pass

---

### R1.3: Convert `status` from String to enum

**Current State**:
```rust
pub struct LiveQuery {
    pub status: String,  // ❌ Stringly-typed, error-prone
}
```

**Problem**: Status is represented as a string, which allows invalid values and makes pattern matching verbose.

**Solution**:
- Create `LiveQueryStatus` enum in `backend/crates/kalamdb-commons/src/models/mod.rs`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  pub enum LiveQueryStatus {
      Active,
      Paused,
      Terminated,
  }
  
  impl LiveQueryStatus {
      pub fn as_str(&self) -> &'static str {
          match self {
              Self::Active => "active",
              Self::Paused => "paused",
              Self::Terminated => "terminated",
          }
      }
      
      pub fn from_str(s: &str) -> Result<Self, String> {
          match s.to_lowercase().as_str() {
              "active" => Ok(Self::Active),
              "paused" => Ok(Self::Paused),
              "terminated" => Ok(Self::Terminated),
              _ => Err(format!("Invalid status: {}", s)),
          }
      }
  }
  ```
- Update `LiveQuery` struct to use `LiveQueryStatus` type
- Update table provider to serialize/deserialize enum correctly
- Update system table definition to keep TEXT column but validate on insert

**Files to modify**:
- `backend/crates/kalamdb-commons/src/models/mod.rs`
- `backend/crates/kalamdb-commons/src/system/mod.rs`
- `backend/crates/kalamdb-system/src/providers/live_queries/live_queries_provider.rs`
- `backend/crates/kalamdb-core/src/live/subscription.rs`

**Validation**: Run `cargo test -p kalamdb-commons` and `cargo test -p kalamdb-system`

---

### R1.4: Change `last_seq_id` type from `i64` to `SeqId`

**Current State**:
```rust
pub struct LiveQuery {
    pub last_seq_id: i64,  // ❌ Should use proper type wrapper
}
```

**Problem**: Using raw `i64` loses type safety and semantic meaning.

**Solution**:
- Update `LiveQuery.last_seq_id` field type to `kalamdb_commons::ids::SeqId`
- Update table provider to convert between `SeqId` and `i64` for storage
- Ensure all call sites use `SeqId` type correctly

**Files to modify**:
- `backend/crates/kalamdb-commons/src/system/mod.rs`
- `backend/crates/kalamdb-system/src/providers/live_queries/live_queries_provider.rs`
- `backend/crates/kalamdb-core/src/live/subscription.rs`

**Validation**: Run `cargo test -p kalamdb-system`

---

### R1.5: Remove `last_seq_id` persistence (keep in-memory only)

**Current State**:
```rust
pub struct LiveQuery {
    pub last_seq_id: i64,  // ❌ Stored in RocksDB but rarely useful
}
```

**Problem**: `last_seq_id` is a transient runtime state that doesn't need to be persisted. It's only useful while the connection is alive.

**Solution**:
- Remove `last_seq_id` field from `LiveQuery` struct
- Remove `last_seq_id` column from `live_queries_table_definition()`
- Keep `last_seq_id` tracking in-memory only (within `WebSocketSession` actor state)
- Update table provider to remove `last_seq_id` handling

**Files to modify**:
- `backend/crates/kalamdb-commons/src/system/mod.rs`
- `backend/crates/kalamdb-system/src/system_table_definitions/live_queries.rs`
- `backend/crates/kalamdb-system/src/providers/live_queries/live_queries_provider.rs`
- `backend/crates/kalamdb-api/src/actors/ws_session.rs` (ensure actor tracks this internally)

**Validation**: Run `cargo test -p kalamdb-system` and verify metadata tests still pass

**Note**: This task depends on R1.4 being reverted/skipped since we're removing the field entirely.

---

## R2: Runtime Status Management

### R2.1: Fetch `status` from in-memory DashMap first, fallback to RocksDB

**Current State**:
- Status is always read from RocksDB via `LiveQueriesTableProvider`
- No attempt to check in-memory connection registry first

**Problem**: Status should reflect the current runtime state. If a subscription is active in-memory, reading from RocksDB may show stale state.

**Solution**:
- Add `get_runtime_status()` method to `LiveQueryRegistry` in `backend/crates/kalamdb-core/src/live/connection_registry.rs`:
  ```rust
  impl LiveQueryRegistry {
      pub fn get_runtime_status(&self, live_id: &LiveId) -> Option<LiveQueryStatus> {
          // Check if subscription exists in DashMap
          let key = live_id.lookup_key(); // (user_id, table_id)
          self.subscriptions.get(&key).and_then(|handles| {
              handles.iter()
                  .find(|h| &h.live_id == live_id)
                  .map(|_| LiveQueryStatus::Active)
          })
      }
  }
  ```
- Update `LiveQueriesTableProvider::scan_all_live_queries()` to:
  1. Load rows from RocksDB
  2. Check `LiveQueryRegistry` for runtime status
  3. Override status with runtime value if found
  4. Mark as `Terminated` if not in runtime and node matches current node

**Files to modify**:
- `backend/crates/kalamdb-core/src/live/connection_registry.rs`
- `backend/crates/kalamdb-system/src/providers/live_queries/live_queries_provider.rs`
- Wire `Arc<LiveQueryRegistry>` into `LiveQueriesTableProvider` constructor

**Validation**: Write test that subscribes, queries `system.live_queries`, unsubscribes, queries again, and verifies status changes

**Future Optimization**: Add node_id check: only consult in-memory registry if `row.node == current_node_id`

---

## R3: Performance Optimizations

### R3.1: Optimize `handle_unsubscribe` to avoid redundant table name parsing

**Current State** (in `backend/crates/kalamdb-api/src/actors/ws_session.rs`):
```rust
fn handle_unsubscribe(&mut self, ctx: &mut ws::WebsocketContext<Self>, subscription_id: String) {
    // Get metadata to reconstruct LiveId
    let (sql, _, _, _) = self.subscription_metadata.get(&subscription_id)?;
    
    // ❌ REDUNDANT: Parse table name from SQL again
    let table_name = manager.extract_table_name_from_query(&sql_clone)?;
    let (namespace, table) = table_name.split_once('.')?;
    let table_id = TableId::new(namespace_id, table_name);
    
    let live_id = LiveId::new(live_conn_id, table_id, sub_id);
}
```

**Problem**: We're parsing the SQL query to extract the table name, but we already have the `LiveId` stored somewhere (or can store it).

**Solution**:
- Store `LiveId` directly in `subscription_metadata` map instead of just `(sql, user_id, snapshot_end_seq, batch_size)`
- Change type to: `HashMap<String, (LiveId, String, SeqId, usize)>` where second String is SQL for batch fetching
- In `handle_unsubscribe`, retrieve `LiveId` directly from metadata
- No need to parse table name again

**Files to modify**:
- `backend/crates/kalamdb-api/src/actors/ws_session.rs`

**Validation**: Run `cargo test --test test_live_queries_metadata` and verify unsubscribe still works

---

## R4: Client-Side Contract Enforcement

### R4.1: Make `SubscriptionConfig.id` required (non-optional)

**Current State** (in `link/src/models.rs` or TypeScript SDK):
```rust
pub struct SubscriptionConfig {
    pub id: Option<String>,  // ❌ Should be required
    pub sql: String,
    // ...
}
```

**Problem**: Subscription ID should always be provided by the client. Making it optional creates ambiguity about who generates IDs.

**Solution**:
- Change `SubscriptionConfig.id` from `Option<String>` to `String` (required)
- Update TypeScript SDK to require `id` field in subscription options
- Update documentation to clarify client must provide unique subscription IDs per connection
- Add validation in backend to reject subscriptions without ID

**Files to modify**:
- `link/src/models.rs`
- `link/sdks/typescript/src/live/types.ts`
- `backend/crates/kalamdb-api/src/models.rs` (if duplicated)
- `backend/crates/kalamdb-api/src/actors/ws_session.rs` (add validation)

**Validation**: Update SDK tests to always provide subscription IDs

---

## R5: Actor State Consolidation

### R5.1: Move subscription tracking into `WebSocketSession` actor

**Current State**:
- Subscriptions are tracked in both `WebSocketSession.subscriptions` (Vec of IDs) and `LiveQueryRegistry` (DashMap)
- Metadata scattered across multiple fields in actor
- LiveQueryManager holds most logic

**Problem**: The actor should own its connection state. Currently, logic is split between actor and manager, making state ownership unclear.

**Proposed Solution** (INVESTIGATION TASK):
- Evaluate moving subscription lifecycle into actor itself:
  - Actor owns: `HashMap<subscription_id, SubscriptionState>` where `SubscriptionState` contains:
    - `live_id: LiveId`
    - `table_id: TableId`
    - `sql: String`
    - `last_seq: Option<SeqId>`
    - `batch_size: usize`
    - `status: LiveQueryStatus`
  - LiveQueryManager becomes a passive service for:
    - Initial data fetching
    - Filter compilation
    - Change notification broadcasting
  - Actor is responsible for:
    - Register/unregister with manager
    - Track all subscription state
    - Handle unsubscribe directly (no async manager call needed)

**Benefits**:
- Single source of truth for connection state
- Simpler unsubscribe logic (no manager async call)
- Clearer ownership model (actor owns its subscriptions)
- Easier to add per-subscription features (pause/resume)

**Drawbacks**:
- Larger actor struct
- Need to ensure `system.live_queries` sync on actor drop
- Manager can't directly query subscription state (must go through actor)

**Files to investigate**:
- `backend/crates/kalamdb-api/src/actors/ws_session.rs`
- `backend/crates/kalamdb-core/src/live/manager/core.rs`
- `backend/crates/kalamdb-core/src/live/connection_registry.rs`

**Decision needed**: Should we consolidate state in actor or keep current split?

**Validation**: If proceeding, ensure all existing tests pass with new architecture

---

## Execution Order

### Critical Path (Must do before US2):
1. **R1.1**: Remove `query_id` field (cleanup)
2. **R1.2**: Update `LiveId` to use `subscription_id` (consistency)
3. **R1.3**: Convert `status` to enum (type safety)
4. **R1.5**: Remove `last_seq_id` persistence (skip R1.4) (simplification)
5. **R3.1**: Optimize `handle_unsubscribe` (performance)
6. **R4.1**: Make subscription ID required (contract)

### High Priority (Should do before US2):
7. **R2.1**: Fetch status from in-memory first (correctness)

### Investigation (Can defer to post-MVP):
8. **R5.1**: Evaluate actor state consolidation (architecture)

### Dependencies:
- R1.2 depends on R1.1 (both touch same ID fields)
- R1.5 supersedes R1.4 (don't do both)
- R2.1 depends on R1.3 (needs enum type)
- R3.1 is independent (can do anytime)
- R4.1 is independent (can do anytime)
- R5.1 is a larger architectural change (investigate separately)

---

## Success Criteria

After completing critical path refactors:
- ✅ No redundant ID fields (`query_id` removed)
- ✅ Type-safe status handling (enum, not string)
- ✅ Consistent ID terminology (`subscription_id` everywhere)
- ✅ Simpler data model (no persisted `last_seq_id`)
- ✅ Faster unsubscribe (no redundant parsing)
- ✅ Clear client contract (required subscription ID)
- ✅ All existing tests pass
- ✅ `cargo test --test test_live_queries_metadata` passes
- ✅ `cargo test -p kalamdb-core --test live_multi_subscription` passes

---

## Notes

- These refactors are **blocking for US2** (metadata observability) because they affect the schema and runtime behavior
- Prioritize R1.* tasks first (data model cleanup)
- R5.1 is marked as investigation because it's a significant architectural change that needs design review
- All changes should maintain backward compatibility with existing WebSocket protocol (client messages unchanged)
- Update `specs/014-live-queries-websocket/data-model.md` after completing R1.* tasks

