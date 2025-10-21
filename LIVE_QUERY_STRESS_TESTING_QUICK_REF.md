# Live Query and Stress Testing Quick Reference

**Feature Branch**: 004-system-improvements-and  
**Spec**: specs/004-system-improvements-and/spec.md  
**Status**: ✅ Specified, Ready for Implementation

## User Story 11: Live Query Change Detection

### Purpose
Verify live query subscriptions correctly detect and deliver all data changes (INSERT/UPDATE/DELETE) in real-time across concurrent operations.

### Test Scenarios

#### 1. INSERT Detection
```rust
// Create messages table, start WebSocket subscription
// INSERT 100 messages from separate thread
// Verify: All 100 INSERT notifications received
```

#### 2. UPDATE Detection
```rust
// Subscribe to messages table
// INSERT 50 messages, UPDATE all 50
// Verify: 50 INSERT + 50 UPDATE notifications with old/new values
```

#### 3. DELETE Detection
```rust
// Subscribe to messages
// INSERT 30, DELETE 15 (soft delete)
// Verify: 30 INSERT + 15 DELETE with _deleted=true
```

#### 4. Concurrent Writers
```rust
// 5 writer threads × 20 messages each = 100 total
// Single listener subscription
// Verify: All 100 messages received, no loss/duplication
```

#### 5. AI Scenario
```rust
// Simulate AI agent writing messages
// Human client subscribes with conversation_id filter
// Verify: All AI messages delivered in real-time
```

#### 6. Mixed Operations Ordering
```rust
// Sequence: INSERT msg1, UPDATE msg1, INSERT msg2, DELETE msg1
// Verify: Listener receives changes in exact chronological order
```

#### 7. Changes Counter Accuracy
```rust
// Subscribe, trigger 50 changes (INSERT/UPDATE/DELETE)
// Query system.live_queries
// Verify: changes field = 50
```

#### 8. Multiple Listeners
```rust
// Create 3 concurrent WebSocket subscriptions to same table
// INSERT 20 messages
// Verify: Each listener receives all 20 independently
```

#### 9. Reconnection Resilience
```rust
// Subscribe, INSERT 10, disconnect/reconnect, INSERT 10 more
// Verify: No messages lost during reconnection
```

#### 10. High-Frequency Changes
```rust
// INSERT 1000 messages as fast as possible
// Verify: Listener receives all 1000 with correct sequence numbers
```

### Success Criteria
- **SC-051**: 100% INSERT notification delivery accuracy
- **SC-052**: 100% UPDATE notifications include old/new values
- **SC-053**: DELETE notifications correctly identify _deleted rows
- **SC-054**: Changes delivered within 50ms under concurrent load
- **SC-055**: system.live_queries changes counter 100% accurate

### Functional Requirements
- FR-156: Verify INSERT notifications
- FR-157: Verify UPDATE with old/new values
- FR-158: Verify DELETE with _deleted flag
- FR-159: Test concurrent writers without loss
- FR-160: Simulate AI agent scenarios
- FR-161: Verify chronological ordering
- FR-162: Validate changes counter
- FR-163: Test multiple subscriptions independently
- FR-164: Test reconnection without data loss
- FR-165: Validate high-frequency delivery (1000+)

---

## User Story 12: Memory Leak and Stress Testing

### Purpose
Ensure system handles sustained high load without memory leaks, resource exhaustion, or performance degradation.

### Test Scenarios

#### 1. Memory Stability Under Write Load
```rust
// 10 writer threads × 10,000 rows each
// Measure memory every 30 seconds
// Verify: Memory growth < 10% over baseline
```

#### 2. Concurrent Writers and Listeners
```rust
// 10 writers + 20 WebSocket listeners
// Run for 5 minutes
// Verify: No WebSocket disconnections, all messages delivered
```

#### 3. CPU Usage Under Load
```rust
// Sustained write load (1000 inserts/sec)
// Measure CPU usage
// Verify: Average < 80%, system remains responsive
```

#### 4. WebSocket Connection Leak Detection
```rust
// Create 50 WebSocket subscriptions, close 25
// Verify: Server releases connections (system.live_queries + netstat)
```

#### 5. Memory Release After Stress
```rust
// Run heavy load test, stop all writers/listeners
// Wait 60 seconds
// Verify: Memory returns to within 5% of baseline
```

#### 6. Query Performance Under Stress
```rust
// While stress test runs (10 writers, 20 listeners)
// Execute SELECT queries
// Verify: Response times < 500ms at p95
```

#### 7. Flush Operations During Stress
```rust
// Continuous writes with periodic manual flushes
// Verify: No memory accumulation from unflushed buffers
```

#### 8. Actor System Stability
```rust
// Monitor actor system (flush jobs, live query actors)
// Verify: No mailbox overflow or stuck actors
```

#### 9. RocksDB Memory Bounds
```rust
// Configure RocksDB memory limits
// Run stress test
// Verify: RocksDB respects bounds, no OOM
```

#### 10. Graceful Degradation
```rust
// Gradually increase load until system reaches capacity
// Verify: Degrades gracefully (slower responses) vs crashing
```

### Success Criteria
- **SC-056**: Memory growth < 10% during 5-minute stress test
- **SC-057**: WebSocket uptime 99.9% under 100,000+ notifications
- **SC-058**: Query p95 response time < 500ms under stress
- **SC-059**: Memory release within 5% baseline after 60 seconds
- **SC-060**: Graceful degradation (no crashes) under extreme load

### Functional Requirements
- FR-166: Monitor memory usage during sustained load
- FR-167: Verify memory growth < 10% over baseline
- FR-168: Validate WebSocket stability under high load
- FR-169: Verify CPU usage < 80% average during writes
- FR-170: Validate WebSocket connection cleanup
- FR-171: Verify memory release after stress
- FR-172: Validate query performance < 500ms p95
- FR-173: Verify flush operations don't accumulate memory
- FR-174: Monitor actor system health
- FR-175: Validate graceful degradation

---

## Implementation Guidelines

### Test File Structure
```rust
// backend/tests/integration/test_live_query_changes.rs
use crate::common::{TestServer, fixtures};

#[tokio::test]
async fn test_live_query_detects_inserts() {
    let server = TestServer::start().await;
    let user_id = "test_user";
    
    // Setup
    fixtures::create_namespace(&server, user_id, "test_ns").await;
    server.execute_sql_as_user(
        user_id,
        "CREATE TABLE test_ns.user.messages (id INT, content TEXT)"
    ).await.unwrap();
    
    // Test implementation
    // ...
}
```

### Stress Test Pattern
```rust
// backend/tests/integration/test_stress_and_memory.rs
#[tokio::test]
async fn test_memory_stability_under_write_load() {
    let server = TestServer::start().await;
    
    // Baseline measurement
    let baseline_memory = measure_memory();
    
    // Spawn writers
    let handles: Vec<_> = (0..10)
        .map(|i| spawn_writer_thread(&server, i))
        .collect();
    
    // Monitor and verify
    // ...
}
```

### Memory Monitoring Utilities
```rust
fn measure_memory() -> usize {
    // Platform-specific memory measurement
    // Returns memory in bytes
}

fn measure_cpu_usage() -> f64 {
    // Platform-specific CPU measurement
    // Returns percentage (0.0-100.0)
}
```

---

## Key Validations

### Live Query Testing
✅ All INSERT/UPDATE/DELETE notifications delivered  
✅ Old/new values included in UPDATE notifications  
✅ _deleted flag present in DELETE notifications  
✅ No message loss with concurrent writers  
✅ Chronological ordering maintained  
✅ system.live_queries metadata accurate  
✅ Reconnection scenarios handled  
✅ High-frequency changes (1000+) delivered

### Stress Testing
✅ Memory growth < 10% during 5-minute test  
✅ Memory returns to baseline after test  
✅ No WebSocket connection leaks  
✅ CPU usage < 80% average  
✅ Query performance maintained (p95 < 500ms)  
✅ Actor system remains healthy  
✅ RocksDB memory bounds respected  
✅ Graceful degradation under extreme load

---

## Why These Tests Matter

### Production Readiness
- **Live queries**: Core feature requiring comprehensive validation
- **Stress testing**: Identifies resource issues before production
- **Memory leaks**: Can cause slow degradation and eventual crashes
- **Connection management**: Critical for long-running WebSocket systems

### Constitution Alignment
- **Principle VI**: Robust Testing - Comprehensive coverage for critical features
- **Principle II**: Production-Grade Quality - Stress testing ensures stability
- **Principle III**: Clear Architecture - Tests validate component boundaries

### Risk Mitigation
- Silent data loss in notifications → Detected by INSERT/UPDATE/DELETE tests
- Race conditions with concurrent operations → Detected by concurrent writer tests
- Memory leaks → Detected by extended stress tests with memory monitoring
- Connection leaks → Detected by WebSocket lifecycle tests
- Performance degradation → Detected by query performance tests under load

---

## Next Steps

1. **Review**: Stakeholder approval of test scenarios
2. **Plan**: Break down into implementation tasks with `/speckit.plan`
3. **Implement**: 
   - Create test_live_query_changes.rs
   - Create test_stress_and_memory.rs
   - Implement memory/CPU monitoring utilities
   - Set up CI to run stress tests on schedule (not every commit)
4. **Validate**: Run tests and verify success criteria met
5. **Document**: Update test documentation with results and insights

---

## Test Execution Strategy

### Local Development
```bash
# Run live query tests
cargo test --test test_live_query_changes

# Run stress tests (longer duration)
cargo test --test test_stress_and_memory -- --test-threads=1

# Run specific test
cargo test --test test_live_query_changes test_concurrent_writers_no_message_loss
```

### CI/CD Integration
- Live query tests: Run on every commit (fast, comprehensive)
- Stress tests: Run on nightly schedule or pre-release (longer duration)
- Memory monitoring: Platform-specific implementations (Linux: /proc, Windows: WMI)

### Performance Baselines
- Establish baseline metrics on target hardware
- Track over time to detect regressions
- Alert on significant deviations (>15% degradation)

---

## Specification Location

**Main Spec**: `specs/004-system-improvements-and/spec.md`
- User Story 11: Lines 305-350 (approx)
- User Story 12: Lines 352-397 (approx)

**Requirements**: `specs/004-system-improvements-and/checklists/requirements.md`
- Round 6 additions documented
- Updated totals: 12 stories, 175 FRs, 60 SCs, 87 tests

**Summary**: `SPEC_ROUND_6_COMPLETE.md`
- Comprehensive overview of Round 6 additions
- Implementation guidelines
- Commit message template
