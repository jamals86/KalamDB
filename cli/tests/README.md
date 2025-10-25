# CLI Integration Tests

This directory contains comprehensive integration tests for the KalamDB CLI tools.

## Overview

The test suite covers:
- **CLI functionality** (kalam-cli)
- **WebSocket subscriptions** (real-time updates)
- **kalam-link library** (client SDK)
- **SQL statement coverage** (DDL, DML, queries)
- **Error handling** and edge cases

## Prerequisites

⚠️ **All integration tests require a running KalamDB server**

The tests will:
- Check if server is running at `http://localhost:8080`
- Skip tests gracefully if server is not available
- Fail fast for kalam-link tests (which require server)

## Running Tests

### Start the Server

```bash
# Terminal 1: Start KalamDB server
cd backend
cargo run --bin kalamdb-server
```

### Run All Tests

```bash
# Terminal 2: Run all CLI tests
cd cli
cargo test

# Or with output visible
cargo test -- --nocapture
```

### Run Specific Test Suites

```bash
# CLI integration tests only
cargo test --test test_cli_integration

# WebSocket integration tests only
cargo test --test test_websocket_integration

# kalam-link library tests only
cd kalam-link
cargo test --test integration_tests
```

### Run Individual Tests

```bash
# Run specific test by name
cargo test test_websocket_subscription_creation

# Run tests matching pattern
cargo test websocket -- --nocapture
```

## Test Organization

### test_cli_integration.rs

Tests the `kalam` CLI binary functionality:

- **Connection & Authentication** (T036-T043, T052-T054)
  - Connection handling
  - JWT authentication
  - User ID headers
  - Localhost bypass

- **Output Formatting** (T038-T040)
  - Table format (default)
  - JSON format
  - CSV format

- **Query Execution** (T037, T050-T051)
  - Basic queries
  - Multi-line queries
  - Comments handling
  - Error messages

- **Batch Operations** (T055)
  - File execution
  - Multiple statements

- **Error Handling** (T056-T057)
  - Syntax errors
  - Connection failures
  - Table not found

- **Meta Commands** (T041-T042)
  - `\dt` - List tables
  - `\d table` - Describe table

- **Live Queries** (T043-T046)
  - Subscribe command
  - Filtered subscriptions
  - Pause/resume (Ctrl+S/Ctrl+Q)
  - Unsubscribe command

- **Configuration** (T047-T049)
  - Config file creation
  - Loading config
  - CLI args precedence

- **Advanced Features** (T058-T068)
  - Health check
  - Explicit flush
  - Color output
  - Session timeout
  - Command history
  - Tab completion
  - Pagination
  - Verbose mode

### test_websocket_integration.rs

Tests WebSocket real-time functionality:

- **Client Creation**
  - Builder pattern
  - Configuration options
  - Health checks

- **WebSocket Subscriptions**
  - Subscription creation
  - Custom configuration
  - Initial data snapshot
  - INSERT notifications
  - UPDATE notifications
  - DELETE notifications
  - Filtered subscriptions

- **SQL Coverage**
  - CREATE NAMESPACE
  - CREATE USER TABLE
  - INSERT
  - SELECT
  - UPDATE
  - DELETE
  - DROP TABLE
  - FLUSH TABLE
  - System tables

- **WHERE Clause Operators**
  - Equality (=)
  - LIKE
  - IN

- **Advanced SQL**
  - LIMIT
  - OFFSET
  - ORDER BY

- **Error Handling**
  - Invalid SQL
  - Table not found
  - Connection refused

### integration_tests.rs (kalam-link)

Tests the kalam-link client library:

- **Client Builder**
  - Basic configuration
  - Timeout settings
  - JWT authentication
  - API key authentication
  - Missing URL validation

- **Query Execution**
  - Simple queries
  - Queries with results
  - Error handling
  - Health checks

- **Auth Provider**
  - None (no auth)
  - JWT tokens
  - API keys

- **WebSocket Subscriptions**
  - Config creation
  - Basic subscriptions
  - Custom configurations

- **Change Events**
  - Error detection
  - Subscription ID extraction

- **CRUD Operations**
  - CREATE NAMESPACE
  - CREATE/DROP TABLE
  - INSERT/SELECT
  - UPDATE
  - DELETE

- **System Tables**
  - system.users
  - system.namespaces
  - system.tables

- **Advanced SQL**
  - WHERE operators
  - LIMIT
  - ORDER BY

- **Concurrent Operations**
  - Multiple queries
  - Timeout handling
  - Invalid servers

## Test Coverage

### SQL Statements Covered

✅ **DDL (Data Definition Language)**
- CREATE NAMESPACE
- DROP NAMESPACE
- CREATE USER TABLE
- DROP TABLE
- FLUSH TABLE

✅ **DML (Data Manipulation Language)**
- INSERT
- UPDATE
- DELETE

✅ **DQL (Data Query Language)**
- SELECT
- WHERE (=, LIKE, IN)
- ORDER BY
- LIMIT
- OFFSET

✅ **System Queries**
- system.users
- system.namespaces
- system.tables

### WebSocket Protocol Covered

✅ **Connection**
- WS/WSS URL conversion
- Authentication (JWT, API Key, Headers)
- Upgrade handshake

✅ **Subscription Management**
- Subscribe messages
- Subscription ID tracking
- Unsubscribe messages

✅ **Change Events**
- ACK (subscription confirmed)
- InitialData (snapshot)
- Insert notifications
- Update notifications (with old_rows)
- Delete notifications
- Error events

## Test Patterns

### Server Check Pattern

All tests follow this pattern:

```rust
#[tokio::test]
async fn test_example() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }
    
    // Test implementation...
}
```

### Setup/Cleanup Pattern

Tests that modify data use setup/cleanup:

```rust
#[tokio::test]
async fn test_example() {
    if !is_server_running().await {
        return;
    }
    
    setup_test_data().await.expect("Setup failed");
    
    // Test implementation...
    
    cleanup_test_data().await.ok();
}
```

### Fail-Fast Pattern (kalam-link)

Library tests fail immediately if server not available:

```rust
#[tokio::test]
async fn test_example() {
    ensure_server_running().await; // Panics if server not running
    
    // Test implementation...
}
```

## Troubleshooting

### Tests Are Skipped

**Problem**: All tests show as skipped or passing without running

**Solution**: Start the KalamDB server first:
```bash
cd backend && cargo run --bin kalamdb-server
```

### Connection Refused

**Problem**: Tests fail with "connection refused"

**Solutions**:
1. Check server is running: `curl http://localhost:8080/v1/api/healthcheck`
2. Verify port 8080 is not blocked
3. Check server logs for startup errors

### WebSocket Tests Timeout

**Problem**: WebSocket tests hang or timeout

**Solutions**:
1. Verify WebSocket endpoint: `ws://localhost:8080/v1/ws`
2. Check server WebSocket implementation is enabled
3. Look for WebSocket errors in server logs
4. Increase test timeout if needed

### Table Already Exists

**Problem**: Tests fail with "table already exists"

**Solution**: Cleanup from previous run:
```sql
-- Connect to server and run:
DROP NAMESPACE ws_test CASCADE;
DROP NAMESPACE link_test CASCADE;
DROP NAMESPACE test_cli CASCADE;
```

### Permission Denied

**Problem**: Tests fail with permission errors

**Solution**: 
1. Check RocksDB data directory permissions
2. Clear test data: `rm -rf backend/data/rocksdb/*`
3. Restart server with clean state

## CI/CD Integration

To run tests in CI:

```yaml
# .github/workflows/test.yml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      # Start server in background
      - name: Start KalamDB Server
        run: |
          cd backend
          cargo run --bin kalamdb-server &
          sleep 5
      
      # Run tests
      - name: Run CLI Tests
        run: |
          cd cli
          cargo test -- --nocapture
```

## Writing New Tests

### Template for New Test

```rust
#[tokio::test]
async fn test_new_feature() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.expect("Setup failed");

    let client = create_test_client().expect("Failed to create client");
    
    // Test your feature
    let result = client.execute_query("YOUR SQL").await;
    assert!(result.is_ok(), "Feature should work");
    
    cleanup_test_data().await.ok();
}
```

### Best Practices

1. **Always check server is running** at test start
2. **Use unique namespaces** to avoid conflicts
3. **Clean up after yourself** with DROP CASCADE
4. **Add delays** after CREATE/DROP (100-200ms)
5. **Handle both success and "already exists"** errors
6. **Use descriptive assertions** with custom messages
7. **Test error cases** as well as success cases

## Test Metrics

Current coverage:
- **34 CLI integration tests**
- **40+ WebSocket integration tests**
- **35+ kalam-link library tests**
- **Total: 109+ integration tests**

Coverage areas:
- ✅ HTTP API endpoints
- ✅ WebSocket subscriptions
- ✅ SQL statement execution
- ✅ Authentication methods
- ✅ Error handling
- ✅ Output formatting
- ✅ Configuration management
- ✅ Concurrent operations

## References

- [API Reference](../../docs/architecture/API_REFERENCE.md)
- [WebSocket Protocol](../../docs/architecture/WEBSOCKET_PROTOCOL.md)
- [Testing Strategy](../../docs/architecture/testing-strategy.md)
- [Quick Test Guide](../../docs/quickstart/QUICK_TEST_GUIDE.md)
