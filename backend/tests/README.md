# KalamDB Integration Tests

This directory contains integration tests for KalamDB.

## Test Categories

### 1. Library Tests (Auto-run)
Standard unit and integration tests that run automatically:
```bash
cargo test --lib
```

### 2. WebSocket Live Query Tests (Manual)
Tests that require a running server with WebSocket support.

**Location**: `integration/combined/test_live_query_changes.rs`

**Tests**:
- `test_live_query_detects_inserts` - INSERT notifications ✅ **Working**
- `test_live_query_detects_updates` - UPDATE notifications with old/new values ⚠️ *Needs refactoring*
- `test_live_query_detects_deletes` - DELETE notifications ⚠️ *Needs refactoring*
- `test_concurrent_writers_no_message_loss` - Concurrent write handling ⚠️ *Needs refactoring*
- `test_ai_message_scenario` - Real-world AI chat streaming ⚠️ *Needs refactoring*
- `test_mixed_operations_ordering` - Mixed INSERT/UPDATE/DELETE ordering ⚠️ *Needs refactoring*
- `test_changes_counter_accuracy` - Change counter validation ⚠️ *Needs refactoring*
- `test_multiple_listeners_same_table` - Multiple subscriber isolation ⚠️ *Needs refactoring*
- `test_listener_reconnect_no_data_loss` - Reconnection handling ⚠️ *Needs refactoring*
- `test_high_frequency_changes` - High-frequency update delivery ⚠️ *Needs refactoring*

**Implementation Issue**: 9 tests use `TestServer::new()` (creates in-memory server) but then try to connect to external `ws://localhost:8080/ws`. They need to be updated to use `setup_http_server_and_table()` instead.

## Running WebSocket Tests

### Option 1: Using the Script (Recommended)

1. **Start the KalamDB server** in one terminal:
   ```bash
   cd backend
   cargo run --bin kalamdb-server
   ```

2. **Run the WebSocket tests** in another terminal:
   ```bash
   cd backend
   ./tests/ws_tests.sh
   ```

The script will:
- ✅ Check if the server is running
- ✅ Run working WebSocket tests (1 test currently)
- ✅ Provide colored output and summary
- ✅ Exit with proper status code

**Note**: By default, only tests using `setup_http_server_and_table()` are run. Most tests have implementation issues where they use `TestServer::new()` (in-memory) but then try to connect to `ws://localhost:8080`. These tests need to be refactored.

To run ALL tests (including broken ones):
```bash
RUN_ALL=1 ./tests/ws_tests.sh
```

### Option 2: Manual Test Execution

Run individual tests:
```bash
# Run a single WebSocket test
cargo test --test test_live_query_changes test_live_query_detects_inserts -- --ignored --nocapture

# Run all WebSocket tests
cargo test --test test_live_query_changes -- --ignored --nocapture
```

### Option 3: Custom Server URL

If your server is running on a different URL:
```bash
KALAMDB_URL=http://localhost:3000 ./tests/ws_tests.sh
```

## Why WebSocket Tests Are Ignored

The WebSocket tests are marked with `#[ignore]` because they:
1. Require a running HTTP server (can't auto-start in test environment)
2. Need WebSocket endpoint `/v1/ws` to be available
3. Require JWT authentication support
4. Are slower (real-time operations with timeouts)

This is a common pattern for integration tests that need external services.

## Test Infrastructure

**WebSocket Client Helper**: `integration/common/websocket.rs`
- Real tokio-tungstenite WebSocket implementation
- JWT authentication support
- Subscription management
- Notification collection and assertions

**Test Fixtures**: `integration/common/fixtures.rs`
- Table creation helpers
- Data generation utilities
- Namespace setup

## Troubleshooting

### Server Not Running
```
ERROR: KalamDB server is not running!
```
**Solution**: Start the server with `cargo run --bin kalamdb-server`

### Connection Refused
```
Error: Connection refused (os error 61)
```
**Solution**: Ensure server is listening on `http://localhost:8080` (default)

### Test Timeout
```
Error: Timeout waiting for notifications
```
**Solution**: Server might be overloaded or WebSocket endpoint not working. Check server logs.

### Port Already In Use
```
Error: Address already in use (os error 48)
```
**Solution**: Another server is running on port 8080. Stop it or change the port.

## CI/CD Integration

For CI pipelines, you can run WebSocket tests like this:

```yaml
# Start server in background
- name: Start KalamDB Server
  run: |
    cd backend
    cargo run --bin kalamdb-server &
    sleep 5  # Wait for server to start

# Run WebSocket tests
- name: Run WebSocket Tests
  run: |
    cd backend
    ./tests/ws_tests.sh
```

## Adding New WebSocket Tests

1. Add test function to `test_live_query_changes.rs`
2. Mark it with `#[ignore = "Requires running server with WebSocket support"]`
3. Add the test name to the `WS_TESTS` array in `ws_tests.sh`
4. Document the test in this README

Example:
```rust
#[tokio::test]
#[ignore = "Requires running server with WebSocket support - run manually"]
async fn test_my_new_websocket_feature() {
    let (server, base_url) = setup_http_server_and_table("test_ns", "my_table", "user1").await;
    // ... test implementation
}
```
