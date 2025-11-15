# Chat + AI Example Smoke Test

This smoke test validates the **real-world chat + AI example** from the main `README.md`.

## What It Tests

The `smoke_chat_ai_example_from_readme` test covers:

1. **Namespace Creation** - Creates a unique chat namespace
2. **User Table Creation** - Creates `conversations` and `messages` tables
3. **Stream Table Creation** - Creates `typing_events` table with TTL
4. **Data Insertion** - Inserts conversations and messages
5. **Data Querying** - Verifies message history is retrievable
6. **Live Subscription** - Subscribes to typing events in real-time
7. **Event Tracking** - Inserts typing/thinking/cancelled events
8. **Subscription Verification** - Confirms events are received via WebSocket

## Architecture

### Tables Created

#### 1. `conversations` (USER TABLE)
```sql
CREATE USER TABLE {namespace}.conversations (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    title TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;
```

**Purpose**: Store chat conversation metadata (one per user).

#### 2. `messages` (USER TABLE)
```sql
CREATE USER TABLE {namespace}.messages (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    conversation_id BIGINT NOT NULL,
    role TEXT NOT NULL,              -- 'user' or 'assistant'
    content TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;
```

**Purpose**: Store all messages in a conversation (user + AI responses).

#### 3. `typing_events` (STREAM TABLE)
```sql
CREATE STREAM TABLE {namespace}.typing_events (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    conversation_id BIGINT NOT NULL,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL,        -- 'typing', 'thinking', 'cancelled'
    created_at TIMESTAMP DEFAULT NOW()
) TTL 30;
```

**Purpose**: Track ephemeral typing indicators (auto-evicted after 30 seconds).

## Test Flow

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Setup Phase                                              │
│    • Create unique namespace (chat_{timestamp}_{counter})   │
│    • Create conversations table (USER)                      │
│    • Create messages table (USER)                           │
│    • Create typing_events table (STREAM, TTL=30s)          │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 2. Conversation Phase                                       │
│    • Insert conversation: "Chat with AI"                    │
│    • Insert user message: "Hello, AI!"                      │
│    • Insert assistant message: "Hi! How can I help..."      │
│    • Query message history (verify both messages)           │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 3. Live Subscription Phase                                  │
│    • Start WebSocket subscription to typing_events          │
│    • Insert event: user_123 → 'typing'                      │
│    • Insert event: ai_model → 'thinking'                    │
│    • Insert event: ai_model → 'cancelled'                   │
│    • Wait for subscription to receive events (5s timeout)   │
│    • Verify at least one event received                     │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 4. Verification Phase                                       │
│    • Stop subscription                                      │
│    • Query typing_events via SELECT (verify persistence)    │
│    • Assert 'typing' and 'thinking' events are present      │
└─────────────────────────────────────────────────────────────┘
```

## Running the Test

### Prerequisites

1. **Server Running**: KalamDB server at `http://localhost:8080`
2. **CLI Built**: `kalam` binary available in PATH or built via `cargo build`

### Run Commands

```bash
# Run just this test
cd cli
cargo test --test smoke chat_ai_example_smoke -- --nocapture

# Run all smoke tests
cargo test --test smoke -- --nocapture

# Run with verbose output
cargo test --test smoke chat_ai_example_smoke -- --nocapture --test-threads=1
```

### Expected Output

```
running 1 test
test chat_ai_example_smoke::smoke_chat_ai_example_from_readme ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 2.35s
```

### Skipped Test Output

If the server is not running:

```
running 1 test
Skipping smoke_chat_ai_example_from_readme: server not running at http://localhost:8080
test chat_ai_example_smoke::smoke_chat_ai_example_from_readme ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.01s
```

## Test Features

### ✅ Uses Common Infrastructure

- **`SubscriptionListener`** - Background thread WebSocket client
- **`execute_sql_as_root_via_cli`** - CLI execution helper
- **`execute_sql_as_root_via_cli_json`** - JSON output for parsing
- **`generate_unique_namespace`** - Collision-free test isolation
- **`is_server_running`** - Graceful skip when server unavailable

### ✅ Real-Time Subscription Testing

```rust
let mut listener = SubscriptionListener::start(&typing_query)
    .expect("failed to start subscription for typing events");

// Insert events
for (user_id, event_type) in &events {
    execute_sql_as_root_via_cli(&insert_event_sql).expect(...);
}

// Wait for subscription to receive events (with timeout)
while start.elapsed() < timeout {
    match listener.try_read_line(Duration::from_millis(500)) {
        Ok(Some(line)) => {
            if line.contains("typing") || line.contains("thinking") {
                received_event = true;
                break;
            }
        }
        Ok(None) => break,     // EOF
        Err(_) => continue,    // Timeout, retry
    }
}
```

### ✅ Unique Test Isolation

Each test run uses a unique namespace to avoid collisions:

```
chat_1731686400123_1  ← First run
chat_1731686400124_2  ← Second run (same millisecond, different counter)
chat_1731686401000_3  ← Next second
```

## Troubleshooting

### Test Fails: "server not running"

**Solution**: Start the KalamDB server:

```bash
cd backend
cargo run --release --bin kalamdb-server
```

### Test Fails: "expected to receive at least one typing event via subscription"

**Possible Causes**:
1. WebSocket subscription not working
2. Stream table events not being broadcast
3. Network latency > 5 seconds

**Solution**: Increase timeout in test or check server logs:

```bash
tail -f backend/logs/kalamdb.log
```

### Test Fails: "failed to create typing_events table"

**Possible Causes**:
1. Namespace collision (rare with unique generation)
2. Stream table syntax not supported

**Solution**: Check server version supports `CREATE STREAM TABLE` with `TTL`.

## Relation to README Example

This test directly mirrors the CLI commands in `README.md` under **Basic Usage – Real-World Chat + AI Example**:

| README Step | Test Coverage |
|-------------|---------------|
| Create namespace and tables | ✅ Step 1 |
| Start conversation, add messages | ✅ Steps 2-3 |
| Query conversation history | ✅ Step 4 |
| Track typing/thinking/cancel events | ✅ Step 6 |
| Subscribe to live typing updates | ✅ Step 5 + 7 |

**Purpose**: Ensure README examples are always working and up-to-date.

## Future Enhancements

- [ ] Parse actual conversation ID from `RETURNING id` JSON instead of assuming `1`
- [ ] Test TTL eviction after 30 seconds (verify events auto-delete)
- [ ] Test multiple concurrent subscriptions (simulate 2+ users watching same conversation)
- [ ] Add AI message streaming (simulate chunked assistant response)
- [ ] Test conversation deletion (CASCADE to messages and typing events)

## Related Tests

- **`smoke_test_stream_subscription.rs`** - General stream table + subscription testing
- **`smoke_test_user_table_subscription.rs`** - User table real-time subscriptions
- **`smoke_test_shared_table_crud.rs`** - Shared table CRUD operations

---

**Last Updated**: November 15, 2025  
**Test Status**: ✅ Passing  
**Server Version**: 0.1.0 (012-full-dml-support)
