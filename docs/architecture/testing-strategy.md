# Testing Strategy for KalamDB

**Purpose**: Define comprehensive testing approach for KalamDB including unit, integration, stress, and performance testing

**Last Updated**: October 23, 2025

## Overview

KalamDB employs a multi-layered testing strategy to ensure reliability, performance, and stability under various load conditions:

1. **Unit Tests**: Component-level testing of individual functions and modules
2. **Integration Tests**: End-to-end testing of complete user workflows
3. **Stress Tests**: Long-running tests to detect memory leaks and performance degradation
4. **Performance Benchmarks**: Repeatable measurements of key performance metrics

## Test Categories

### 1. Unit Tests

**Location**: Within each crate's `tests/` directory or inline with `#[cfg(test)]`

**Purpose**: Test individual functions, structs, and modules in isolation

**Coverage Target**: >80% for core business logic

**Example**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_id_creation() {
        let id = UserId::new("user123");
        assert_eq!(id.as_str(), "user123");
    }
}
```

### 2. Integration Tests

**Location**: `backend/tests/integration/`

**Purpose**: Test complete user workflows and system interactions

**Key Test Files**:
- `test_user_tables.rs` - USER table operations
- `test_shared_tables.rs` - SHARED table operations
- `test_stream_tables.rs` - STREAM table operations
- `test_live_query_changes.rs` - Live query subscriptions
- `test_automatic_flushing.rs` - Automatic flush triggers
- `test_flush_operations.rs` - Manual flush commands
- `test_storage_management.rs` - Storage configuration
- `test_system_tables.rs` - System table queries
- `test_stress_and_memory.rs` - Stress testing and memory leak detection

**Test Harness**: Uses `common/mod.rs` with `TestServer` for server lifecycle management

**Example**:
```rust
#[actix_web::test]
async fn test_user_table_insert_and_query() {
    let server = setup_test_server(TestConfig::default()).await;
    
    // Create table
    let response = server.execute_sql("CREATE USER TABLE messages ...").await;
    assert!(response.success);
    
    // Insert data
    let response = server.execute_sql("INSERT INTO messages ...").await;
    assert!(response.success);
    
    // Query data
    let response = server.execute_sql("SELECT * FROM messages").await;
    assert_eq!(response.row_count, 1);
}
```

### 3. Stress Tests and Memory Leak Detection

**Location**: `backend/tests/integration/test_stress_and_memory.rs`

**Purpose**: Verify system stability under sustained high load and detect memory leaks

**Test Duration**: 5+ minutes per test (marked with `#[ignore]` attribute)

**Key Stress Tests**:

#### T220: Memory Stability Under Write Load
- **Goal**: Verify memory growth <10% over 5 minutes
- **Method**:
  1. Measure baseline memory (RSS)
  2. Spawn 10 concurrent writers
  3. Maintain constant write rate
  4. Measure memory every 30 seconds
  5. Calculate growth percentage
- **Pass Criteria**: Memory growth <10% from baseline
- **Failure Indicators**: Memory growth >10%, out-of-memory errors

#### T221: Concurrent Writers and Listeners
- **Goal**: Verify no WebSocket disconnections under sustained load
- **Method**:
  1. Spawn 10 concurrent writers (inserts)
  2. Create 20 WebSocket subscriptions (live queries)
  3. Run for 5 minutes
  4. Monitor disconnection count
- **Pass Criteria**: Zero unexpected disconnections
- **Failure Indicators**: WebSocket disconnects, notification delays >10ms

#### T222: CPU Usage Under Load
- **Goal**: Verify CPU usage <80% under 1000 inserts/sec
- **Method**:
  1. Spawn writers to achieve 1000 inserts/sec
  2. Monitor CPU usage every 1 second
  3. Calculate average and peak CPU
- **Pass Criteria**: Average CPU <60%, peak CPU <80%
- **Failure Indicators**: CPU saturation (>90%), throttling

#### T223: WebSocket Connection Leak Detection
- **Goal**: Verify proper cleanup of closed connections
- **Method**:
  1. Create 50 WebSocket subscriptions
  2. Close 25 subscriptions randomly
  3. Wait 30 seconds for cleanup
  4. Verify connection count == 25
  5. Check memory usage stable
- **Pass Criteria**: Correct connection count, no memory leaks
- **Failure Indicators**: Stale connections, memory not released

#### T224: Memory Release After Stress
- **Goal**: Verify memory returns to baseline after load stops
- **Method**:
  1. Measure baseline memory
  2. Run stress test (writers + listeners) for 2 minutes
  3. Stop all operations
  4. Wait 60 seconds for cleanup
  5. Measure final memory
- **Pass Criteria**: Final memory within 15% of baseline
- **Failure Indicators**: Memory not released, resource leaks

#### T225: Query Performance Under Stress
- **Goal**: Verify query latency <500ms (p95) during stress
- **Method**:
  1. Start background stress (10 writers)
  2. Execute SELECT queries every 1 second
  3. Measure query latencies
  4. Calculate p50, p95, p99 percentiles
- **Pass Criteria**: p95 latency <500ms, p50 <100ms
- **Failure Indicators**: High latency variance, timeouts

#### T226: Flush Operations During Stress
- **Goal**: Verify no memory accumulation with periodic flushes
- **Method**:
  1. Start background stress (10 writers)
  2. Trigger flush every 30 seconds
  3. Monitor memory after each flush
  4. Verify memory stable or decreasing
- **Pass Criteria**: Memory stable between flushes
- **Failure Indicators**: Memory accumulation, flush failures

#### T227: Actor System Stability
- **Goal**: Verify no mailbox overflow in flush/live query actors
- **Method**:
  1. Start background stress
  2. Monitor actor mailbox sizes
  3. Monitor actor processing rates
  4. Check for dropped messages
- **Pass Criteria**: Mailbox sizes <1000, no dropped messages
- **Failure Indicators**: Mailbox overflow, message loss

#### T228: Graceful Degradation
- **Goal**: Verify graceful slowdown (not crashes) at capacity
- **Method**:
  1. Start with moderate load (100 inserts/sec)
  2. Gradually increase load every 30 seconds
  3. Monitor response times and error rates
  4. Stop at first errors or extreme latency
- **Pass Criteria**: Graceful latency increase, HTTP 503 on overload
- **Failure Indicators**: Panics, crashes, connection drops

### Running Stress Tests

**Command**:
```bash
cd backend
cargo test --ignored test_stress_and_memory
```

**Time Commitment**: 30-60 minutes for full stress test suite

**Resource Requirements**:
- 8+ GB RAM
- 4+ CPU cores
- 10+ GB disk space

**CI/CD Integration**: Run nightly, not on every commit

### 4. Performance Benchmarks

**Location**: `backend/benches/performance.rs`

**Purpose**: Repeatable measurements of key performance metrics

**Tool**: Criterion.rs for statistical analysis

**Key Benchmarks**:
- `bench_rocksdb_writes` - Single write latency (<1ms)
- `bench_rocksdb_reads` - Single read latency (<1ms)
- `bench_datafusion_queries` - Query execution time
- `bench_flush_operations` - Flush throughput
- `bench_websocket_serialization` - Notification serialization
- `bench_sustained_write_load` - 1000 inserts/sec sustained
- `bench_concurrent_writes` - Multi-threaded write scaling

**Running Benchmarks**:
```bash
cd backend
cargo bench
```

**Output**: HTML report in `target/criterion/`

## Test Utilities and Infrastructure

### Stress Test Utilities (`backend/tests/integration/common/stress_utils.rs`)

**Purpose**: Reusable components for stress testing

**Components**:

#### ConcurrentWriters
Spawns multiple writer threads with configurable insert rate.

```rust
let config = WriterConfig {
    writer_count: 10,
    target_rate: 1000, // inserts/sec total
    namespace: "default".to_string(),
    table_name: "stress_test".to_string(),
};

let mut writers = ConcurrentWriters::new(config);
writers.start().await;

// Run stress...

let stats = writers.stop().await;
println!("Total inserts: {}", stats.total_inserts);
```

#### WebSocketSubscribers
Creates multiple WebSocket connections with monitoring.

```rust
let config = SubscriberConfig {
    subscriber_count: 20,
    namespace: "default".to_string(),
    table_name: "stress_test".to_string(),
};

let mut subscribers = WebSocketSubscribers::new(config);
subscribers.start().await;

// Monitor notifications...

let stats = subscribers.stop().await;
println!("Notifications: {}", stats.total_notifications);
println!("Disconnects: {}", stats.total_disconnects);
```

#### MemoryMonitor
Measures memory usage at regular intervals.

```rust
let mut monitor = MemoryMonitor::new(Duration::from_secs(30));
monitor.start();

// Run stress...

let measurements = monitor.stop().await;
let growth = monitor.memory_growth_percentage().await;
assert!(growth.unwrap() < 10.0);
```

#### CpuMonitor
Measures CPU usage over time.

```rust
let mut monitor = CpuMonitor::new(Duration::from_secs(1));
monitor.start();

// Run stress...

let measurements = monitor.stop().await;
let max_cpu = monitor.max_cpu_percent().await;
assert!(max_cpu.unwrap() < 80.0);
```

## Performance Targets

| Metric | Target | Test |
|--------|--------|------|
| Write latency | <1ms (p95) | bench_rocksdb_writes |
| Read latency | <1ms (p95) | bench_rocksdb_reads |
| Query latency | <100ms (simple) | bench_datafusion_queries |
| Notification latency | <10ms | test_live_query_changes |
| Flush throughput | >10,000 rows/sec | bench_flush_operations |
| Sustained write rate | 1000 inserts/sec | bench_sustained_write_load |
| Memory growth | <10% over 5 min | test_memory_stability |
| CPU usage | <80% at 1000/sec | test_cpu_usage_under_load |
| WebSocket stability | 0 disconnects | test_concurrent_writers_and_listeners |

## Test Execution Schedule

### Development (Pre-Commit)
- Run unit tests: `cargo test --lib`
- Run quick integration tests: `cargo test --test '*' (exclude #[ignore])`
- Time: ~2 minutes

### CI/CD (Pull Request)
- Run all unit tests
- Run all integration tests (exclude stress tests)
- Run core benchmarks
- Time: ~10 minutes

### Nightly Build
- Run all unit tests
- Run all integration tests
- Run all stress tests (`--ignored`)
- Run full benchmark suite
- Generate coverage report
- Time: ~60 minutes

### Release Candidate
- Run all tests (including stress)
- Run extended stress tests (10+ minute duration)
- Manual performance review
- Memory profiling
- Time: ~2 hours

## Monitoring and Metrics

### Memory Metrics
- **RSS (Resident Set Size)**: Total memory used by process
- **Heap**: Allocated heap memory
- **Stack**: Thread stack memory
- **Mmap**: Memory-mapped files (RocksDB, Parquet)

**Tools**:
- Linux: `/proc/self/status`, `smaps`
- macOS: `task_info`, `vmmap`
- Windows: `GetProcessMemoryInfo`

### CPU Metrics
- **User Time**: Time executing user code
- **System Time**: Time in kernel (I/O, system calls)
- **Total CPU**: (User + System) / Wall Time
- **Per-Core Usage**: Distribution across cores

**Tools**:
- Linux: `/proc/self/stat`
- macOS: `task_info`
- Windows: `GetProcessTimes`

### WebSocket Metrics
- **Active Connections**: Current WebSocket count
- **Notifications Sent**: Total notifications delivered
- **Disconnections**: Unexpected connection drops
- **Message Queue Depth**: Pending notifications per connection

## Debugging Failed Stress Tests

### High Memory Growth

**Symptoms**: Memory growth >10% over test duration

**Investigation**:
1. Check for unfreed allocations (use valgrind/heaptrack)
2. Verify RocksDB cache configuration
3. Check DataFusion plan cache eviction
4. Look for retained WebSocket connections

**Common Causes**:
- Memory leak in custom code
- Unbounded cache growth
- Stale WebSocket connections not cleaned up
- Large Parquet files held in memory

### CPU Saturation

**Symptoms**: CPU >80% at target load

**Investigation**:
1. Profile with `perf` or `cargo flamegraph`
2. Check for hot loops
3. Verify async runtime not blocked
4. Check thread pool configuration

**Common Causes**:
- Blocking I/O on async threads
- Inefficient query plans
- Lock contention
- Thread pool starvation

### WebSocket Disconnections

**Symptoms**: Unexpected connection drops

**Investigation**:
1. Check server logs for errors
2. Monitor heartbeat messages
3. Verify network stability
4. Check for backpressure/slow consumers

**Common Causes**:
- Client timeout (heartbeat failure)
- Message queue overflow
- Network issues
- Server overload

## Future Improvements

### Planned Enhancements
- [ ] Automated memory profiling in CI
- [ ] Flamegraph generation for benchmarks
- [ ] Distributed stress testing (multiple clients)
- [ ] Continuous performance tracking (track metrics over time)
- [ ] Integration with monitoring tools (Prometheus, Grafana)

### Additional Test Scenarios
- [ ] Network partition simulation
- [ ] Disk full scenarios
- [ ] Crash recovery testing
- [ ] Schema evolution under load
- [ ] Multi-tenant isolation testing

## References

- [Criterion.rs Documentation](https://bheisler.github.io/criterion.rs/)
- [Tokio Testing Guidelines](https://tokio.rs/tokio/topics/testing)
- [Memory Leak Detection in Rust](https://doc.rust-lang.org/nomicon/)
- [Performance Profiling with Perf](https://perf.wiki.kernel.org/index.php/Tutorial)
