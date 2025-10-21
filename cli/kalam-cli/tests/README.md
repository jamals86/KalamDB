# KalamDB CLI Integration Tests

This directory contains integration tests for the `kalam-cli` terminal client against a running KalamDB server.

## Test Architecture

The tests use a **two-process model**:

1. **Server Process**: KalamDB server running on `localhost:8080`
2. **CLI Process**: `kalam-cli` binary executed via `assert_cmd`

Tests are **gracefully skipped** if the server is not running, allowing for quick unit test runs without server dependencies.

## Running Tests

### Option 1: With Server Running (Full Integration Tests)

```bash
# Terminal 1: Start the server
cd backend
cargo run --release --bin kalamdb-server

# Terminal 2: Run the tests
cd cli
cargo test -p kalam-cli --test test_cli_integration -- --nocapture
```

### Option 2: Without Server (Quick Validation)

```bash
cd cli
cargo test -p kalam-cli --test test_cli_integration

# Output will show:
# ⚠️  Server not running. Skipping test.
# This is expected behavior.
```

## Test Coverage

### ✅ Implemented Tests (11 tests)

#### Basic Functionality
- **T036**: `test_cli_connection_and_prompt` - Verify CLI help output
- **T037**: `test_cli_basic_query_execution` - Execute SELECT queries
- **T050**: `test_cli_help_command` - Help flag validation
- **T051**: `test_cli_version` - Version output

#### Output Formatting
- **T038**: `test_cli_table_output_formatting` - ASCII table format
- **T039**: `test_cli_json_output_format` - JSON output (`--json`)
- **T040**: `test_cli_csv_output_format` - CSV output (`--csv`)

#### Batch Operations
- **T055**: `test_cli_batch_file_execution` - SQL file execution (`--file`)

#### Error Handling
- **T056**: `test_cli_syntax_error_handling` - Invalid SQL error messages
- **T057**: `test_cli_connection_failure_handling` - Connection error handling

#### Server Health
- `test_server_health_check` - Verify server is accessible

### 🚧 Not Yet Implemented (23 tests)

These require additional test infrastructure:
- T041-T043: Table operations and WebSocket subscriptions
- T044-T049: Advanced subscription features
- T052-T054: Authentication testing
- T058-T068: Advanced CLI features (health, flush, autocomplete, etc.)

## Test Helper Functions

### `is_server_running()`
Checks if the server is accessible at `http://localhost:8080/api/health`.

### `execute_sql(sql: &str)`
Helper to execute SQL via HTTP POST for test data setup.

### `setup_test_data()`
Creates the `test_cli` namespace and `messages` table.

### `cleanup_test_data()`
Drops the `test_cli` namespace and all tables.

## Example Test Output

```
running 11 tests
⚠️  Server not running. Skipping test.
test test_cli_batch_file_execution ... ok
test test_cli_basic_query_execution ... ok
test test_cli_csv_output_format ... ok
test test_cli_help_command ... ok
test test_cli_json_output_format ... ok
test test_cli_syntax_error_handling ... ok
test test_cli_table_output_formatting ... ok
test test_cli_version ... ok
test test_cli_connection_and_prompt ... ok
test test_cli_connection_failure_handling ... ok
test test_server_health_check ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Adding New Tests

1. Add test function with `#[tokio::test]` attribute
2. Check server availability with `is_server_running()`
3. Return early with skip message if server not available
4. Use `assert_cmd::Command::cargo_bin("kalam")` to execute CLI
5. Clean up test data in test body or use cleanup helpers

Example:

```rust
#[tokio::test]
async fn test_new_feature() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.unwrap();

    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("SELECT * FROM test_cli.messages");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("expected output"));

    cleanup_test_data().await.unwrap();
}
```

## Dependencies

- `assert_cmd` - Process spawning and output validation
- `predicates` - Flexible assertions for CLI output
- `reqwest` - HTTP client for server communication
- `tempfile` - Temporary file management for batch tests

## CI/CD Integration

For continuous integration:

```bash
# Start server in background
cargo run --release --bin kalamdb-server &
SERVER_PID=$!

# Wait for server to be ready
sleep 5

# Run tests
cargo test -p kalam-cli --test test_cli_integration

# Cleanup
kill $SERVER_PID
```

## Troubleshooting

### "Server not running" warnings
**Solution**: Start the server in a separate terminal before running tests.

### Connection refused errors
**Solution**: Ensure the server is listening on port 8080:
```bash
lsof -i :8080  # Check if port is in use
```

### Test timeouts
**Solution**: Increase `TEST_TIMEOUT` constant in test file.

### Binary not found
**Solution**: Build the CLI first:
```bash
cargo build -p kalam-cli
```
