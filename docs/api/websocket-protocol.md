# KalamDB WebSocket Protocol

**Version**: 0.1.3  
**Endpoint**: `ws://localhost:8080/v1/ws`

## Overview

KalamDB provides real-time data streaming via WebSocket connections. Clients can subscribe to live queries and receive immediate notifications when data changes (INSERT, UPDATE, DELETE).

---

## Connection Flow

```
1. Client connects to ws://localhost:8080/v1/ws
2. Client must authenticate (default timeout: 3 seconds)
3. Client subscribes to a live query (SELECT)
4. Server sends SubscriptionAck and InitialDataBatch messages
5. Client requests more batches via NextBatch until ready
6. Server sends change notifications after initial load completes
```

---

## Authentication

WebSocket connections are established unauthenticated, then the client must send an
`authenticate` message immediately after connecting.

**Basic auth**:
```json
{"type": "authenticate", "method": "basic", "username": "alice", "password": "secret"}
```

**JWT auth**:
```json
{"type": "authenticate", "method": "jwt", "token": "<JWT_TOKEN>"}
```

Server replies with either:

```json
{"type": "auth_success", "user_id": "alice", "role": "dba"}
```

or:

```json
{"type": "auth_error", "message": "Invalid username or password"}
```

---

## Message Types

### 1. Subscribe (Client → Server)

Subscribe to a live query.

**Message**:
```json
{
  "type": "subscribe",
  "subscription": {
    "id": "messages_recent",
    "sql": "SELECT * FROM app.messages WHERE timestamp > NOW() - INTERVAL '1 hour'",
    "options": {"last_rows": 10}
  }
}
```

**Fields**:
- `subscription.id`: Client-generated subscription identifier
- `subscription.sql`: A SELECT statement to monitor for changes
- `subscription.options.last_rows` (optional): Fetch N most recent rows for initial load
- `subscription.options.batch_size` (optional): Hint for server batch sizing
- `subscription.options.from_seq_id` (optional): Resume from a sequence id

**Server Response (Acknowledgement)**:
```json
{
  "type": "subscription_ack",
  "subscription_id": "messages_recent",
  "total_rows": 0,
  "batch_control": {"batch_num": 0, "total_batches": null, "has_more": true, "status": "loading", "last_seq_id": null, "snapshot_end_seq": null}
}
```

**Server Response (Initial Data Batch)**:
```json
{
  "type": "initial_data_batch",
  "subscription_id": "messages_recent",
  "rows": [{"id": 100, "content": "Recent message"}],
  "batch_control": {"batch_num": 0, "total_batches": null, "has_more": true, "status": "loading", "last_seq_id": 100, "snapshot_end_seq": 123456}
}
```

If `batch_control.has_more` is `true`, request the next batch.

**Message (Next Batch Request)**:
```json
{
  "type": "next_batch",
  "subscription_id": "messages_recent",
  "last_seq_id": 100
}
```

---

### 2. Unsubscribe (Client → Server)

Cancel an active subscription.

**Message**:
```json
{
  "type": "unsubscribe",
  "subscription_id": "messages_recent"
}
```

The server stops sending updates for that subscription.

---

### 3. Change Notification (Server → Client)

Sent when a change matching a subscribed query occurs.

#### INSERT Notification

```json
{
  "type": "change",
  "subscription_id": "messages_recent",
  "change_type": "insert",
  "rows": [
    {
      "id": 101,
      "content": "New message",
      "timestamp": "2025-10-20T15:30:00Z"
    }
  ]
}
```

**Fields**:
- `type`: Always `"change"`
- `subscription_id`: Identifier of the subscription that matched
- `change_type`: `"insert"`, `"update"`, or `"delete"`
- `rows`: New/current row values (for INSERT and UPDATE)
- `old_values`: Previous row values (for UPDATE and DELETE)

#### UPDATE Notification

```json
{
  "type": "change",
  "subscription_id": "messages_recent",
  "change_type": "update",
  "rows": [
    {
      "id": 100,
      "content": "Updated message",
      "timestamp": "2025-10-20T14:00:00Z"
    }
  ],
  "old_values": [
    {
      "id": 100,
      "content": "Original message",
      "timestamp": "2025-10-20T14:00:00Z"
    }
  ]
}
```

**Note**: UPDATE notifications include both `old_values` (before) and `rows` (after).

#### DELETE Notification (Soft Delete)

```json
{
  "type": "change",
  "subscription_id": "messages_recent",
  "change_type": "delete",
  "old_values": [
    {
      "id": 100,
      "content": "Deleted message",
      "timestamp": "2025-10-20T14:00:00Z"
    }
  ]
}
```

### 4. Error (Server → Client)

Sent when an error occurs processing a client request.

**Message**:
```json
{
  "type": "error",
  "subscription_id": "messages_recent",
  "code": "INVALID_SQL",
  "message": "Table not found: app.nonexistent"
}
```

**Fields**:
- `type`: Always `"error"`
- `subscription_id`: Identifier of the subscription related to the error
- `code`: Stable error code string (examples below)
- `message`: Human-readable error message

---

### 5. Ping/Pong (Keepalive)

KalamDB uses standard WebSocket ping/pong control frames for keepalive. Clients do not send JSON `ping`/`pong` messages.

---

## Subscription Filters

Subscriptions support SQL WHERE clauses to filter notifications.

### Example: Filter by Timestamp

```json
{
  "type": "subscribe",
  "subscription": {
    "id": "recent_only",
    "sql": "SELECT * FROM app.messages WHERE timestamp > NOW() - INTERVAL '5 minutes'",
    "options": {}
  }
}
```

**Result**: Only rows with `timestamp` in the last 5 minutes trigger notifications.

### Example: Filter by Author

```json
{
  "type": "subscribe",
  "subscription": {
    "id": "alice_messages",
    "sql": "SELECT * FROM app.messages WHERE author = 'alice'",
    "options": {}
  }
}
```

**Result**: Only rows where `author = 'alice'` trigger notifications.

### Example: Complex Filter

```json
{
  "type": "subscribe",
  "subscription": {
    "id": "urgent_messages",
    "sql": "SELECT * FROM app.messages WHERE priority > 5 AND status = 'active'",
    "options": {}
  }
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
  "type": "subscribe",
  "subscription": {
    "id": "messages_with_history",
    "sql": "SELECT * FROM app.messages",
    "options": {"last_rows": 50}
  }
}
```

**Server Response**:
1. **SubscriptionAck**:
   ```json
   {
     "type": "subscription_ack",
     "subscription_id": "messages_with_history",
     "total_rows": 0,
     "batch_control": {"batch_num": 0, "total_batches": null, "has_more": true, "status": "loading", "last_seq_id": null, "snapshot_end_seq": null}
   }
   ```

2. **InitialDataBatch** (first batch):
   ```json
   {
     "type": "initial_data_batch",
     "subscription_id": "messages_with_history",
     "rows": [{"id": 1, "content": "..."}],
     "batch_control": {"batch_num": 0, "total_batches": null, "has_more": true, "status": "loading", "last_seq_id": 100, "snapshot_end_seq": 123456}
   }
   ```

3. **Real-time change notifications** (after `batch_control.status` becomes `ready`):
   ```json
   {
     "type": "change",
     "subscription_id": "messages_with_history",
     "change_type": "insert",
     "rows": [{"id": 101, "content": "New message"}]
   }
   ```

---

## Error Types

KalamDB sends errors as `{"type":"error", "subscription_id": "...", "code": "...", "message": "..."}`.

Common `code` values:

| Code | Description |
|------|-------------|
| `INVALID_SUBSCRIPTION_ID` | Subscription ID cannot be empty |
| `SUBSCRIPTION_LIMIT_EXCEEDED` | User has too many active subscriptions |
| `INVALID_SQL` | SQL parsing failed |
| `UNAUTHORIZED` | Permission denied |
| `NOT_FOUND` | Referenced table/object not found |
| `UNSUPPORTED` | Unsupported operation (e.g., non-SELECT subscription) |
| `MESSAGE_TOO_LARGE` | Message exceeds `security.max_ws_message_size` |
| `RATE_LIMIT_EXCEEDED` | Too many messages per connection |
| `UNSUPPORTED_DATA` | Binary frames from client are not supported |
| `BATCH_FETCH_FAILED` | Failed to fetch a requested batch |

---

## Rate Limiting

Rate limiting is enabled for WebSocket connections.

Defaults (configurable via `rate_limit.*` in `server.toml`):
- Max subscriptions per user: 10
- Max messages per second per connection: 50
- Max WebSocket message size: 1 MB (`security.max_ws_message_size`)

**Example (rate limit exceeded)**:
```json
{
  "type": "error",
  "subscription_id": "rate_limit",
  "code": "RATE_LIMIT_EXCEEDED",
  "message": "Too many messages"
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
  const ws = new WebSocket('ws://localhost:8080/v1/ws');
  
  ws.onopen = () => {
    console.log('Connected');
    retryDelay = 1000; // Reset retry delay

    // Authenticate immediately after connecting
    ws.send(JSON.stringify({
      type: 'authenticate',
      method: 'jwt',
      token: '<JWT_TOKEN>'
    }));
    
    // Resubscribe with recent data
    ws.send(JSON.stringify({
      type: 'subscribe',
      subscription: {
        id: 'messages',
        sql: 'SELECT * FROM app.messages',
        options: { last_rows: 100 }
      }
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
    const subscription = { id: queryId, sql, options };
    
    this.ws.send(JSON.stringify({
      type: 'subscribe',
      subscription
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
      case 'subscription_ack':
        console.log('Subscription ack for', msg.subscription_id);
        break;

      case 'initial_data_batch':
        console.log('Initial batch for', msg.subscription_id, ':', msg.rows);

        // Request next batch if available
        if (msg.batch_control?.has_more && msg.batch_control?.last_seq_id != null) {
          this.ws.send(JSON.stringify({
            type: 'next_batch',
            subscription_id: msg.subscription_id,
            last_seq_id: msg.batch_control.last_seq_id
          }));
        }
        break;
        
      case 'change':
        const handlers = this.subscriptions.get(msg.subscription_id)?.handlers || [];
        handlers.forEach(handler => handler(msg));
        break;
        
      case 'error':
        console.error('Subscription error:', msg.code, msg.message);
        break;
    }
  }

  unsubscribe(subscriptionIds) {
    subscriptionIds.forEach(id => {
      this.ws.send(JSON.stringify({
        type: 'unsubscribe',
        subscription_id: id
      }));
      this.subscriptions.delete(id);
    });
  }

  disconnect() {
    this.ws.close();
  }
}

// Usage example
const client = new KalamDBClient('ws://localhost:8080/v1/ws', 'alice');

await client.connect();

await client.subscribe('messages', 'SELECT * FROM app.messages', { last_rows: 10 });

client.onNotification('messages', (notification) => {
  console.log('Change detected:', notification.change_type, notification.rows, notification.old_values);
});
```

---

## Performance Considerations

1. **Batched initial load**: large initial result sets are streamed in batches using `initial_data_batch` + `next_batch`.

2. **Compression**: JSON payloads larger than ~512 bytes may be gzip-compressed and sent as binary frames.

---

## See Also

- [REST API Reference](api-reference.md) - SQL command documentation
- [SQL Syntax Reference](../reference/sql.md) - Complete SQL command list
- [Quick Start Guide](../getting-started/quick-start.md) - Getting started tutorial
