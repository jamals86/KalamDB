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
live_id = "{connection_id}-{table_name}-{query_id}"
```

**Components**:
- `connection_id`: Format `{user_id}-{unique_connection_id}` - Globally unique WebSocket connection identifier
- `table_name`: Target table name extracted from SQL SELECT statement
- `query_id`: User-friendly subscription identifier (client-chosen)

**Examples**:
- `"user123-conn_abc-messages-updatedMessages"` - Subscription for messages query
- `"user123-conn_abc-notifications-myNotifications"` - Subscription for notifications query
- `"user456-conn_xyz-messages-chatHistory"` - Subscription for chat history query

**Benefits**:
- ✅ Client-friendly: Users choose meaningful names like "updatedMessages", "myNotifications"
- ✅ Guaranteed uniqueness: Composite format ensures global uniqueness across users
- ✅ Type-safe: LiveId struct provides direct access to connection_id, table_name, and query_id
- ✅ Efficient lookup: table_name enables O(1) filtering during change detection
- ✅ User isolation: connection_id includes user_id for multi-tenant isolation

---

## Architecture Components

### 1. Persistent Storage: system.live_queries Table

**Schema**:
```sql
CREATE TABLE system.live_queries (
    live_id TEXT PRIMARY KEY,          -- Format: "{user_id}-{unique_conn_id}-{table_name}-{query_id}"
    connection_id TEXT NOT NULL,       -- Format: "{user_id}-{unique_conn_id}"
    table_name TEXT NOT NULL,          -- Target table name from SQL SELECT
    query_id TEXT NOT NULL,            -- User-chosen query identifier
    user_id TEXT NOT NULL,             -- Owner of the subscription
    query TEXT NOT NULL,               -- SQL query text
    options TEXT,                      -- JSON: {"last_rows": 50} for initial data fetch
    created_at TIMESTAMP NOT NULL,     -- When subscription was created
    updated_at TIMESTAMP NOT NULL,     -- Last notification timestamp
    changes INTEGER NOT NULL,          -- Total notifications sent
    node TEXT NOT NULL                 -- Which node owns the WebSocket
);

CREATE INDEX idx_live_queries_connection ON system.live_queries(connection_id);
CREATE INDEX idx_live_queries_user ON system.live_queries(user_id);
CREATE INDEX idx_live_queries_table ON system.live_queries(table_name);
CREATE INDEX idx_live_queries_node ON system.live_queries(node);
```

**Purpose**:
- Persists subscription metadata in RocksDB
- Enables cluster-wide visibility of active subscriptions
- Tracks subscription activity (changes counter, timestamps)
- Supports subscription management (KILL LIVE QUERY, system diagnostics)

**Field Details**:
- **live_id**: Primary key, composite format `{user_id}-{unique_conn_id}-{table_name}-{query_id}` ensures uniqueness
- **connection_id**: Groups all subscriptions for a WebSocket connection, format `{user_id}-{unique_conn_id}`
- **table_name**: Extracted from SQL SELECT statement, enables efficient change filtering
- **query_id**: Included in notifications so client knows which subscription changed
- **user_id**: For security/authorization checks and user-based filtering
- **query**: SQL text to re-evaluate on data changes
- **options**: JSON-encoded LiveQueryOptions (e.g., `{"last_rows": 50}` for initial data fetch)
- **created_at**: Subscription creation time (immutable)
- **updated_at**: Last notification delivery time (updated on each change)
- **changes**: Counter of total notifications sent (incremented on each change)
- **node**: Cluster node identifier owning the WebSocket connection

### 2. In-Memory Connection Registry

**File**: `backend/crates/kalamdb-core/src/live_query/connection_registry.rs`

**Data Structures**:
```rust

struct LiveId {
    connection_id: ConnectionId,
    table_name: String,
    query_id: String,
}

struct ConnectionId {
    user_id: String,
    unique_conn_id: String,
}

struct NodeId(String);

struct UserId(String);

struct LiveQuery {
    live_id: LiveId,
    query: String,
    options: LiveQueryOptions,
    changes: u64,
}

struct LiveQueryOptions {
    last_rows: Option<u32>,
}

/// Represents a connected WebSocket with all its subscriptions
struct UserConnectionSocket {
    connection_id: ConnectionId,
    actor: Addr<WebSocketSession>,
    live_queries: HashMap<LiveId, LiveQuery>,
}

struct UserConnections {
    sockets: HashMap<ConnectionId, UserConnectionSocket>,
}

/// In-memory registry for WebSocket connections and subscriptions
struct LiveQueryRegistry {
    users: HashMap<UserId, UserConnections>,
    node_id: NodeId,
}
```

**Purpose**:
- **Fast notification delivery**: O(1) lookup from user_id → table_name → matching subscriptions
- **Lifecycle management**: Single source of truth for active WebSocket connections
- **Multi-subscription support**: Each connection tracks all its subscriptions
- **Efficient cleanup**: One connection_id lookup gets all subscriptions to remove
- **Type safety**: Strongly-typed LiveId, ConnectionId, NodeId, UserId prevent errors

**Operations**:
```rust
impl LiveQueryRegistry {
    // Called when WebSocket connects
    fn register_connection(&mut self, user_id: UserId, connection_id: ConnectionId, actor: Addr<WebSocketSession>);
    
    // Called when creating a subscription
    fn register_subscription(&mut self, user_id: UserId, live_query: LiveQuery);
    
    // Called when delivering change notification
    fn get_subscriptions_for_table(&self, user_id: &UserId, table_name: &str) -> Vec<&LiveQuery>;
    
    // Called when WebSocket disconnects
    fn unregister_connection(&mut self, user_id: &UserId, connection_id: &ConnectionId) -> Vec<LiveId>;
    
    // Called when killing individual subscription
    fn unregister_subscription(&mut self, live_id: &LiveId) -> Option<ConnectionId>;
}

impl LiveId {
    // Parse from string format
    fn from_string(s: &str) -> Result<Self, ParseError>;
    
    // Serialize to string format
    fn to_string(&self) -> String;
    
    // Direct access to components
    fn connection_id(&self) -> &ConnectionId;
    fn table_name(&self) -> &str;
    fn query_id(&self) -> &str;
}

impl ConnectionId {
    // Parse from string format
    fn from_string(s: &str) -> Result<Self, ParseError>;
    
    // Serialize to string format
    fn to_string(&self) -> String;
    
    // Direct access to components
    fn user_id(&self) -> &str;
    fn unique_conn_id(&self) -> &str;
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
  |     (connection_id generated: |
  |      user_id-unique_conn_id)  |
  |                               |
  |                            Registry:
  |                            users[user_id].sockets[connection_id] = UserConnectionSocket {
  |                                actor: <WebSocketActor>,
  |                                connection_id: ConnectionId { user_id, unique_conn_id },
  |                                live_queries: HashMap::new()
  |                            }
```

### Phase 2: Subscription Creation

**Client Request**:
```json
**Client Request**:
```json
{
  "type": "subscribe",
  "subscriptions": [
    {
      "query_id": "updatedMessages",
      "sql": "SELECT * FROM messages WHERE conversation_id = 'conv123'",
      "options": { "last_rows": 50 }
    },
    {
      "query_id": "myNotifications",
      "sql": "SELECT * FROM notifications WHERE user_id = CURRENT_USER()",
      "options": { "last_rows": 10 }
    }
  ]
}
```
```

**Server Processing**:
```
For each subscription:
  1. Parse SQL SELECT to extract table_name (e.g., "messages")
  
  2. Generate live_id = LiveId {
       connection_id: ConnectionId { user_id, unique_conn_id },
       table_name: "messages",
       query_id: "updatedMessages"
     }
     String format: "user123-conn_abc-messages-updatedMessages"
  
  3. Insert into system.live_queries:
     INSERT INTO system.live_queries (
       live_id, connection_id, table_name, query_id, user_id,
       query, options, created_at, updated_at, changes, node
     ) VALUES (
       'user123-conn_abc-messages-updatedMessages',
       'user123-conn_abc',
       'messages',
       'updatedMessages',
       'user123',
       'SELECT * FROM messages WHERE conversation_id = ''conv123''',
       '{"last_rows": 50}',
       NOW(),
       NOW(),
       0,
       'node_1'
     );
  
  4. Add to in-memory registry:
     registry.users[user_id].sockets[connection_id].live_queries[live_id] = LiveQuery {
       live_id,
       query: "SELECT * FROM messages WHERE conversation_id = 'conv123'",
       options: LiveQueryOptions { last_rows: Some(50) },
       changes: 0
     }
  
  5. If options.last_rows > 0:
     - Execute query with LIMIT last_rows
     - Send initial data to client
```

**Server Response**:
```json
{
  "type": "subscription_created",
  "subscriptions": [
    {
      "query_id": "updatedMessages",
      "status": "active",
      "initial_data": [
        { "id": "msg1", "text": "Hello", "conversation_id": "conv123" },
        { "id": "msg2", "text": "World", "conversation_id": "conv123" }
      ]
    },
    {
      "query_id": "myNotifications",
      "status": "active",
      "initial_data": [
        { "id": "notif1", "text": "New message", "user_id": "user123" }
      ]
    }
  ]
}
```

### Phase 3: Active Monitoring
```
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
users = {
    UserId("user123"): UserConnections {
        sockets: {
            ConnectionId { user_id: "user123", unique_conn_id: "conn_abc" }: UserConnectionSocket {
                connection_id: ConnectionId { user_id: "user123", unique_conn_id: "conn_abc" },
                actor: Addr<WebSocketSession>,
                live_queries: {
                    LiveId { connection_id, table_name: "messages", query_id: "updatedMessages" }: LiveQuery { ... },
                    LiveId { connection_id, table_name: "notifications", query_id: "myNotifications" }: LiveQuery { ... }
                }
            }
        }
    }
}
```

**Persistent State**:
```
system.live_queries table:
| live_id                                      | connection_id        | table_name      | query_id           | user_id    | query            | options           | created_at | updated_at | changes | node   |
|----------------------------------------------|----------------------|---------------|--------------------|------------|------------------|-------------------|------------|------------|---------|--------|
| user123-conn_abc-messages-updatedMessages    | user123-conn_abc     | messages      | updatedMessages    | user123    | SELECT * FROM... | {"last_rows":50}  | 10:00:00   | 10:00:00   | 0       | node_1 |
| user123-conn_abc-notifications-myNotifications| user123-conn_abc    | notifications | myNotifications    | user123    | SELECT * FROM... | {"last_rows":10}  | 10:00:00   | 10:00:00   | 0       | node_1 |
```

### Phase 4: Connection Termination

**On WebSocket Disconnect**:
```
1. Lookup user's connections:
   user_connections = registry.users.get(user_id)
   socket = user_connections.sockets.get(connection_id)
   
2. Collect all live_ids:
   live_ids = socket.live_queries.keys()
   // Returns: [LiveId { ... }, LiveId { ... }]

3. Delete from system.live_queries:
   DELETE FROM system.live_queries
   WHERE connection_id = 'user123-conn_abc';

4. Remove from in-memory registry:
   registry.users[user_id].sockets.remove(connection_id)
   // This automatically removes all live_queries in that socket
```

**On Individual Subscription Kill**:
```
KILL LIVE QUERY 'user123-conn_abc-messages-updatedMessages';

1. Parse live_id:
   live_id = LiveId::from_string('user123-conn_abc-messages-updatedMessages')
   // Returns: LiveId { 
   //   connection_id: ConnectionId { user_id: "user123", unique_conn_id: "conn_abc" },
   //   table_name: "messages",
   //   query_id: "updatedMessages"
   // }

2. Delete from system.live_queries:
   DELETE FROM system.live_queries 
   WHERE live_id = 'user123-conn_abc-messages-updatedMessages';

3. Remove from in-memory registry:
   user_id = live_id.connection_id().user_id()
   connection_id = live_id.connection_id()
   registry.users[user_id].sockets[connection_id].live_queries.remove(live_id)
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
  - Column Family: user_table:production:messages
  - Key format: {user_id}:{row_id}
  - Key: "user123:msg1"
  - Affected row: { id: 'msg1', conversation_id: 'conv1', text: 'Hello!', ... }
  
Extract user_id from key:
  - user_id = "user123"
```

### Step 2: Find Matching Subscriptions

```rust
// Check if user has any connections
if let Some(user_connections) = registry.users.get(&UserId("user123")) {
    // Extract table_name from column family name
    let table_name = "messages";
    
    // Loop over all sockets for this user
    for (connection_id, socket) in user_connections.sockets.iter() {
        // Loop over all live queries for this socket
        for (live_id, live_query) in socket.live_queries.iter() {
            // Check if this subscription matches the changed table
            if live_id.table_name() == table_name {
                // This subscription needs to be evaluated
                // live_id = LiveId { 
                //   connection_id: "user123-conn_abc",
                //   table_name: "messages",
                //   query_id: "updatedMessages"
                // }
                // live_query.query = "SELECT * FROM messages WHERE conversation_id = 'conv1'"
            }
        }
    }
}
```

### Step 3: Re-evaluate Query

```
Execute subscription query with change context:
  SELECT * FROM messages WHERE conversation_id = 'conv1'
  
  Returns: [
    { id: 'msg1', conversation_id: 'conv1', sender_id: 'user_bob', text: 'Hello!' }
  ]
  
  (This is the new/changed row that matches the subscription filter)
```

### Step 4: Send Notification

```rust
// We already have direct access from Step 2 loop:
// - socket.actor: Addr<WebSocketSession>
// - live_id.query_id(): &str (e.g., "updatedMessages")

// Send message to WebSocket actor
socket.actor.do_send(ChangeNotification {
    query_id: live_id.query_id().to_string(),  // "updatedMessages"
    change_type: ChangeType::Insert,
    rows: vec![/* changed data */],
});
```

**Client Receives**:
```json
{
  "type": "change",
  "query_id": "updatedMessages",
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

### Step 5: Update Metrics
    }
  ]
}
```

### Step 5: Update Metrics

```sql
-- Increment changes counter and update timestamp
UPDATE system.live_queries
SET changes = changes + 1,
    updated_at = NOW()
WHERE live_id = 'user123-conn_abc-messages-updatedMessages';
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
    'user123-conn_abc-messages-updatedMessages',  -- live_id
    'user123-conn_abc',                            -- connection_id
    'messages',                                     -- table_name
    'updatedMessages',                              -- query_id
    'user123',                                      -- user_id
    'SELECT * FROM messages...',                    -- query
    '{"last_rows": 50}',                            -- options (JSON)
    NOW(),                                          -- created_at
    NOW(),                                          -- updated_at
    0,                                              -- changes
    'node_1'                                        -- node
);
```

**Change Detection on Node 2**:
```rust
// Node 2 detects a data change in user123's messages table
// Extract user_id from RocksDB key: "user123:msg1"
let user_id = "user123";

// Check in-memory registry (Node 2's registry)
if let Some(user_connections) = registry.users.get(&UserId(user_id)) {
    // Returns: None (user123 has no connections on node_2)
}

// Node 2 SKIPS notification (user not connected to this node)
```

**Change Detection on Node 1**:
```rust
// Node 1 detects the same data change
// Extract user_id from RocksDB key: "user123:msg1"
let user_id = "user123";

// Check in-memory registry (Node 1's registry)
if let Some(user_connections) = registry.users.get(&UserId(user_id)) {
    // Returns: Some(UserConnections { sockets: { ... } })
    
    // Extract table_name from column family
    let table_name = "messages";
    
    // Find matching subscriptions (as described in Step 2)
    for (connection_id, socket) in user_connections.sockets.iter() {
        for (live_id, live_query) in socket.live_queries.iter() {
            if live_id.table_name() == table_name {
                // MATCH! Deliver notification
            }
        }
    }
}

// Node 1 DELIVERS notification (user connected to this node)
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
   fn register_subscription(&mut self, user_id: UserId, connection_id: ConnectionId, live_query: LiveQuery) {
       // Get or create user's connections
       let user_conns = self.users.entry(user_id).or_insert_with(|| UserConnections {
           sockets: HashMap::new()
       });
       
       // Add live_query to socket
       if let Some(socket) = user_conns.sockets.get_mut(&connection_id) {
           socket.live_queries.insert(live_query.live_id.clone(), live_query);
       }
   }
   ```

2. **Notification Delivery** (O(k) where k = subscriptions for table):
   ```rust
   fn deliver_notifications(&self, user_id: &UserId, table_name: &str, change_data: ChangeData) {
       // Lookup user's connections (O(1))
       if let Some(user_conns) = self.users.get(user_id) {
           // Iterate over all sockets for this user (typically 1-2)
           for (_, socket) in user_conns.sockets.iter() {
               // Iterate over all live queries in socket
               for (live_id, live_query) in socket.live_queries.iter() {
                   // Check if table matches (O(1) string comparison)
                   if live_id.table_name() == table_name {
                       // Re-evaluate query filter
                       if change_matches_query(&change_data, &live_query.query) {
                           // Send notification to actor
                           socket.actor.do_send(ChangeNotification {
                               query_id: live_id.query_id().to_string(),
                               change_type: change_data.change_type,
                               rows: change_data.rows.clone(),
                           });
                       }
                   }
               }
           }
       }
   }
   ```

3. **Connection Cleanup** (O(m) where m = total subscriptions for connection):
   ```rust
   fn unregister_connection(&mut self, user_id: &UserId, connection_id: &ConnectionId) -> Vec<LiveId> {
       let mut removed_live_ids = Vec::new();
       
       if let Some(user_conns) = self.users.get_mut(user_id) {
           // Remove socket and get all live_ids
           if let Some(socket) = user_conns.sockets.remove(connection_id) {
               removed_live_ids = socket.live_queries.keys().cloned().collect();
           }
           
           // If user has no more sockets, remove user entry
           if user_conns.sockets.is_empty() {
               self.users.remove(user_id);
           }
       }
       
       removed_live_ids
   }
   ```

### Database Queries

**Create Subscription**:
```sql
INSERT INTO system.live_queries (
    live_id, connection_id, table_name, query_id, user_id, 
    query, options, created_at, updated_at, changes, node
) VALUES (?, ?, ?, ?, ?, ?, ?, NOW(), NOW(), 0, ?);
```

**Find Matching Subscriptions** (on data change):
```rust
// No database query needed - use in-memory registry
// 1. Extract user_id from RocksDB key
// 2. Lookup user in registry: registry.users.get(&user_id)
// 3. Filter subscriptions by table_name
// 4. Re-evaluate query filter for each match
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

1. **User-Centric Design**: Registry organized by `user_id` → `connection_id` → `live_id` hierarchy with strongly-typed structs (LiveId, ConnectionId, NodeId, UserId)

2. **Type-Safe LiveId**: Composite format `{user_id}-{unique_conn_id}-{table_name}-{query_id}` with struct-based access to all components without parsing

3. **Dual Storage**: Persistent `system.live_queries` table in RocksDB + in-memory `LiveQueryRegistry` for O(1) user lookup

4. **Client-Friendly**: Users choose meaningful `query_id` values included in notifications

5. **Efficient Change Detection**: Extract `user_id` from RocksDB key → lookup in registry → filter by `table_name` → evaluate query

6. **Multi-Subscription**: Multiple live queries per WebSocket connection, each with unique `query_id`

7. **Node-Aware**: In-memory registry ensures only the node owning a WebSocket delivers notifications (no cross-node coordination)

8. **Observable**: Activity tracking via `changes` counter and `updated_at` timestamp in system.live_queries

9. **Performant**: O(1) user lookup, O(k) notification delivery where k = subscriptions for that user's table

10. **Clean Lifecycle**: Automatic cleanup on disconnect via ConnectionId, manual cleanup via KILL LIVE QUERY with LiveId parsing

This architecture is production-ready for real-time applications like chat, dashboards, collaborative editing, and live data monitoring with millions of concurrent users.

---

## Complete Flow Recap

**Subscription Creation Flow**:
1. Client sends WebSocket connection request
2. Server generates `ConnectionId { user_id, unique_conn_id }`
3. Server creates WebSocket actor and registers in `LiveQueryRegistry.users[user_id].sockets[connection_id]`
4. Client sends subscription array with `query_id` and SQL SELECT
5. For each subscription:
   - Parse SQL to extract `table_name`
   - Generate `LiveId { connection_id, table_name, query_id }`
   - Insert into `system.live_queries` with live_id string format
   - Add to in-memory `socket.live_queries[live_id] = LiveQuery { live_id, query, options, changes }`
   - If `options.last_rows > 0`, execute query and send initial data

**Change Detection & Notification Flow**:
1. INSERT/UPDATE/DELETE to user table writes to RocksDB with key `{user_id}:{row_id}`
2. Extract `user_id` from key
3. Check `registry.users.get(&user_id)` - if None, skip (user not connected to this node)
4. Extract `table_name` from column family name (e.g., `user_table:production:messages` → "messages")
5. Loop over `user_connections.sockets`:
   - For each `socket`, loop over `socket.live_queries`
   - If `live_id.table_name() == table_name`:
     - Re-evaluate `live_query.query` with change data
     - If matches filter, send to `socket.actor` with `live_id.query_id()`
6. Update `system.live_queries` metrics: increment `changes`, set `updated_at`

**Disconnection Cleanup Flow**:
1. WebSocket disconnects
2. Extract `user_id` and `connection_id`
3. Lookup `registry.users[user_id].sockets[connection_id]`
4. Collect all `live_ids` from `socket.live_queries.keys()`
5. Delete from `system.live_queries WHERE connection_id = ?`
6. Remove from in-memory: `user_connections.sockets.remove(connection_id)`
7. If `user_connections.sockets.is_empty()`, remove `registry.users[user_id]`

This design ensures **no parsing overhead** during hot path (change detection), **type safety** throughout, and **O(1) user isolation** for massive scalability.
