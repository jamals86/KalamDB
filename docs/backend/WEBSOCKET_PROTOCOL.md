# KalamDB WebSocket Protocol

**Version**: 0.1.0  
**Endpoint**: `ws://localhost:8080/ws`

## Overview

KalamDB provides real-time data streaming via WebSocket connections. Clients can subscribe to live queries and receive immediate notifications when data changes (INSERT, UPDATE, DELETE) or when data is flushed to cold storage.

---

## Connection Flow

```
1. Client connects to ws://localhost:8080/ws
2. Client sends subscription message with SQL queries
3. Server returns initial data (if last_rows option specified)
4. Server sends real-time notifications for matching changes
5. Client can unsubscribe or disconnect
```

---

## Authentication

*Current*: User identification via initial message
```json
{
  "user_id": "alice"
}
```

*Future (Phase 17)*: JWT token in URL or initial handshake
```
ws://localhost:8080/ws?token=<JWT_TOKEN>
```

---

## Message Types

### 1. Subscribe (Client → Server)

Subscribe to one or more live queries.

**Message**:
```json
{
  "type": "subscribe",
  "subscriptions": [
    {
      "query_id": "messages_recent",
      "sql": "SELECT * FROM app.messages WHERE timestamp > NOW() - INTERVAL '1 hour'",
      "options": {
        "last_rows": 10
      }
    },
    {
      "query_id": "events_live",
      "sql": "SELECT * FROM app.events",
      "options": {}
    }
  ]
}
```

**Fields**:
- `type`: Always `"subscribe"`
- `subscriptions`: Array of subscription requests
  - `query_id`: Unique identifier for this subscription (client-defined)
  - `sql`: SELECT query to monitor for changes
  - `options`: Optional configuration
    - `last_rows` (integer): Fetch N most recent rows before starting real-time notifications

**Server Response (Initial Data)**:
```json
{
  "type": "initial_data",
  "query_id": "messages_recent",
  "data": [
    {
      "id": 100,
      "content": "Recent message",
      "timestamp": "2025-10-20T15:00:00Z",
      "_updated": "2025-10-20T15:00:00Z",
      "_deleted": false
    }
  ],
  "rows_returned": 10
}
```

**Server Response (Subscription Confirmed)**:
```json
{
  "type": "subscribed",
  "query_id": "messages_recent",
  "message": "Successfully subscribed to query"
}
```

---

### 2. Unsubscribe (Client → Server)

Cancel an active subscription.

**Message**:
```json
{
  "type": "unsubscribe",
  "query_ids": ["messages_recent", "events_live"]
}
```

**Server Response**:
```json
{
  "type": "unsubscribed",
  "query_ids": ["messages_recent", "events_live"],
  "message": "Successfully unsubscribed from 2 queries"
}
```

---

### 3. Change Notification (Server → Client)

Sent when a change matching a subscribed query occurs.

#### INSERT Notification

```json
{
  "type": "notification",
  "query_id": "messages_recent",
  "change_type": "INSERT",
  "table_name": "messages",
  "data": {
    "id": 101,
    "content": "New message",
    "timestamp": "2025-10-20T15:30:00Z",
    "_updated": "2025-10-20T15:30:00Z",
    "_deleted": false
  },
  "timestamp": "2025-10-20T15:30:00.123Z"
}
```

**Fields**:
- `type`: Always `"notification"`
- `query_id`: Identifier of the subscription that matched
- `change_type`: `"INSERT"`, `"UPDATE"`, `"DELETE"`, or `"FLUSH"`
- `table_name`: Name of the table that changed
- `data`: New row data
- `timestamp`: When the change occurred (server time)

#### UPDATE Notification

```json
{
  "type": "notification",
  "query_id": "messages_recent",
  "change_type": "UPDATE",
  "table_name": "messages",
  "old_data": {
    "id": 100,
    "content": "Original message",
    "timestamp": "2025-10-20T14:00:00Z",
    "_updated": "2025-10-20T14:00:00Z",
    "_deleted": false
  },
  "data": {
    "id": 100,
    "content": "Updated message",
    "timestamp": "2025-10-20T14:00:00Z",
    "_updated": "2025-10-20T15:30:00Z",
    "_deleted": false
  },
  "timestamp": "2025-10-20T15:30:00.456Z"
}
```

**Note**: UPDATE notifications include both `old_data` (before) and `data` (after).

#### DELETE Notification (Soft Delete)

```json
{
  "type": "notification",
  "query_id": "messages_recent",
  "change_type": "DELETE",
  "table_name": "messages",
  "data": {
    "id": 100,
    "content": "Deleted message",
    "timestamp": "2025-10-20T14:00:00Z",
    "_updated": "2025-10-20T15:30:00Z",
    "_deleted": true
  },
  "timestamp": "2025-10-20T15:30:00.789Z"
}
```

**Note**: Soft deletes set `_deleted = true`. Row remains in storage for retention period.

#### DELETE Notification (Hard Delete)

```json
{
  "type": "notification",
  "query_id": "messages_recent",
  "change_type": "DELETE",
  "table_name": "messages",
  "row_id": "100",
  "data": null,
  "timestamp": "2025-10-20T15:30:00.789Z"
}
```

**Note**: Hard deletes send only `row_id`. `data` field is `null`.

#### FLUSH Notification

```json
{
  "type": "notification",
  "query_id": "messages_recent",
  "change_type": "FLUSH",
  "table_name": "messages",
  "data": {
    "rows_flushed": 1000,
    "parquet_file": "user/alice/messages/batch-20251020-001.parquet",
    "file_size_bytes": 524288
  },
  "timestamp": "2025-10-20T15:35:00.000Z"
}
```

**Note**: FLUSH notifications indicate data moved from hot (RocksDB) to cold (Parquet) storage.

---

### 4. Error (Server → Client)

Sent when an error occurs processing a client request.

**Message**:
```json
{
  "type": "error",
  "query_id": "invalid_query",
  "error": "Table not found: app.nonexistent",
  "error_type": "TableNotFound"
}
```

**Fields**:
- `type`: Always `"error"`
- `query_id`: Identifier of the subscription that failed (if applicable)
- `error`: Human-readable error message
- `error_type`: Error category (see Error Types section)

---

### 5. Ping/Pong (Keepalive)

**Client → Server (Ping)**:
```json
{
  "type": "ping"
}
```

**Server → Client (Pong)**:
```json
{
  "type": "pong",
  "timestamp": "2025-10-20T15:40:00.123Z"
}
```

**Note**: Clients should send pings every 30 seconds to keep connections alive.

---

## Subscription Filters

Subscriptions support SQL WHERE clauses to filter notifications.

### Example: Filter by Timestamp

```json
{
  "query_id": "recent_only",
  "sql": "SELECT * FROM app.messages WHERE timestamp > NOW() - INTERVAL '5 minutes'"
}
```

**Result**: Only rows with `timestamp` in the last 5 minutes trigger notifications.

### Example: Filter by Author

```json
{
  "query_id": "alice_messages",
  "sql": "SELECT * FROM app.messages WHERE author = 'alice'"
}
```

**Result**: Only rows where `author = 'alice'` trigger notifications.

### Example: Complex Filter

```json
{
  "query_id": "urgent_messages",
  "sql": "SELECT * FROM app.messages WHERE priority > 5 AND status = 'active'"
}
```

**Result**: Only rows matching both conditions trigger notifications.

---

## User Isolation

**For User Tables**:
- Users automatically receive only their own data changes
- System enforces `user_id` filtering at storage layer
- No need to specify user_id in SQL WHERE clause

**For Shared Tables**:
- All users receive all changes
- Use WHERE clauses to filter if needed

**For Stream Tables**:
- All users receive all changes
- Stream tables are global by design

---

## Initial Data Fetch

Use the `last_rows` option to fetch recent data before receiving real-time notifications.

**Example**:
```json
{
  "query_id": "messages_with_history",
  "sql": "SELECT * FROM app.messages",
  "options": {
    "last_rows": 50
  }
}
```

**Server Response**:
1. **Initial Data** (50 most recent rows):
   ```json
   {
     "type": "initial_data",
     "query_id": "messages_with_history",
     "data": [...50 rows...],
     "rows_returned": 50
   }
   ```

2. **Subscription Confirmed**:
   ```json
   {
     "type": "subscribed",
     "query_id": "messages_with_history"
   }
   ```

3. **Real-time Notifications** (as data changes):
   ```json
   {
     "type": "notification",
     "query_id": "messages_with_history",
     "change_type": "INSERT",
     ...
   }
   ```

---

## Error Types

| Error Type | Description |
|-----------|-------------|
| `TableNotFound` | Table does not exist |
| `InvalidSql` | SQL syntax error in subscription query |
| `InvalidOperation` | Operation not allowed (e.g., non-SELECT query) |
| `PermissionDenied` | User lacks permission to subscribe |
| `SubscriptionLimitExceeded` | Too many active subscriptions (rate limit) |
| `ConnectionClosed` | WebSocket connection closed unexpectedly |

---

## Rate Limiting

*Coming in Phase 17*

Per-user limits:
- Max subscriptions per user: 100
- Max messages per second: 1000
- Max message size: 1 MB

Per-connection limits:
- Max subscriptions per connection: 50
- Idle timeout: 5 minutes (send pings to keep alive)

**Error Response (Rate Limit Exceeded)**:
```json
{
  "type": "error",
  "error": "Subscription limit exceeded. Max 100 subscriptions per user.",
  "error_type": "SubscriptionLimitExceeded"
}
```

---

## Reconnection Strategy

If the WebSocket connection drops, clients should:

1. **Wait before reconnecting**: Exponential backoff (1s, 2s, 4s, 8s, max 30s)
2. **Resubscribe with `last_rows`**: Fetch missed data during downtime
3. **Deduplicate data**: Use `_updated` timestamp to avoid duplicates

**Example Reconnection**:
```javascript
let retryDelay = 1000; // Start with 1 second

function connect() {
  const ws = new WebSocket('ws://localhost:8080/ws');
  
  ws.onopen = () => {
    console.log('Connected');
    retryDelay = 1000; // Reset retry delay
    
    // Resubscribe with recent data
    ws.send(JSON.stringify({
      type: 'subscribe',
      subscriptions: [{
        query_id: 'messages',
        sql: 'SELECT * FROM app.messages',
        options: { last_rows: 100 }
      }]
    }));
  };
  
  ws.onclose = () => {
    console.log('Disconnected. Reconnecting in', retryDelay, 'ms');
    setTimeout(connect, retryDelay);
    retryDelay = Math.min(retryDelay * 2, 30000); // Max 30s
  };
  
  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    handleMessage(msg);
  };
}

connect();
```

---

## Example Client Implementation

### JavaScript/TypeScript

```javascript
class KalamDBClient {
  constructor(url, userId) {
    this.url = url;
    this.userId = userId;
    this.ws = null;
    this.subscriptions = new Map();
  }

  connect() {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.url);
      
      this.ws.onopen = () => {
        console.log('WebSocket connected');
        resolve();
      };
      
      this.ws.onerror = (error) => {
        console.error('WebSocket error:', error);
        reject(error);
      };
      
      this.ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);
        this.handleMessage(msg);
      };
      
      this.ws.onclose = () => {
        console.log('WebSocket closed');
        // Implement reconnection logic here
      };
    });
  }

  subscribe(queryId, sql, options = {}) {
    const subscription = { query_id: queryId, sql, options };
    
    this.ws.send(JSON.stringify({
      type: 'subscribe',
      subscriptions: [subscription]
    }));
    
    return new Promise((resolve) => {
      this.subscriptions.set(queryId, { resolve, handlers: [] });
    });
  }

  onNotification(queryId, handler) {
    const sub = this.subscriptions.get(queryId);
    if (sub) {
      sub.handlers.push(handler);
    }
  }

  handleMessage(msg) {
    switch (msg.type) {
      case 'initial_data':
        console.log('Initial data for', msg.query_id, ':', msg.data);
        break;
        
      case 'subscribed':
        const sub = this.subscriptions.get(msg.query_id);
        if (sub) sub.resolve();
        break;
        
      case 'notification':
        const handlers = this.subscriptions.get(msg.query_id)?.handlers || [];
        handlers.forEach(handler => handler(msg));
        break;
        
      case 'error':
        console.error('Subscription error:', msg.error);
        break;
        
      case 'pong':
        // Keepalive response
        break;
    }
  }

  ping() {
    this.ws.send(JSON.stringify({ type: 'ping' }));
  }

  unsubscribe(queryIds) {
    this.ws.send(JSON.stringify({
      type: 'unsubscribe',
      query_ids: queryIds
    }));
    
    queryIds.forEach(id => this.subscriptions.delete(id));
  }

  disconnect() {
    this.ws.close();
  }
}

// Usage example
const client = new KalamDBClient('ws://localhost:8080/ws', 'alice');

await client.connect();

await client.subscribe('messages', 'SELECT * FROM app.messages', { last_rows: 10 });

client.onNotification('messages', (notification) => {
  console.log('Change detected:', notification.change_type, notification.data);
});

// Send keepalive pings every 30 seconds
setInterval(() => client.ping(), 30000);
```

---

## Performance Considerations

1. **Batch Notifications**: Server may batch notifications within a short time window (< 100ms) to reduce message overhead.

2. **Backpressure**: If client cannot keep up with notifications, server may:
   - Buffer up to 1000 notifications per connection
   - Close connection if buffer exceeds limit
   - Send `slow_consumer` warning before closing

3. **Filter Optimization**: Server compiles and caches WHERE clause filters for performance. Reusing `query_id` values across reconnections improves efficiency.

---

## See Also

- [REST API Reference](API_REFERENCE.md) - SQL command documentation
- [SQL Syntax Reference](SQL_SYNTAX.md) - Complete SQL command list
- [Quick Start Guide](../QUICK_START.md) - Getting started tutorial
