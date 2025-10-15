# Connection-Based Live Query Architecture Update

**Date**: 2025-10-15  
**Status**: ✅ Complete

## Overview

Updated live query architecture to use connection-based registry with composite `live_id` format, enabling cleaner WebSocket lifecycle management and client-friendly query identification.

---

## Architecture Changes

### Previous Design (sub_id-based)
```rust
// Old registry structure
HashMap<sub_id, Addr<WebSocketSession>>
HashMap<connection_id, Vec<sub_id>>

// Live queries table
system.live_queries {
  sub_id TEXT PRIMARY KEY,
  user_id TEXT,
  query TEXT,
  created_at TIMESTAMP,
  updated_at TIMESTAMP,
  changes INTEGER,
  node TEXT
}
```

### New Design (connection_id + query_id)
```rust
// New registry structure
struct ConnectedWebSocket {
    actor: Addr<WebSocketSession>,
    live_ids: Vec<String>,  // All subscriptions for this connection
}

HashMap<connection_id, ConnectedWebSocket>  // Main connection registry
HashMap<live_id, connection_id>             // Reverse lookup for notifications

// Live queries table
system.live_queries {
  live_id TEXT PRIMARY KEY,           // Format: "{connection_id}-{query_id}"
  connection_id TEXT,                 // WebSocket connection identifier
  query_id TEXT,                      // User-chosen query identifier
  user_id TEXT,
  query TEXT,
  created_at TIMESTAMP,
  updated_at TIMESTAMP,
  changes INTEGER,
  node TEXT
}
```

---

## Key Improvements

### 1. Composite Live ID Format
- **Format**: `live_id = connection_id + "-" + query_id`
- **Example**: `"conn_abc123-messages"`, `"conn_abc123-notifications"`
- **Benefits**:
  - Client-friendly: Users choose meaningful `query_id` values
  - Easy parsing: Split on `-` to extract `query_id` for notifications
  - Guaranteed uniqueness: `connection_id` is globally unique

### 2. Connection-Centric Registry
- **Single source of truth**: `ConnectedWebSocket` struct holds both actor and all subscriptions
- **Efficient cleanup**: One lookup by `connection_id` gets all subscriptions to delete
- **Lifecycle management**: Connect → add to registry, disconnect → remove from registry

### 3. Query ID in Notifications
- Clients receive `query_id` with each change notification
- Enables multiplexing: One WebSocket can handle multiple query streams
- Example notification:
  ```json
  {
    "query_id": "messages",
    "change_type": "INSERT",
    "rows": [...]
  }
  ```

### 4. Reverse Lookup for Notifications
- `HashMap<live_id, connection_id>` enables fast notification delivery
- On change detection:
  1. Find matching `live_id` from `system.live_queries`
  2. Lookup `connection_id` from `live_id`
  3. Get `ConnectedWebSocket` from `connection_id`
  4. Extract `query_id` from `live_id` (split on `-`)
  5. Send message to `actor` with `query_id` and change data

---

## Updated Files

### spec.md Changes

1. **Live Query Subscriptions & Change Tracking** (lines ~200-320):
   - Updated flow diagram with `connection_id` generation
   - Added `ConnectedWebSocket` struct definition
   - Added `live_id` format specification
   - Updated in-memory registry pseudocode
   - Added query_id extraction logic

2. **User Story 2a - System Architecture** (lines ~803-830):
   - Updated bullet points with new registry structure
   - Changed key format description: `{live_id}` (composite: `{connection_id}-{query_id}`)
   - Updated acceptance scenarios with `connection_id`, `query_id`, `live_id` terminology

### tasks.md Changes

1. **T041 - system.live_queries schema** (line 103):
   ```
   OLD: (sub_id, user_id, query, created_at, updated_at, changes, node)
   NEW: (live_id [PK, format: connection_id-query_id], connection_id, query_id, user_id, query, created_at, updated_at, changes, node)
   ```

2. **T081 - In-memory registry** (line 196):
   ```
   OLD: actor_registry.rs (HashMap<sub_id, Addr<WebSocketSession>> and HashMap<connection_id, Vec<sub_id>>)
   NEW: connection_registry.rs (struct ConnectedWebSocket { actor, live_ids }, HashMap<connection_id, ConnectedWebSocket>, HashMap<live_id, connection_id>)
   ```

3. **T083 - Subscription registration** (line 198):
   - Added `connection_id` generation step
   - Added `live_id = connection_id + "-" + query_id` construction
   - Added `connection_id` and `query_id` to system.live_queries insert
   - Added both HashMap updates

4. **T084 - Multi-subscription support** (line 199):
   - Clarified tracking in `ConnectedWebSocket.live_ids` vector

5. **T085 - Subscription cleanup** (line 200):
   - Updated to lookup by `connection_id`
   - Iterate over `ConnectedWebSocket.live_ids`
   - Remove from both HashMaps

6. **T087 - Node-aware delivery** (line 202):
   - Added `connection_id` lookup from `live_id`
   - Added `query_id` extraction from `live_id`
   - Added query_id to actor message

7. **T099 - KILL LIVE QUERY parser** (line 204):
   ```
   OLD: parse KILL LIVE QUERY sub_id
   NEW: parse KILL LIVE QUERY live_id
   ```

8. **T100 - Kill execution** (line 205):
   - Updated to lookup `connection_id` from `live_id`
   - Get `ConnectedWebSocket` from connection registry

9. **Summary section** (lines ~658, ~684-691):
   - Updated system.live_queries schema description
   - Added composite `live_id` format to key formats
   - Expanded "Key Additions for Live Query Architecture" with 7 detailed bullet points

---

## Implementation Checklist

- [x] Update spec.md Live Query Subscriptions section
- [x] Update spec.md User Story 2a acceptance scenarios
- [x] Update tasks.md T041 (system.live_queries schema)
- [x] Update tasks.md T081 (rename to connection_registry.rs, new structs)
- [x] Update tasks.md T083 (subscription registration with live_id)
- [x] Update tasks.md T084 (multi-subscription clarification)
- [x] Update tasks.md T085 (cleanup logic with connection_id lookup)
- [x] Update tasks.md T087 (node-aware delivery with query_id extraction)
- [x] Update tasks.md T099 (KILL LIVE QUERY with live_id)
- [x] Update tasks.md T100 (kill execution with connection lookup)
- [x] Update tasks.md summary section (schema, registry, key formats)
- [x] Create CONNECTION_BASED_LIVE_QUERY_UPDATE.md summary document

---

## Benefits Summary

1. **Cleaner Lifecycle**: Connection-centric design mirrors WebSocket lifecycle (connect → register, disconnect → cleanup all subscriptions)

2. **Client-Friendly**: Users choose meaningful `query_id` values like "messages", "notifications" instead of server-generated UUIDs

3. **Efficient Notifications**: O(1) lookup from `live_id` → `connection_id` → `ConnectedWebSocket`, include `query_id` in message

4. **Simplified Cleanup**: Single `connection_id` lookup gets all subscriptions to delete (no iteration needed)

5. **Cluster Support**: `node` field unchanged, still enables node-aware delivery in distributed deployments

6. **Observability**: `changes` counter and `updated_at` timestamp unchanged, still track subscription activity

---

## Next Steps

1. **Review**: User should review updated spec.md and tasks.md
2. **Implementation**: Begin with Phase 2 (Foundation) to create system.live_queries schema with new fields
3. **Testing**: User Story 2a acceptance scenarios cover all new functionality

All specification work is complete. Ready for implementation.
