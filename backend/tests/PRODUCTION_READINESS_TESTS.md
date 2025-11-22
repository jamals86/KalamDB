# Production Readiness Tests

**Status**: ⚠️ **Needs API Compatibility Fix** - Tests designed, need rewrite using TestServer HTTP API  
**Location**: `backend/tests/test_prod_*.rs`  
**Purpose**: Validate KalamDB's robustness, durability, and operational readiness

> **⚠️ Current State**: Tests written but use outdated internal APIs. Need 3-4 hour rewrite to use `TestServer` pattern.  
> **See**: [PRODUCTION_READINESS_STATUS.md](./PRODUCTION_READINESS_STATUS.md) for fix instructions and examples.

---

## Overview

This test suite focuses on **failure modes**, **edge cases**, **concurrency**, **durability**, and **observability** to ensure KalamDB can be safely deployed in production environments.

**Key Principles**:
- ✅ **No core backend modifications** - Only tests and test helpers
- ✅ **Observe existing behavior** - Document gaps as TODOs
- ✅ **Fast enough for CI** - No external dependencies, deterministic execution
- ✅ **Clear failure messages** - Easy to debug when tests fail

---

## Test Categories

### 1. Configuration and Startup Tests
**File**: `test_configuration_startup.rs`

Validates server startup behavior and configuration handling:

- ✅ Default config starts successfully
- ✅ Server readiness checks (system tables queryable)
- ✅ Multiple namespace creation
- ✅ Concurrent queries during startup
- ✅ Invalid namespace names rejected
- ✅ Invalid flush policies rejected
- ✅ Invalid TTL values rejected
- ✅ Tables without PRIMARY KEY rejected
- ✅ Unknown table types rejected
- ✅ System tables schema consistency
- ✅ DROP NAMESPACE with/without IF EXISTS

**Run**: 
```bash
cargo test --test production_readiness -- test_configuration_startup --nocapture
```

---

### 2. Durability and Crash Recovery Tests
**File**: `test_durability_crash_recovery.rs`

Validates data survives server restarts and crashes:

- ✅ Data survives server restart (RocksDB persistence)
- ✅ Flushed data survives restart (Parquet durability)
- ✅ Manifest integrity after multiple flushes
- ✅ Mixed data consistency (flushed + buffered)
- ✅ Table metadata survives restart

**Approach**: Tests restart the server with the same data directory and verify:
- Tables remain queryable
- Row counts are consistent
- Parquet files are accessible
- Schema metadata is preserved

**Run**:
```bash
cargo test --test production_readiness -- test_durability_crash_recovery --nocapture
```

**Note**: These tests simulate clean shutdowns (RocksDB persists WAL). True crash recovery (process kill mid-operation) requires external process spawning.

---

### 3. Concurrency and Contention Tests
**File**: `test_concurrency_contention.rs`

Validates correct behavior under concurrent access:

- ✅ Concurrent writers to same user table
- ✅ Concurrent inserts with PRIMARY KEY conflicts
- ✅ Read/write contention (readers + writers simultaneously)
- ✅ Concurrent UPDATE operations on same row
- ✅ Concurrent DELETE operations
- ✅ Concurrent shared table access by multiple users

**Patterns Tested**:
- 10 concurrent writers inserting 100 rows total
- PRIMARY KEY conflict resolution (duplicate inserts)
- 5 readers + 3 writers simultaneously
- High-frequency UPDATEs and DELETEs

**Run**:
```bash
cargo test --test production_readiness -- test_concurrency_contention --nocapture
```

---

### 4. Backpressure and Limits Tests
**File**: `test_backpressure_limits.rs`

Validates behavior under resource pressure:

- ✅ Large batch INSERT (100 rows in single statement)
- ✅ Very long string values (10KB strings)
- ✅ Many concurrent clients (20 clients)
- ✅ Rapid-fire sequential queries (50 inserts)
- ✅ Wide tables (30 columns)
- ✅ High-frequency UPDATEs
- ✅ Large query result sets (200 rows)

**Metrics Collected**:
- Queries per second under load
- Success rate for concurrent operations
- Memory behavior (via observation)

**Run**:
```bash
cargo test --test production_readiness -- test_backpressure_limits --nocapture
```

---

### 5. Validation and Error Handling Tests
**File**: `test_validation_error_handling.rs`

Validates clear error messages for invalid operations:

- ✅ Syntax errors return helpful messages
- ✅ Operations on wrong table types (FLUSH on STREAM)
- ✅ Unauthorized ALTER operations on system tables
- ✅ DROP system columns rejected
- ✅ User isolation in USER tables
- ✅ Shared table access levels (PUBLIC vs RESTRICTED)
- ✅ Invalid data types in INSERT
- ✅ NULL constraint violations
- ✅ Permission checks for CREATE USER
- ✅ Invalid FLUSH TABLE format
- ✅ DROP TABLE on non-existent tables
- ✅ Duplicate column names rejected

**Run**:
```bash
cargo test --test production_readiness -- test_validation_error_handling --nocapture
```

---

### 6. Observability Tests
**File**: `test_observability.rs`

Validates system tables and metrics for monitoring:

- ✅ `system.tables` metadata accuracy
- ✅ `system.namespaces` tracking
- ✅ `system.users` visibility (including soft-deletes)
- ✅ `system.jobs` tracking flush jobs
- ✅ `system.storages` listing
- ✅ `system.stats` cache metrics
- ✅ Schema changes reflected in `system.tables`
- ✅ Row counts in `system.tables` accuracy
- ✅ `system.live_queries` subscription tracking

**Key Metrics Verified**:
- `schema_cache_hits`, `schema_cache_misses`, `schema_cache_hit_rate`
- `schema_cache_size`, `schema_cache_evictions`

**Run**:
```bash
cargo test --test production_readiness -- test_observability --nocapture
```

---

### 7. Subscription Lifecycle Tests
**File**: `test_subscription_lifecycle.rs`

Validates WebSocket subscription behavior:

- ✅ SUBSCRIBE TO returns metadata
- ✅ SUBSCRIBE with WHERE clause
- ✅ SUBSCRIBE with OPTIONS (last_rows)
- ✅ SUBSCRIBE to STREAM tables
- ✅ SUBSCRIBE to SHARED tables
- ✅ SUBSCRIBE without namespace rejected
- ✅ SUBSCRIBE to non-existent table rejected
- ✅ Invalid SUBSCRIBE syntax rejected
- ✅ Concurrent SUBSCRIBE commands
- ✅ Query `system.live_queries` for active subscriptions

**Note**: Full WebSocket lifecycle tests (connect, subscribe, disconnect, cleanup) require WebSocket client implementation in tests.

**Run**:
```bash
cargo test --test production_readiness -- test_subscription_lifecycle --nocapture
```

---

### 8. CLI Robustness Tests
**File**: `test_cli_robustness.rs`

Validates CLI-like behavior (exit codes, scripting):

- ✅ Successful queries return Success status (simulates exit code 0)
- ✅ Failed queries return Error status (simulates non-zero exit code)
- ✅ Batch execution (multiple statements)
- ✅ Clear error messages for common mistakes
- ✅ Idempotent namespace operations (IF NOT EXISTS)
- ✅ Idempotent table operations (IF NOT EXISTS)
- ✅ Query output consistency
- ✅ Empty result sets handled correctly
- ✅ Large result sets
- ✅ Meta-commands simulation (\dt, \stats)
- ✅ Error recovery (continue after error)

**Run**:
```bash
cargo test --test production_readiness -- test_cli_robustness --nocapture
```

---

## Running All Tests

### Run entire production readiness suite:
```bash
cargo test --test production_readiness --nocapture
```

### Run specific category:
```bash
cargo test --test production_readiness test_configuration_startup --nocapture
cargo test --test production_readiness test_durability_crash_recovery --nocapture
cargo test --test production_readiness test_concurrency_contention --nocapture
cargo test --test production_readiness test_backpressure_limits --nocapture
cargo test --test production_readiness test_validation_error_handling --nocapture
cargo test --test production_readiness test_observability --nocapture
cargo test --test production_readiness test_subscription_lifecycle --nocapture
cargo test --test production_readiness test_cli_robustness --nocapture
```

### Run all backend tests (including production readiness):
```bash
cd backend
cargo test --release
```

---

## CI Integration

### GitHub Actions Workflow

Add to `.github/workflows/rust.yml`:

```yaml
name: Rust Tests

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main, develop ]

jobs:
  test:
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rust-lang/setup-rust-toolchain@v1
      
    - name: Run all tests
      run: |
        cd backend
        cargo test --release -- --nocapture
        
    - name: Run production readiness tests
      run: |
        cd backend
        cargo test --test production_readiness --release -- --nocapture
```

---

## Test Coverage Summary

| Category | Tests | Lines | Coverage |
|----------|-------|-------|----------|
| Configuration & Startup | 13 | ~400 | Config validation, startup checks |
| Durability & Crash Recovery | 6 | ~550 | Restart scenarios, manifest integrity |
| Concurrency & Contention | 6 | ~650 | Concurrent writers, conflicts, read/write |
| Backpressure & Limits | 7 | ~450 | Large payloads, many clients, high frequency |
| Validation & Error Handling | 12 | ~600 | Invalid SQL, permissions, type checks |
| Observability | 10 | ~550 | System tables, metrics, stats |
| Subscription Lifecycle | 10 | ~400 | SUBSCRIBE commands, filters, options |
| CLI Robustness | 11 | ~500 | Exit codes, batch execution, scripting |
| **TOTAL** | **75** | **~4,100** | **Production-ready validation** |

---

## Known Limitations & Future Work

### 1. **Crash Recovery (SIGKILL)**
**Current**: Tests simulate clean shutdowns (RocksDB persists WAL)  
**Future**: Add tests that kill server process mid-operation:
- Spawn server in subprocess
- Kill with `SIGKILL` during INSERT/FLUSH
- Restart and verify no corruption

**Tracking**: TODO in `test_durability_crash_recovery.rs`

---

### 2. **WebSocket Subscription Full Lifecycle**
**Current**: Tests validate SUBSCRIBE SQL command parsing  
**Future**: Add WebSocket client tests:
- Connect to WebSocket endpoint
- Send subscription message
- Receive initial data + change notifications
- Verify cleanup on disconnect

**Tracking**: TODO in `test_subscription_lifecycle.rs`

---

### 3. **Slow Subscriber Backpressure**
**Current**: Not tested (would require WebSocket client)  
**Future**: Simulate slow consumer:
- Subscribe to high-frequency updates
- Delay reading messages
- Verify server doesn't OOM

**Tracking**: TODO in `test_backpressure_limits.rs`

---

### 4. **Manifest Corruption Detection**
**Current**: Tests verify manifest integrity after restart  
**Future**: Add tests that:
- Manually corrupt manifest.json
- Restart server
- Verify clear error message (not panic)

**Tracking**: TODO in `test_durability_crash_recovery.rs`

---

### 5. **External Process CLI Tests**
**Current**: Tests simulate CLI behavior via API  
**Future**: Spawn actual `kalam` binary:
- Test exit codes in shell
- Capture stdout/stderr
- Test interactive mode (Ctrl+C handling)

**Tracking**: TODO in `test_cli_robustness.rs`

---

### 6. **Backward Compatibility**
**Current**: Not tested  
**Future**: Add upgrade scenario tests:
- Prepare data directory with old manifest format
- Start new server version
- Verify data is readable

**Tracking**: TODO (new test file needed)

---

## Debugging Failures

### Enable detailed logging:
```bash
RUST_LOG=debug cargo test --test production_readiness -- --nocapture
```

### Run single test with backtrace:
```bash
RUST_BACKTRACE=1 cargo test --test production_readiness test_data_survives_server_restart -- --nocapture
```

### Check test server data directory:
```bash
# Tests create temp directories that are auto-cleaned
# To inspect, modify test to use fixed path:
let db_path = "/tmp/test_kalamdb";
```

---

## Contributing

When adding new production readiness tests:

1. **Follow naming convention**: `test_<feature>_<scenario>`
2. **Add documentation**: Explain what real-world risk the test protects against
3. **Keep tests fast**: No sleeps >100ms, no external network calls
4. **Use existing helpers**: Leverage `common::fixtures`, `flush_helpers`
5. **Document TODOs**: If a test can't fully validate behavior, note it

---

## Related Documentation

- [SQL Syntax Reference](../../../docs/SQL.md) - SQL commands and behavior
- [CLI Documentation](../../../docs/CLI.md) - CLI usage and meta-commands
- [Integration Tests](../integration/) - Main integration test suite

---

**Last Updated**: November 22, 2025  
**Test Count**: 75 tests across 8 categories  
**Lines of Test Code**: ~4,100 lines
