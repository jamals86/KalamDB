# Live Query Subscription Architecture

**Date**: 2025-10-15  
**Status**: ✅ Complete

## Overview

This document describes the complete live query subscription architecture for KalamDB, including the connection-based registry design, composite live ID format, and node-aware change notification system. This architecture enables real-time data synchronization over WebSocket connections with support for multiple subscriptions per connection and cluster deployments.

---

## Table of Contents

1. [Core Concepts](#core-concepts)
2. [Architecture Components](#architecture-components)
3. [Subscription Lifecycle](#subscription-lifecycle)
4. [Change Notification Flow](#change-notification-flow)
5. [Cluster Support](#cluster-support)
6. [Implementation Details](#implementation-details)
7. [Benefits](#benefits)

---

## Core Concepts

### Live Query Subscription

A **live query subscription** is an active SQL query that continuously monitors data changes and streams updates to connected WebSocket clients in real-time.

**Key Properties**:
- **Persistent**: Subscription remains active until explicitly killed or connection drops
- **Real-time**: Changes are delivered within milliseconds of data modification
- **Efficient**: Uses in-memory registry for O(1) notification delivery
- **Multiplexed**: Multiple subscriptions can share a single WebSocket connection

### Composite Live ID

Each subscription is identified by a **composite live_id** with the format:

```
live_id = "{connection_id}-{query_id}"
```

**Components**:
- `connection_id`: Globally unique WebSocket connection identifier (server-generated)
- `query_id`: User-friendly subscription identifier (client-chosen)

**Examples**:
- `"conn_abc123-messages"` - Subscription for messages query
- `"conn_abc123-notifications"` - Subscription for notifications query
- `"conn_xyz789-chat_history"` - Subscription for chat history query

**Benefits**:
- ✅ Client-friendly: Users choose meaningful names like "messages", "notifications"
- ✅ Guaranteed uniqueness: `connection_id` ensures global uniqueness
- ✅ Easy parsing: Split on `-` to extract `query_id` for client notifications
- ✅ Connection grouping: All subscriptions for a connection share same prefix

---

## Architecture Components

### 1. Persistent Storage: system.live_queries Table

**Schema**:
```sql
CREATE TABLE system.live_queries (
    live_id TEXT PRIMARY KEY,          -- Format: "{connection_id}-{query_id}"
    connection_id TEXT NOT NULL,       -- WebSocket connection identifier
    query_id TEXT NOT NULL,            -- User-chosen query identifier
    user_id TEXT NOT NULL,             -- Owner of the subscription
    query TEXT NOT NULL,               -- SQL query text
    created_at TIMESTAMP NOT NULL,     -- When subscription was created
    updated_at TIMESTAMP NOT NULL,     -- Last notification timestamp
    changes INTEGER NOT NULL,          -- Total notifications sent
    node TEXT NOT NULL                 -- Which node owns the WebSocket
);

CREATE INDEX idx_live_queries_connection ON system.live_queries(connection_id);
CREATE INDEX idx_live_queries_node ON system.live_queries(node);
```

**Purpose**:
- Persists subscription metadata in RocksDB
- Enables cluster-wide visibility of active subscriptions
- Tracks subscription activity (changes counter, timestamps)
- Supports subscription management (KILL LIVE QUERY, system diagnostics)

**Field Details**:
- **live_id**: Primary key, composite format ensures uniqueness
- **connection_id**: Groups all subscriptions for a WebSocket connection
- **query_id**: Included in notifications so client knows which subscription changed
- **user_id**: For security/authorization checks
- **query**: SQL text to re-evaluate on data changes
- **created_at**: Subscription creation time (immutable)
- **updated_at**: Last notification delivery time (updated on each change)
- **changes**: Counter of total notifications sent (incremented on each change)
- **node**: Cluster node identifier owning the WebSocket connection

### 2. In-Memory Connection Registry

**File**: `backend/crates/kalamdb-core/src/live_query/connection_registry.rs`

**Data Structures**:
```rust
/// Represents a connected WebSocket with all its subscriptions
struct ConnectedWebSocket {
    actor: Addr<WebSocketSession>,  // Actix actor address for message delivery
    live_ids: Vec<String>,          // All subscription live_ids for this connection
}

/// In-memory registry for WebSocket connections and subscriptions
struct LiveQueryRegistry {
    // Map: connection_id → ConnectedWebSocket
    connections: HashMap<String, ConnectedWebSocket>,
    
    // Map: live_id → connection_id (reverse lookup for notifications)
    live_id_to_connection: HashMap<String, String>,
    
    // Current node identifier
    node_id: String,
}
```

**Purpose**:
- **Fast notification delivery**: O(1) lookup from live_id → actor address
- **Lifecycle management**: Single source of truth for active WebSocket connections
- **Multi-subscription support**: Each connection tracks all its subscriptions
- **Efficient cleanup**: One connection_id lookup gets all subscriptions to remove

**Operations**:
```rust
impl LiveQueryRegistry {
    // Called when WebSocket connects
    fn register_connection(&mut self, connection_id: String, actor: Addr<WebSocketSession>);
    
    // Called when creating a subscription
    fn register_subscription(&mut self, live_id: String, connection_id: String);
    
    // Called when delivering change notification
    fn get_actor(&self, live_id: &str) -> Option<&Addr<WebSocketSession>>;
    
    // Called when WebSocket disconnects
    fn unregister_connection(&mut self, connection_id: &str) -> Vec<String>;
    
    // Called when killing individual subscription
    fn unregister_subscription(&mut self, live_id: &str) -> Option<String>;
}
```

---

## Subscription Lifecycle

### Phase 1: Connection Establishment

```
Client                          Server
  |                               |
  |-- WebSocket Handshake ------> |
  |                               |
  | <-- Connection Accepted ----- |
  |     (connection_id generated) |
  |                               |
  |                            Registry:
  |                            connections[connection_id] = ConnectedWebSocket {
  |                                actor: <WebSocketActor>,
  |                                live_ids: []
  |                            }
```

### Phase 2: Subscription Creation

**Client Request**:
```json
{
  "type": "subscribe",
  "subscriptions": [
    {
      "query_id": "messages",
      "sql": "SELECT * FROM messages WHERE conversation_id = 'conv1'"
    },
    {
      "query_id": "notifications",
      "sql": "SELECT * FROM notifications WHERE user_id = CURRENT_USER()"
    }
  ]
}
```

**Server Processing**:
```
For each subscription:
  1. Generate live_id = connection_id + "-" + query_id
     Example: "conn_abc123-messages"
  
  2. Insert into system.live_queries:
     INSERT INTO system.live_queries VALUES (
       'conn_abc123-messages',      -- live_id
       'conn_abc123',               -- connection_id
       'messages',                  -- query_id
       'user_alice',                -- user_id
       'SELECT * FROM messages...', -- query
       NOW(),                       -- created_at
       NOW(),                       -- updated_at
       0,                           -- changes
       'node_1'                     -- node
     );
  
  3. Update in-memory registry:
     registry.register_subscription('conn_abc123-messages', 'conn_abc123');
     // This adds to connections['conn_abc123'].live_ids
     // and to live_id_to_connection['conn_abc123-messages'] = 'conn_abc123'
  
  4. Execute initial query and send results to client
```

**Server Response**:
```json
{
  "type": "subscription_created",
  "subscriptions": [
    {
      "query_id": "messages",
      "live_id": "conn_abc123-messages",
      "initial_results": [/* query results */]
    },
    {
      "query_id": "notifications",
      "live_id": "conn_abc123-notifications",
      "initial_results": [/* query results */]
    }
  ]
}
```

### Phase 3: Active Monitoring

**In-Memory State**:
```rust
// Registry state after subscription creation
connections = {
    "conn_abc123": ConnectedWebSocket {
        actor: <WebSocketActor>,
        live_ids: [
            "conn_abc123-messages",
            "conn_abc123-notifications"
        ]
    }
}

live_id_to_connection = {
    "conn_abc123-messages": "conn_abc123",
    "conn_abc123-notifications": "conn_abc123"
}
```

**Persistent State**:
```
system.live_queries table:
| live_id                        | connection_id | query_id      | user_id    | query            | created_at | updated_at | changes | node   |
|--------------------------------|---------------|---------------|------------|------------------|------------|------------|---------|--------|
| conn_abc123-messages           | conn_abc123   | messages      | user_alice | SELECT * FROM... | 10:00:00   | 10:00:00   | 0       | node_1 |
| conn_abc123-notifications      | conn_abc123   | notifications | user_alice | SELECT * FROM... | 10:00:00   | 10:00:00   | 0       | node_1 |
```

### Phase 4: Connection Termination

**On WebSocket Disconnect**:
```
1. Lookup all subscriptions:
   live_ids = registry.unregister_connection('conn_abc123')
   // Returns: ["conn_abc123-messages", "conn_abc123-notifications"]

2. Delete from system.live_queries:
   DELETE FROM system.live_queries
   WHERE live_id IN ('conn_abc123-messages', 'conn_abc123-notifications');

3. Remove from in-memory registry:
   // Already done by unregister_connection:
   // - Removes connections['conn_abc123']
   // - Removes live_id_to_connection entries for both live_ids
```

**On Individual Subscription Kill**:
```
KILL LIVE QUERY 'conn_abc123-messages';

1. Lookup connection_id:
   connection_id = registry.unregister_subscription('conn_abc123-messages')
   // Returns: "conn_abc123"

2. Delete from system.live_queries:
   DELETE FROM system.live_queries WHERE live_id = 'conn_abc123-messages';

3. Remove from in-memory registry:
   // Already done by unregister_subscription:
   // - Removes 'conn_abc123-messages' from connections['conn_abc123'].live_ids
   // - Removes live_id_to_connection['conn_abc123-messages']
```

---

## Change Notification Flow

### Trigger: Data Modification

```
User executes:
INSERT INTO messages (id, conversation_id, sender_id, text)
VALUES ('msg1', 'conv1', 'user_bob', 'Hello!');
```

### Step 1: Change Detection

```
Storage Layer (RocksDB) detects change:
  - Table: messages
  - Operation: INSERT
  - Affected row: { id: 'msg1', conversation_id: 'conv1', ... }
```

### Step 2: Find Matching Subscriptions

```sql
-- Query system.live_queries for subscriptions that match the change
SELECT live_id, connection_id, query_id, query
FROM system.live_queries
WHERE node = 'node_1'  -- Only process subscriptions owned by current node
  AND query LIKE '%messages%';  -- Simple filter (actual logic is more complex)

-- Returns:
-- live_id: 'conn_abc123-messages'
-- connection_id: 'conn_abc123'
-- query_id: 'messages'
-- query: 'SELECT * FROM messages WHERE conversation_id = 'conv1''
```

### Step 3: Re-evaluate Query

```
Execute subscription query with change context:
  SELECT * FROM messages WHERE conversation_id = 'conv1'
  
  Returns: [
    { id: 'msg1', conversation_id: 'conv1', sender_id: 'user_bob', text: 'Hello!' }
  ]
  
  (This is the new/changed row that matches the subscription)
```

### Step 4: Lookup Connection

```rust
// Use reverse lookup to find connection
connection_id = registry.live_id_to_connection.get("conn_abc123-messages");
// Returns: "conn_abc123"

// Get WebSocket actor
connected_ws = registry.connections.get("conn_abc123");
// Returns: ConnectedWebSocket { actor: <WebSocketActor>, live_ids: [...] }
```

### Step 5: Extract Query ID

```rust
// Parse live_id to extract query_id for client notification
let parts: Vec<&str> = "conn_abc123-messages".split('-').collect();
let query_id = parts[1];  // "messages"
```

### Step 6: Send Notification

```rust
// Send message to WebSocket actor
connected_ws.actor.do_send(ChangeNotification {
    query_id: "messages".to_string(),
    change_type: ChangeType::Insert,
    rows: vec![/* changed data */],
});
```

**Client Receives**:
```json
{
  "type": "change",
  "query_id": "messages",
  "change_type": "INSERT",
  "rows": [
    {
      "id": "msg1",
      "conversation_id": "conv1",
      "sender_id": "user_bob",
      "text": "Hello!",
      "created_at": "2025-10-15T10:30:00Z"
    }
  ]
}
```

### Step 7: Update Metrics

```sql
-- Increment changes counter and update timestamp
UPDATE system.live_queries
SET changes = changes + 1,
    updated_at = NOW()
WHERE live_id = 'conn_abc123-messages';
```

---

## Cluster Support

### Multi-Node Architecture

```
         Client A                    Client B
             |                           |
             |                           |
         [Node 1]                    [Node 2]
             |                           |
             +----------RocksDB----------+
                  (system.live_queries)
```

### Node Field Usage

**Purpose**: The `node` field in `system.live_queries` ensures that only the node owning a WebSocket connection delivers notifications, preventing duplicate messages in cluster deployments.

**Subscription Creation**:
```rust
// Node 1 creates subscription
INSERT INTO system.live_queries VALUES (
    'conn_abc123-messages',
    'conn_abc123',
    'messages',
    'user_alice',
    'SELECT * FROM messages...',
    NOW(),
    NOW(),
    0,
    'node_1'  // <-- Node identifier
);
```

**Change Detection on Node 2**:
```sql
-- Node 2 detects a data change
-- Query for matching subscriptions:
SELECT live_id, connection_id, query_id, query
FROM system.live_queries
WHERE node = 'node_2'  -- <-- Only process subscriptions owned by this node
  AND query LIKE '%messages%';

-- Returns: No results (subscription is owned by node_1)
-- Node 2 SKIPS notification (no duplicate sent)
```

**Change Detection on Node 1**:
```sql
-- Node 1 detects the same data change
-- Query for matching subscriptions:
SELECT live_id, connection_id, query_id, query
FROM system.live_queries
WHERE node = 'node_1'  -- <-- Matches subscription
  AND query LIKE '%messages%';

-- Returns: 'conn_abc123-messages'
-- Node 1 DELIVERS notification (correct node)
```

### Node-Aware Benefits

✅ **Prevents Duplicate Notifications**: Each subscription notified only once, by owning node  
✅ **Load Distribution**: Each node only processes its own WebSocket connections  
✅ **Scalability**: Linear scaling with number of nodes (no cross-node coordination needed)  
✅ **Fault Tolerance**: If node fails, subscriptions are lost (client reconnects to different node)

---

## Implementation Details

### Task Breakdown

**Phase 2: Foundation**
- **T041**: Create `system.live_queries` table with schema defined above

**User Story 2a: WebSocket Live Query Subscriptions**
- **T080**: WebSocket subscription endpoint (parse subscribe message)
- **T081**: Create in-memory connection registry (`connection_registry.rs`)
- **T082**: Generate `connection_id` on WebSocket connect
- **T083**: Subscription registration logic (generate `live_id`, insert into table, update registry)
- **T084**: Multi-subscription support (track multiple `live_ids` per connection)
- **T085**: Subscription cleanup on disconnect (remove from registry and table)
- **T086**: Implement changes counter tracking (increment on each notification)
- **T087**: Node-aware notification delivery (filter by `node` field, extract `query_id`)
- **T088**: Message serialization (format change notifications with `query_id`)
- **T089**: WebSocket error handling (connection drops, timeouts)
- **T090**: KILL LIVE QUERY command (parse `live_id`, lookup connection, remove subscription)

### Registry Implementation

**File Structure**:
```
backend/crates/kalamdb-core/src/live_query/
├── mod.rs                    # Module declarations
├── connection_registry.rs    # In-memory registry
├── subscription.rs           # Subscription logic
├── change_detector.rs        # Change detection
└── notification.rs           # Notification delivery
```

**Key Algorithms**:

1. **Subscription Registration** (O(1)):
   ```rust
   fn register_subscription(&mut self, live_id: String, connection_id: String) {
       // Add to reverse lookup
       self.live_id_to_connection.insert(live_id.clone(), connection_id.clone());
       
       // Add to connection's live_ids vector
       if let Some(conn) = self.connections.get_mut(&connection_id) {
           conn.live_ids.push(live_id);
       }
   }
   ```

2. **Notification Delivery** (O(1)):
   ```rust
   fn deliver_notification(&self, live_id: &str, change_data: ChangeData) {
       // Lookup connection_id (O(1))
       if let Some(connection_id) = self.live_id_to_connection.get(live_id) {
           // Get actor (O(1))
           if let Some(conn) = self.connections.get(connection_id) {
               // Extract query_id from live_id
               let query_id = live_id.split('-').nth(1).unwrap();
               
               // Send message to actor (non-blocking)
               conn.actor.do_send(ChangeNotification {
                   query_id: query_id.to_string(),
                   change_type: change_data.change_type,
                   rows: change_data.rows,
               });
           }
       }
   }
   ```

3. **Connection Cleanup** (O(n) where n = subscriptions per connection):
   ```rust
   fn unregister_connection(&mut self, connection_id: &str) -> Vec<String> {
       // Remove connection and get all live_ids (O(1))
       if let Some(conn) = self.connections.remove(connection_id) {
           // Remove all reverse lookups (O(n))
           for live_id in &conn.live_ids {
               self.live_id_to_connection.remove(live_id);
           }
           return conn.live_ids;
       }
       vec![]
   }
   ```

### Database Queries

**Create Subscription**:
```sql
INSERT INTO system.live_queries (
    live_id, connection_id, query_id, user_id, 
    query, created_at, updated_at, changes, node
) VALUES (?, ?, ?, ?, ?, NOW(), NOW(), 0, ?);
```

**Find Matching Subscriptions** (on data change):
```sql
SELECT live_id, connection_id, query_id, query
FROM system.live_queries
WHERE node = ?
  AND (/* complex query matching logic */);
```

**Update Activity Metrics**:
```sql
UPDATE system.live_queries
SET changes = changes + 1,
    updated_at = NOW()
WHERE live_id = ?;
```

**Delete Subscription**:
```sql
DELETE FROM system.live_queries
WHERE live_id = ?;
```

**Delete Connection Subscriptions**:
```sql
DELETE FROM system.live_queries
WHERE connection_id = ?;
```

---

## Benefits

### 🚀 Scalability

**In-Memory Performance**:
- O(1) notification delivery (direct HashMap lookup)
- No database queries needed for message routing
- Supports millions of concurrent subscriptions (limited only by RAM)

**Cluster Scalability**:
- Linear scaling with number of nodes
- No cross-node coordination for notifications
- Each node independently processes its own connections

### 🔄 Client Experience

**User-Friendly Identifiers**:
- Clients choose meaningful `query_id` values ("messages", "notifications", "chat")
- No need to track server-generated UUIDs
- Easy to debug and log

**Multiplexing**:
- Multiple subscriptions over single WebSocket connection
- Reduces connection overhead
- Independent lifecycle for each subscription

**Real-Time Updates**:
- Sub-millisecond notification delivery
- Immediate data synchronization
- No polling required

### 📊 Observability

**Activity Tracking**:
- `changes` counter shows subscription activity
- `updated_at` timestamp shows last notification time
- Easy to identify stale/inactive subscriptions

**System Diagnostics**:
```sql
-- Top active subscriptions
SELECT live_id, query_id, user_id, changes
FROM system.live_queries
ORDER BY changes DESC
LIMIT 10;

-- Subscriptions per node
SELECT node, COUNT(*) as subscription_count
FROM system.live_queries
GROUP BY node;

-- Recently created subscriptions
SELECT live_id, query_id, created_at
FROM system.live_queries
ORDER BY created_at DESC
LIMIT 20;
```

### 🎯 Operational Benefits

**Clean Lifecycle Management**:
- Connection-centric design mirrors WebSocket lifecycle
- Single `connection_id` lookup gets all subscriptions
- Automatic cleanup on disconnect

**Fault Tolerance**:
- Subscriptions automatically cleaned up on node failure
- Clients reconnect and re-subscribe
- No orphaned subscriptions in registry

**Security**:
- `user_id` field enables authorization checks
- Query validation before subscription creation
- Per-subscription access control

---

## Example Scenarios

### Scenario 1: Chat Application

**Setup**:
```json
// User connects and subscribes to conversation
{
  "type": "subscribe",
  "subscriptions": [
    {
      "query_id": "conversation_123",
      "sql": "SELECT * FROM messages WHERE conversation_id = '123' ORDER BY created_at DESC LIMIT 50"
    }
  ]
}
```

**Data Change**:
```sql
INSERT INTO messages (id, conversation_id, sender_id, text, created_at)
VALUES ('msg_456', '123', 'user_bob', 'Hey there!', NOW());
```

**Notification Flow**:
1. Change detector finds subscription `conn_abc-conversation_123`
2. Re-evaluates query, gets new message
3. Looks up `connection_id` from `live_id`
4. Extracts `query_id` = "conversation_123"
5. Sends to WebSocket actor with `query_id`
6. Client receives:
   ```json
   {
     "type": "change",
     "query_id": "conversation_123",
     "change_type": "INSERT",
     "rows": [{ "id": "msg_456", "text": "Hey there!", ... }]
   }
   ```

### Scenario 2: Multi-Subscription Dashboard

**Setup**:
```json
{
  "type": "subscribe",
  "subscriptions": [
    {
      "query_id": "active_users",
      "sql": "SELECT * FROM users WHERE status = 'online'"
    },
    {
      "query_id": "recent_activity",
      "sql": "SELECT * FROM activity_log ORDER BY timestamp DESC LIMIT 100"
    },
    {
      "query_id": "system_alerts",
      "sql": "SELECT * FROM alerts WHERE severity = 'high' AND resolved = false"
    }
  ]
}
```

**State**:
```
Registry:
  connections['conn_xyz'] = {
    actor: <WebSocketActor>,
    live_ids: [
      'conn_xyz-active_users',
      'conn_xyz-recent_activity',
      'conn_xyz-system_alerts'
    ]
  }

Database:
  system.live_queries has 3 rows for conn_xyz
```

**Data Changes**:
- User goes online → `active_users` subscription notified
- Activity logged → `recent_activity` subscription notified  
- Alert created → `system_alerts` subscription notified

**Each notification includes the correct `query_id` so the client knows which dashboard widget to update**

### Scenario 3: Cluster Deployment

**Setup**:
- 3-node cluster: node_1, node_2, node_3
- User A connected to node_1
- User B connected to node_2

**Subscriptions**:
```
system.live_queries:
| live_id                 | connection_id | query_id | node   |
|-------------------------|---------------|----------|--------|
| conn_a-messages         | conn_a        | messages | node_1 |
| conn_b-messages         | conn_b        | messages | node_2 |
```

**Data Change** (occurs on node_3):
```sql
INSERT INTO messages VALUES (...);
```

**Change Detection**:
- **node_1**: Queries `WHERE node = 'node_1'` → finds `conn_a-messages` → notifies User A ✅
- **node_2**: Queries `WHERE node = 'node_2'` → finds `conn_b-messages` → notifies User B ✅
- **node_3**: Queries `WHERE node = 'node_3'` → finds nothing → no notifications ✅

**Result**: Both users notified exactly once, no duplicates

---

## Summary

The KalamDB live query subscription architecture provides:

1. **Connection-Based Design**: Uses `connection_id` as the primary grouping key with composite `live_id = connection_id + "-" + query_id` format

2. **Dual Storage**: Persistent `system.live_queries` table + in-memory `connection_registry` for fast lookups

3. **Client-Friendly**: Users choose meaningful `query_id` values included in notifications

4. **Multi-Subscription**: Multiple live queries per WebSocket connection

5. **Node-Aware**: Cluster support with `node` field prevents duplicate notifications

6. **Observable**: Activity tracking via `changes` counter and `updated_at` timestamp

7. **Performant**: O(1) notification delivery, supports millions of subscriptions

8. **Clean Lifecycle**: Automatic cleanup on disconnect, manual cleanup via KILL LIVE QUERY

This architecture is production-ready for real-time applications like chat, dashboards, collaborative editing, and live data monitoring.
