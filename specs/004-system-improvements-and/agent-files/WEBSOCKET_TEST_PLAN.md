# WebSocket Notification Test Plan

## Overview

Comprehensive testing strategy for WebSocket real-time notifications covering both **user tables** and **stream tables** with focus on:
- Performance (no significant overhead on INSERT/UPDATE/DELETE)
- Reliability (storage succeeds even if notification fails)
- Completeness (bulk operations, rapid updates, no message loss)
- Unified behavior (identical pattern for both table types)

## Test Categories

### 1. Basic Functionality Tests

#### User Tables

**File**: `/backend/tests/integration/websocket/test_user_table_notifications.rs`

```rust
#[tokio::test]
async fn test_user_table_insert_notification() {
    // Setup: Create user table with schema
    // Action: Subscribe to table, INSERT single row
    // Assert: Receive InsertNotification within 1 second
    // Assert: Notification data matches inserted row
}

#[tokio::test]
async fn test_user_table_update_notification() {
    // Setup: Create table, insert initial row
    // Action: Subscribe, UPDATE the row
    // Assert: Receive UpdateNotification with new values
    // Assert: row_id matches
}

#[tokio::test]
async fn test_user_table_delete_notification() {
    // Setup: Create table, insert row
    // Action: Subscribe, DELETE the row
    // Assert: Receive DeleteNotification with correct row_id
}
```

#### Stream Tables

**File**: `/backend/tests/integration/websocket/test_stream_table_notifications.rs`

```rust
#[tokio::test]
async fn test_stream_table_insert_notification() {
    // Same as user table but for stream table
    // Verify identical notification behavior
}

// NOTE: Stream tables don't support UPDATE/DELETE
// Focus on INSERT and bulk operations
```

### 2. Bulk Operations Tests (Requirement #6)

Test multiple changes in single statement to ensure NO notifications are lost.

#### User Tables - Bulk INSERT

```rust
#[tokio::test]
async fn test_user_table_bulk_insert_all_notifications_received() {
    // Setup: Subscribe to user table
    // Action: INSERT 100 rows in single SQL statement
    //   INSERT INTO users VALUES (1, 'Alice'), (2, 'Bob'), ..., (100, 'Zoe');
    // Assert: Receive exactly 100 InsertNotification events
    // Assert: All notifications received within 5 seconds
    // Assert: Each notification has unique row data
    // Assert: No duplicates, no missing rows
    
    let mut received_ids = HashSet::new();
    let mut notification_count = 0;
    
    // Collect notifications with timeout
    loop {
        match timeout(Duration::from_secs(5), ws_stream.next()).await {
            Ok(Some(Ok(msg))) => {
                if let NotificationMessage::Insert(data) = msg {
                    received_ids.insert(data["id"].as_i64().unwrap());
                    notification_count += 1;
                }
                if notification_count >= 100 { break; }
            }
            _ => break,
        }
    }
    
    assert_eq!(notification_count, 100, "Expected 100 notifications");
    assert_eq!(received_ids.len(), 100, "Expected 100 unique IDs");
}
```

#### User Tables - Bulk UPDATE

```rust
#[tokio::test]
async fn test_user_table_bulk_update_all_notifications_received() {
    // Setup: Insert 50 rows, subscribe to table
    // Action: UPDATE 50 rows in single statement
    //   UPDATE users SET active = true WHERE id <= 50;
    // Assert: Receive exactly 50 UpdateNotification events
    // Assert: No updates missed
}
```

#### Stream Tables - Bulk INSERT

```rust
#[tokio::test]
async fn test_stream_table_bulk_insert_all_notifications_received() {
    // Setup: Subscribe to stream table
    // Action: INSERT 200 events in single statement
    // Assert: Receive exactly 200 InsertNotification events
    // Assert: All events have unique timestamps
    // Assert: No events lost
}
```

### 3. Rapid Sequential Updates (Requirement #6)

Test multiple updates on same row to ensure all changes are delivered in order.

```rust
#[tokio::test]
async fn test_user_table_rapid_updates_same_row_no_loss() {
    // Setup: Insert row with id=123, status='pending'
    // Action: Subscribe to table
    // Action: UPDATE row 5 times rapidly (< 100ms apart)
    //   UPDATE users SET status='processing' WHERE id=123;  (t=0ms)
    //   UPDATE users SET status='validating' WHERE id=123;  (t=20ms)
    //   UPDATE users SET status='approved' WHERE id=123;    (t=40ms)
    //   UPDATE users SET status='completed' WHERE id=123;   (t=60ms)
    //   UPDATE users SET status='archived' WHERE id=123;    (t=80ms)
    // Assert: Receive exactly 5 UpdateNotification events
    // Assert: Notifications in correct order
    // Assert: Final notification has status='archived'
    
    let mut updates = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    
    while Instant::now() < deadline && updates.len() < 5 {
        if let Ok(Some(Ok(msg))) = timeout(Duration::from_millis(500), ws_stream.next()).await {
            if let NotificationMessage::Update(data) = msg {
                updates.push(data["status"].as_str().unwrap().to_string());
            }
        }
    }
    
    assert_eq!(updates.len(), 5, "Expected 5 update notifications");
    assert_eq!(updates, vec!["processing", "validating", "approved", "completed", "archived"]);
}
```

### 4. Performance Tests (Requirement #1)

Verify notifications add minimal overhead (< 5%) to INSERT/UPDATE/DELETE operations.

```rust
#[tokio::test]
async fn test_insert_performance_without_subscribers() {
    // Baseline: Measure 1000 INSERTs with NO subscribers
    let start = Instant::now();
    for i in 0..1000 {
        execute_insert(&db, i).await?;
    }
    let baseline_duration = start.elapsed();
    
    println!("Baseline (no subscribers): {:?}", baseline_duration);
    // Expected: < 100ms for 1000 inserts (~100 µs per insert)
}

#[tokio::test]
async fn test_insert_performance_with_subscribers() {
    // With 5 subscribers: Measure 1000 INSERTs
    let (sub1, sub2, sub3, sub4, sub5) = setup_5_subscribers().await;
    
    let start = Instant::now();
    for i in 0..1000 {
        execute_insert(&db, i).await?;
    }
    let with_subscribers_duration = start.elapsed();
    
    println!("With 5 subscribers: {:?}", with_subscribers_duration);
    
    // Assert: Overhead < 5%
    let overhead_pct = ((with_subscribers_duration.as_millis() as f64 / baseline_duration.as_millis() as f64) - 1.0) * 100.0;
    assert!(overhead_pct < 5.0, "Overhead {}% exceeds 5% threshold", overhead_pct);
}

#[tokio::test]
async fn test_async_fire_and_forget_doesnt_block() {
    // Insert row, immediately query to verify INSERT returned before notification delivered
    let insert_start = Instant::now();
    let row_id = execute_insert(&db, 123).await?;
    let insert_duration = insert_start.elapsed();
    
    // INSERT should complete in < 1ms (doesn't wait for notification)
    assert!(insert_duration.as_millis() < 1, "INSERT blocked by notification");
    
    // Verify row is in database immediately
    let row = query_by_id(&db, &row_id).await?;
    assert_eq!(row["id"], 123);
    
    // Notification may arrive later (async)
    // Wait up to 100ms for notification
    let notification = timeout(Duration::from_millis(100), ws_stream.next()).await;
    assert!(notification.is_ok(), "Notification should arrive eventually");
}
```

### 5. Reliability Tests (Requirement #2)

Verify storage succeeds even if notification delivery fails.

```rust
#[tokio::test]
async fn test_insert_succeeds_when_notification_fails() {
    // Setup: Mock LiveQueryManager to always return error
    let broken_manager = create_failing_live_query_manager();
    
    // Action: INSERT row with broken notification system
    let result = execute_insert_with_manager(&db, &broken_manager, 123).await;
    
    // Assert: INSERT succeeds (doesn't propagate notification error)
    assert!(result.is_ok());
    
    // Assert: Row is in database
    let row = query_by_id(&db, &result.unwrap()).await?;
    assert_eq!(row["id"], 123);
    
    // Assert: Error was logged (check logs)
    // logs should contain "Failed to notify subscribers"
}

#[tokio::test]
async fn test_notification_failure_doesnt_rollback_storage() {
    // Setup: Start transaction, insert row, break notification mid-flight
    // Assert: RocksDB write committed even if notification failed
}
```

### 6. Ordering and Consistency Tests

```rust
#[tokio::test]
async fn test_notification_order_matches_insert_order() {
    // Insert 10 rows sequentially
    // Assert: Notifications arrive in same order
    
    let mut inserted_ids = vec![];
    for i in 1..=10 {
        let row_id = execute_insert(&db, i).await?;
        inserted_ids.push(row_id);
    }
    
    let mut received_ids = vec![];
    for _ in 0..10 {
        if let Some(Ok(msg)) = ws_stream.next().await {
            if let NotificationMessage::Insert(data) = msg {
                received_ids.push(data["_row_id"].as_str().unwrap().to_string());
            }
        }
    }
    
    assert_eq!(received_ids, inserted_ids, "Notification order mismatch");
}

#[tokio::test]
async fn test_stream_table_timestamp_ordering() {
    // Stream tables use timestamp-based keys
    // Insert 100 events rapidly
    // Assert: Notifications preserve timestamp order
}
```

### 7. Edge Cases and Error Handling

```rust
#[tokio::test]
async fn test_notification_when_websocket_disconnected() {
    // Setup: Subscribe, then disconnect WebSocket
    // Action: INSERT row
    // Assert: INSERT succeeds (doesn't fail because client gone)
    // Assert: No panic or error in logs
}

#[tokio::test]
async fn test_notification_with_no_subscribers() {
    // Action: INSERT row when no one is subscribed
    // Assert: INSERT succeeds
    // Assert: No errors in logs
    // Assert: Counter metrics show 0 notifications sent
}

#[tokio::test]
async fn test_large_notification_payload() {
    // Insert row with 1MB JSON data
    // Assert: Notification delivered successfully
    // Assert: No truncation of large payloads
}
```

### 8. Filtered Subscription Tests

```rust
#[tokio::test]
async fn test_filtered_subscription_insert_matching() {
    // Subscribe with: SELECT * FROM users WHERE age > 18
    // Insert row with age=25
    // Assert: Receive notification
}

#[tokio::test]
async fn test_filtered_subscription_insert_non_matching() {
    // Subscribe with: SELECT * FROM users WHERE age > 18
    // Insert row with age=15
    // Assert: Do NOT receive notification
}

#[tokio::test]
async fn test_filtered_subscription_update_changes_match() {
    // Subscribe with: WHERE status='active'
    // Insert row with status='pending' (no notification)
    // Update row to status='active' (should receive notification)
}
```

### 9. Multiple Subscribers Tests

```rust
#[tokio::test]
async fn test_multiple_subscribers_all_receive_notification() {
    // Setup: 3 clients subscribe to same table
    // Action: INSERT row
    // Assert: All 3 clients receive identical InsertNotification
}

#[tokio::test]
async fn test_different_filters_different_notifications() {
    // Client A: subscribes with WHERE type='A'
    // Client B: subscribes with WHERE type='B'
    // Insert row with type='A'
    // Assert: Only Client A receives notification
}
```

### 10. Unified Behavior Tests (Requirement #4)

Verify stream tables and user tables have identical notification patterns.

```rust
#[tokio::test]
async fn test_user_and_stream_tables_same_notification_structure() {
    // Insert into user table, capture notification format
    // Insert into stream table, capture notification format
    // Assert: Both use identical ChangeNotification structure
    // Assert: Both have same fields (table_name, event_type, data)
}

#[tokio::test]
async fn test_user_and_stream_tables_same_timing() {
    // Measure notification latency for user table
    // Measure notification latency for stream table
    // Assert: Both < 100ms, similar performance
}
```

## Test Execution

### Run All Tests

```bash
# Backend integration tests
cd backend
cargo test --test test_user_table_notifications -- --test-threads=1
cargo test --test test_stream_table_notifications -- --test-threads=1

# CLI integration tests (should now pass)
cd ../cli
./run_integration_tests.sh
```

### Expected Outcomes

**Before Implementation**:
- ❌ 4 CLI WebSocket tests fail (no notifications received)
- ❌ Backend integration tests don't exist

**After Implementation**:
- ✅ All CLI WebSocket tests pass
- ✅ All backend integration tests pass
- ✅ Bulk insert tests verify no message loss
- ✅ Rapid update tests verify ordering preserved
- ✅ Performance tests verify < 5% overhead
- ✅ Reliability tests verify storage succeeds even if notification fails

## Success Metrics

| Metric | Target | Test Coverage |
|--------|--------|---------------|
| Notification Delivery Rate | 100% | Bulk insert tests |
| Notification Latency | < 100ms | Basic functionality tests |
| INSERT Overhead | < 5% | Performance tests |
| Storage Success (notification fails) | 100% | Reliability tests |
| Message Ordering | 100% | Ordering tests |
| Filtered Subscription Accuracy | 100% | Filtered subscription tests |
| User/Stream Table Parity | 100% | Unified behavior tests |

## Implementation Checklist

- [ ] Implement notification in UserTableInsertHandler
- [ ] Implement notification in UserTableUpdateHandler  
- [ ] Implement notification in UserTableDeleteHandler
- [ ] Wire LiveQueryManager through UserTableProvider
- [ ] Write basic functionality tests (user tables)
- [ ] Write basic functionality tests (stream tables)
- [ ] Write bulk operation tests (requirement #6)
- [ ] Write rapid update tests (requirement #6)
- [ ] Write performance tests (requirement #1)
- [ ] Write reliability tests (requirement #2)
- [ ] Write ordering tests
- [ ] Write filtered subscription tests
- [ ] Write multiple subscriber tests
- [ ] Write unified behavior tests (requirement #4)
- [ ] Run CLI integration tests (verify fixes)
- [ ] Measure performance benchmarks
- [ ] Document results

## Notes

- Use `--test-threads=1` for integration tests (avoid race conditions)
- Set realistic timeouts (1-5 seconds for notification delivery)
- Mock LiveQueryManager for reliability tests (simulate failures)
- Use `tokio::time::timeout` to avoid hanging tests
- Log test failures with detailed context (which notification missing, etc.)
