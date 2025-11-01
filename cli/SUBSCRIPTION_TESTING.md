# Subscription Testing Quick Reference

## Running Subscription Tests

### 1. Start the Server
```bash
# In terminal 1
cd backend
cargo run --release --bin kalamdb-server
```

### 2. Run Subscription Test
```bash
# In terminal 2
cd cli
cargo test test_subscription_manual -- --nocapture --test-threads=1
```

## Manual CLI Subscription Test

### Basic Subscription
```bash
# Create a test table first
cargo run --bin kalam -- -u http://localhost:8080 --command \
  "CREATE USER TABLE test_cli.events (id INT AUTO_INCREMENT, message VARCHAR, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP) FLUSH ROWS 10"

# Start subscription (will run until Ctrl+C)
cargo run --bin kalam -- -u http://localhost:8080 --subscribe \
  "SELECT * FROM test_cli.events"
```

### Test Data Streaming

```bash
# In terminal 1: Start subscription
cargo run --bin kalam -- -u http://localhost:8080 --subscribe \
  "SELECT * FROM test_cli.events"

# In terminal 2: Insert test data
cargo run --bin kalam -- -u http://localhost:8080 --command \
  "INSERT INTO test_cli.events (message) VALUES ('Test message 1')"

cargo run --bin kalam -- -u http://localhost:8080 --command \
  "INSERT INTO test_cli.events (message) VALUES ('Test message 2')"

# Terminal 1 should show the new rows as they're inserted
```

## Expected Subscription Behavior

✅ **Subscription should:**
1. Connect to server successfully
2. Display initial rows (if table has data)
3. Display new rows as they're inserted
4. Run continuously until Ctrl+C
5. Handle server disconnection gracefully

❌ **Common Issues:**
- "Connection refused" - Server not running
- "Table not found" - Create table first
- No output - Check table has data or insert some rows

## Subscription Listener API

### In Tests

```rust
use common::*;

// Start subscription
let query = "SELECT * FROM test_cli.events";
let mut listener = SubscriptionListener::start(query)?;

// Read lines one at a time
while let Ok(Some(line)) = listener.read_line() {
    println!("Received: {}", line);
    if line.contains("expected_text") {
        break;
    }
}

// Stop subscription
listener.stop()?;
```

### Background Thread Pattern

```rust
use common::*;

// Start background listener
let receiver = start_subscription_listener("SELECT * FROM events")?;

// Wait for events with timeout
use std::time::Duration;
match receiver.recv_timeout(Duration::from_secs(5)) {
    Ok(event) => println!("Got event: {}", event),
    Err(_) => eprintln!("Timeout waiting for event"),
}
```

## Troubleshooting

### Subscription Not Showing Output

1. **Check server is running:**
   ```bash
   curl http://localhost:8080/health
   ```

2. **Check table exists:**
   ```bash
   cargo run --bin kalam -- -u http://localhost:8080 --command \
     "SELECT * FROM system.tables WHERE table_name = 'events'"
   ```

3. **Check table has data:**
   ```bash
   cargo run --bin kalam -- -u http://localhost:8080 --command \
     "SELECT COUNT(*) FROM test_cli.events"
   ```

### Subscription Hangs

- Press Ctrl+C to stop subscription
- Check server logs for errors
- Verify query syntax is correct

### Permission Errors

- Ensure using correct username/password
- Root user (empty password) works for localhost
- Check user has SELECT permission on table

## Test Files

- `test_subscribe.rs` - Subscription integration tests
- `test_subscription_manual.rs` - Manual subscription verification test
- `common.rs` - SubscriptionListener implementation

## CLI Options

```bash
# Subscribe to a query
--subscribe <SQL>

# Subscribe with custom URL
-u http://localhost:8080 --subscribe "SELECT * FROM events"

# Subscribe with authentication
--username alice --password secret123 --subscribe "SELECT * FROM events"
```
