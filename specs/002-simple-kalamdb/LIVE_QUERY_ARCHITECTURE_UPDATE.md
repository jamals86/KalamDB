# Live Query Architecture Update Summary

**Date**: 2025-10-15  
**Updated**: spec.md and tasks.md with in-memory actor registry and node-aware live query architecture

## Changes Made

### 1. **Updated system.live_queries Table Schema**

**Previous Schema**:
```
- id (subscription identifier)
- user_id
- query
- created_at
- bandwidth (bytes/messages sent)
```

**New Schema**:
```
- sub_id (unique subscription identifier)
- user_id (owner of the subscription)
- query (SQL query text)
- created_at (when subscription was created)
- updated_at (last notification timestamp)
- changes (counter: total notifications sent)
- node (which node owns the WebSocket connection)
```

**Key Changes**:
- ✅ `id` → `sub_id` (more explicit naming)
- ✅ Added `updated_at` for tracking last activity
- ✅ `bandwidth` → `changes` (simpler metric: count of notifications)
- ✅ Added `node` field for cluster-aware delivery

### 2. **In-Memory WebSocket Actor Registry**

**New Component**: `backend/crates/kalamdb-core/src/live_query/actor_registry.rs`

```rust
struct LiveQueryRegistry {
    // Map: sub_id → WebSocket Actor Address
    actors: HashMap<String, Addr<WebSocketSession>>,
    
    // Map: connection_id → List of sub_ids
    connections: HashMap<String, Vec<String>>,
    
    // Current node identifier
    node_id: String,
}
```

**Purpose**:
- Maintain in-memory mapping of subscription IDs to WebSocket actors
- Enable direct message delivery to WebSocket connections
- Track which subscriptions belong to each connection
- Support multiple subscriptions per WebSocket connection

### 3. **Node-Aware Change Detection**

**Architecture**:

```
Data Change Detected (INSERT/UPDATE/DELETE)
    ↓
Find matching subscriptions in system.live_queries
    ↓
Filter by node field:
  - WHERE node = current_node_id
    ↓
For each matching sub_id:
  - Lookup actor in in-memory registry
  - Send notification message to actor
  - Increment changes counter
  - Update updated_at timestamp
```

**Benefits**:
- ✅ **Prevents duplicate notifications** in clustered deployments
- ✅ **Efficient**: Only the node owning the WebSocket delivers messages
- ✅ **Scalable**: Other nodes skip notifications for remote connections

### 4. **Multiple Subscriptions Per Connection**

**Previous Limitation**: One subscription per WebSocket connection (implied)

**New Capability**: Each WebSocket connection can have **multiple live query subscriptions**

**Example Flow**:
```
User connects to WebSocket
    ↓
User sends subscription array:
[
  { sql: "SELECT * FROM messages WHERE conversation_id = 'conv1'" },
  { sql: "SELECT * FROM messages WHERE sender_id = 'user123'" },
  { sql: "SELECT * FROM notifications WHERE user_id = CURRENT_USER()" }
]
    ↓
System creates 3 separate subscriptions:
  - sub_id_1 → saved in system.live_queries, actor stored in registry
  - sub_id_2 → saved in system.live_queries, actor stored in registry
  - sub_id_3 → saved in system.live_queries, actor stored in registry
    ↓
All 3 sub_ids mapped to same connection_id in actor registry
    ↓
On disconnect: All 3 sub_ids removed from registry and system.live_queries
```

### 5. **Updated Tasks (User Story 2a)**

**Previous Tasks (T080-T088)**: 9 tasks

**New Tasks (T080-T090)**: 11 tasks

**Added Tasks**:
- **T081**: Create in-memory WebSocket actor registry
- **T084**: Implement multi-subscription support per WebSocket connection
- **T087**: Add node-aware notification delivery

**Updated Tasks**:
- **T083**: Generate sub_id, register with all fields (sub_id, user_id, query, created_at, updated_at, changes=0, node)
- **T086**: Implement changes counter tracking (increment on each notification)
- **T090**: Kill live query by sub_id (lookup actor in registry, disconnect)

### 6. **Updated User Story 2a Acceptance Scenarios**

**Added Scenarios**:
1. Multiple subscriptions per connection:
   - **Given** one WebSocket with 3 subscription queries
   - **Then** 3 separate rows in system.live_queries

2. Node-aware delivery:
   - **Given** cluster with nodeA and nodeB
   - **When** user connected to nodeB
   - **Then** subscription shows node='nodeB'
   - **When** data changes in nodeA
   - **Then** only subscriptions with node='nodeA' are notified

3. Changes counter tracking:
   - **When** subscription actively streaming
   - **Then** changes counter increments
   - **And** updated_at reflects last notification

## Architecture Benefits

### 🚀 **Scalability**
- In-memory actor registry enables O(1) lookup for notification delivery
- No database queries needed to find WebSocket actor addresses
- Supports millions of concurrent subscriptions (limited only by memory)

### 🔄 **Cluster Support**
- Node field enables multi-node deployments
- Prevents duplicate notifications across cluster
- Each node only delivers to its own WebSocket connections
- system.live_queries table shared across cluster for visibility

### 📊 **Observability**
- Changes counter provides simple metric for subscription activity
- updated_at shows last notification time
- Easy to identify stale or inactive subscriptions

### 🎯 **Flexibility**
- Multiple subscriptions per connection reduces WebSocket overhead
- Clients can manage related queries in single connection
- Independent lifecycle for each subscription (can kill individual sub_ids)

## Implementation Checklist

- [x] Update system.live_queries schema in spec.md
- [x] Update Live Query Subscriptions & Change Tracking section in spec.md
- [x] Update User Story 2a acceptance scenarios in spec.md
- [x] Update T041 (Foundation) with new schema
- [x] Update User Story 2a tasks (T080-T090)
- [x] Add in-memory actor registry task (T081)
- [x] Add multi-subscription support task (T084)
- [x] Add node-aware delivery task (T087)
- [x] Update summary section in tasks.md
- [x] Document architecture in this summary file

## Next Steps

1. **Review** the updated spec.md and tasks.md
2. **Validate** the in-memory actor registry design matches Actix patterns
3. **Consider** how node discovery works in cluster (outside scope of this spec)
4. **Implement** T041 (system.live_queries schema) during Foundation phase
5. **Implement** T080-T090 during User Story 2a phase

## Related Files

- `specs/002-simple-kalamdb/spec.md` - Updated Live Query Subscriptions section
- `specs/002-simple-kalamdb/tasks.md` - Updated T041, T080-T090, and summary
- `backend/crates/kalamdb-core/src/live_query/actor_registry.rs` - New file to create
- `backend/crates/kalamdb-core/src/tables/system/live_queries.rs` - Updated schema

## Notes

- **Backward Compatibility**: This is a new feature, no backward compatibility concerns
- **Performance**: In-memory registry adds minimal memory overhead (few KB per subscription)
- **Persistence**: system.live_queries persisted in RocksDB, but actor registry is in-memory only (rebuilt on server restart from active WebSocket connections)
- **Cleanup**: Critical to remove entries from both actor registry AND system.live_queries on disconnect
