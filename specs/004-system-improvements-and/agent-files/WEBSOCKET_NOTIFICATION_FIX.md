# WebSocket Notification Fix - Implementation Plan (UPDATED)

## Problem Summary

WebSocket subscriptions (SUBSCRIBE TO) are currently broken. When clients subscribe to a table and perform INSERT/UPDATE/DELETE operations, they do NOT receive real-time notifications even though:
- The WebSocket infrastructure exists (`kalamdb-live` crate)
- The `LiveQueryManager` has a `notify_table_change()` method
- Subscriptions are stored and managed correctly

**Root Cause**: The `notify_table_change()` method is never called from the INSERT/UPDATE/DELETE operations in **user tables**. Stream tables already have this implemented correctly!

## Critical Requirements (User-Specified)

1. **Performance**: Notifications must NOT add significant overhead to INSERT/UPDATE/DELETE operations
   - Solution: Fire-and-forget async notifications via `tokio::spawn()`
   - Main data path returns immediately after RocksDB write
   
2. **Reliability**: Storage mutations must succeed even if notifications fail
   - Solution: Try-catch around notification, log errors but don't propagate
   - RocksDB write completes first, notification is best-effort
   
3. **Async Push**: Use background tasks for notification delivery
   - Solution: Use `tokio::spawn()` to avoid blocking main operation
   
4. **Unified Approach**: Stream tables AND user tables must use identical notification pattern
   - Solution: Stream tables already implement this correctly - copy the pattern!
   
5. **Comprehensive Testing**: Test edge cases including:
   - Bulk INSERT (multiple rows in one statement)
   - Multiple UPDATE on same row_id
   - Verify all changes received, none dropped
   - Test for both user tables AND stream tables

## Architecture Analysis

### Stream Table Pattern (REFERENCE - ALREADY WORKING ✅)

```rust
// In StreamTableProvider::insert_event() - LINE 207-223
if let Some(live_query_manager) = &self.live_query_manager {
    let notification = ChangeNotification::insert(
        self.table_name().as_str().to_string(),
        row_data.clone(),
    );

    // Deliver notification asynchronously (spawn task to avoid blocking)
    let manager = Arc::clone(live_query_manager);
    let table_name = self.table_name().as_str().to_string();
    tokio::spawn(async move {
        if let Err(e) = manager.notify_table_change(&table_name, notification).await {
            #[cfg(debug_assertions)]
            eprintln!("Failed to notify subscribers: {}", e);
        }
    });
}
```

**Key Pattern Elements**:
1. ✅ Optional LiveQueryManager (graceful degradation if not set)
2. ✅ Clone data before spawning (avoid lifetime issues)
3. ✅ `tokio::spawn()` for async, non-blocking delivery
4. ✅ Error handling that logs but doesn't propagate
5. ✅ Notification happens AFTER successful RocksDB write

### User Table Pattern (NEEDS IMPLEMENTATION ❌)

**Current State**: No notifications at all in user table handlers

**Required State**: Identical pattern to stream tables

## Required Code Changes

### 1. UserTableInsertHandler (`backend/crates/kalamdb-core/src/tables/user_table_insert.rs`)

**Line ~30**: Add field to struct
```rust
pub struct UserTableInsertHandler {
    store: Arc<UserTableStore>,
    live_query_manager: Option<Arc<LiveQueryManager>>,  // ADD THIS
}
```

**Line ~40**: Update constructor
```rust
impl UserTableInsertHandler {
    pub fn new(store: Arc<UserTableStore>) -> Self {
        Self { 
            store,
            live_query_manager: None,  // ADD THIS
        }
    }
    
    // ADD THIS METHOD (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }
}
```

**Line ~90**: Add notification in `insert_row()` AFTER successful RocksDB write
```rust
pub fn insert_row(...) -> Result<String, KalamDbError> {
    // ... existing validation ...
    
    let row_id = self.generate_row_id()?;
    
    // RocksDB write (existing code)
    self.store.put(...)?;
    
    // ✅ REQUIREMENT 2: Notification AFTER storage success
    // ✅ REQUIREMENT 1 & 3: Async fire-and-forget pattern
    if let Some(manager) = &self.live_query_manager {
        let notification = ChangeNotification::insert(
            table_name.as_str().to_string(),
            row_data.clone(),
        );
        
        let mgr = Arc::clone(manager);
        let tname = table_name.as_str().to_string();
        tokio::spawn(async move {
            // ✅ REQUIREMENT 2: Log errors, don't propagate
            if let Err(e) = mgr.notify_table_change(&tname, notification).await {
                log::warn!("Failed to notify subscribers for INSERT: {}", e);
            }
        });
    }
    
    Ok(row_id)
}
```

**Line ~140**: Add notification in `insert_batch()` for EACH row
```rust
pub fn insert_batch(...) -> Result<Vec<String>, KalamDbError> {
    let mut row_ids = Vec::with_capacity(rows.len());

    for row_data in rows {
        // ... existing validation and insert ...
        
        self.store.put(...)?;
        
        // ✅ REQUIREMENT 5: Notify for each row in batch
        if let Some(manager) = &self.live_query_manager {
            let notification = ChangeNotification::insert(
                table_name.as_str().to_string(),
                row_data.clone(),
            );
            
            let mgr = Arc::clone(manager);
            let tname = table_name.as_str().to_string();
            tokio::spawn(async move {
                if let Err(e) = mgr.notify_table_change(&tname, notification).await {
                    log::warn!("Failed to notify subscribers for batch INSERT: {}", e);
                }
            });
        }
        
        row_ids.push(row_id);
    }
    
    Ok(row_ids)
}
```

### 2. UserTableUpdateHandler (`backend/crates/kalamdb-core/src/tables/user_table_update.rs`)

**Same pattern as INSERT**:

**Line ~20**: Add field
```rust
pub struct UserTableUpdateHandler {
    store: Arc<UserTableStore>,
    live_query_manager: Option<Arc<LiveQueryManager>>,  // ADD THIS
}
```

**Line ~30**: Add builder method
```rust
pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
    self.live_query_manager = Some(manager);
    self
}
```

**Line ~110**: Add notification in `update_row()` AFTER successful write
```rust
pub fn update_row(...) -> Result<String, KalamDbError> {
    // ... existing read, merge, write logic ...
    
    self.store.put(...)?;
    
    // ✅ REQUIREMENT 4: Same pattern as stream tables
    if let Some(manager) = &self.live_query_manager {
        let notification = ChangeNotification::update(
            table_name.as_str().to_string(),
            row_id.to_string(),
            existing_row.clone(),
        );
        
        let mgr = Arc::clone(manager);
        let tname = table_name.as_str().to_string();
        tokio::spawn(async move {
            if let Err(e) = mgr.notify_table_change(&tname, notification).await {
                log::warn!("Failed to notify subscribers for UPDATE: {}", e);
            }
        });
    }
    
    Ok(row_id.to_string())
}
```

**Line ~150**: Add notification in `update_batch()` for EACH row
```rust
pub fn update_batch(...) -> Result<Vec<String>, KalamDbError> {
    for (row_id, updates) in row_updates {
        // ... existing update logic ...
        
        // ✅ REQUIREMENT 5: Notify for each update in batch
        // (notification already added in update_row() call)
        let updated_id = self.update_row(...)?;
        updated_row_ids.push(updated_id);
    }
    
    Ok(updated_row_ids)
}
```

### 3. UserTableDeleteHandler (`backend/crates/kalamdb-core/src/tables/user_table_delete.rs`)

**Same pattern as INSERT/UPDATE**:

Add:
- `live_query_manager: Option<Arc<LiveQueryManager>>` field
- `with_live_query_manager()` builder method
- Notification in `delete_row()` AFTER successful RocksDB delete
- Notification in `delete_batch()` for EACH deleted row

```rust
// In delete_row() after self.store.delete(...)
if let Some(manager) = &self.live_query_manager {
    let notification = ChangeNotification::delete(
        table_name.as_str().to_string(),
        row_id.to_string(),
    );
    
    let mgr = Arc::clone(manager);
    let tname = table_name.as_str().to_string();
    tokio::spawn(async move {
        if let Err(e) = mgr.notify_table_change(&tname, notification).await {
            log::warn!("Failed to notify subscribers for DELETE: {}", e);
        }
    });
}
```

### 4. UserTableProvider (`backend/crates/kalamdb-core/src/tables/user_table_provider.rs`)

**Required**: Pass LiveQueryManager when creating handlers

```rust
pub struct UserTableProvider {
    // ... existing fields ...
    live_query_manager: Option<Arc<LiveQueryManager>>,  // ADD if not present
}

impl UserTableProvider {
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    async fn insert_into(&self, ...) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // ✅ REQUIREMENT 4: Pass manager to handlers (same as stream tables)
        let mut insert_handler = UserTableInsertHandler::new(self.store.clone());
        if let Some(manager) = &self.live_query_manager {
            insert_handler = insert_handler.with_live_query_manager(Arc::clone(manager));
        }
        
        // ... rest of insert logic ...
    }
    
    // Similar for UPDATE and DELETE operations
}
```

### 5. SQL Executor / Table Registration

**Find where UserTableProvider is created and registered**

Files to check:
- `backend/crates/kalamdb-sql/src/lib.rs`
- `backend/crates/kalamdb-core/src/services/`

**Required change**:
```rust
// When registering user tables with DataFusion
let provider = UserTableProvider::new(...)
    .with_live_query_manager(Arc::clone(&live_query_manager));

context.register_table(table_name, Arc::new(provider))?;
```

### 6. StreamTableProvider - NO CHANGES NEEDED ✅

Stream tables already implement the correct pattern! Reference implementation at:
- File: `backend/crates/kalamdb-core/src/tables/stream_table_provider.rs`
- Lines: 207-223 (insert_event method)

## Integration Tests Required

### File: `/backend/tests/integration/websocket/test_user_table_notifications.rs`

```rust
/// Test INSERT notification for user tables
#[tokio::test]
async fn test_user_table_insert_notification() {
    // 1. Create user table
    // 2. Subscribe to table via WebSocket
    // 3. INSERT a row
    // 4. Assert: WebSocket receives InsertNotification within 1 second
    // 5. Assert: Notification contains correct row data
}

/// Test UPDATE notification for user tables
#[tokio::test]
async fn test_user_table_update_notification() {
    // 1. Create table and insert row
    // 2. Subscribe to table
    // 3. UPDATE the row
    // 4. Assert: Receives UpdateNotification with new data
}

/// Test DELETE notification for user tables
#[tokio::test]
async fn test_user_table_delete_notification() {
    // 1. Create table and insert row
    // 2. Subscribe to table
    // 3. DELETE the row
    // 4. Assert: Receives DeleteNotification with row_id
}

/// ✅ REQUIREMENT 5: Test bulk INSERT
#[tokio::test]
async fn test_bulk_insert_notifications() {
    // 1. Subscribe to table
    // 2. INSERT 10 rows in single statement
    // 3. Assert: Receive exactly 10 InsertNotification events
    // 4. Assert: All 10 notifications received within 2 seconds
    // 5. Assert: No notifications missing
}

/// ✅ REQUIREMENT 5: Test multiple UPDATE on same row
#[tokio::test]
async fn test_multiple_updates_same_row() {
    // 1. Insert row with id=123
    // 2. Subscribe to table
    // 3. UPDATE id=123 three times rapidly
    // 4. Assert: Receive exactly 3 UpdateNotification events
    // 5. Assert: Each notification has correct sequential data
    // 6. Assert: No updates lost or duplicated
}
```

### File: `/backend/tests/integration/websocket/test_stream_table_notifications.rs`

```rust
/// ✅ REQUIREMENT 4: Verify stream tables still work with same pattern
#[tokio::test]
async fn test_stream_table_insert_notification() {
    // Same tests as user tables but for stream tables
}

/// ✅ REQUIREMENT 5: Test bulk INSERT for stream tables
#[tokio::test]
async fn test_stream_bulk_insert_notifications() {
    // Same bulk insert test as user tables
}

/// ✅ REQUIREMENT 5: Test event ordering preservation
#[tokio::test]
async fn test_stream_event_ordering() {
    // 1. Insert 100 events rapidly
    // 2. Assert: All 100 notifications received
    // 3. Assert: Notifications in correct timestamp order
}
```

## Implementation Priority

1. ✅ **Understand Stream Table Pattern** - DONE (reference implementation identified)
2. **Add LiveQueryManager to UserTableInsertHandler** (highest priority)
3. **Add async notification in insert_row() and insert_batch()**
4. **Add LiveQueryManager to UserTableUpdateHandler**
5. **Add async notification in update_row() and update_batch()**
6. **Add LiveQueryManager to UserTableDeleteHandler**
7. **Add async notification in delete_row() and delete_batch()**
8. **Wire LiveQueryManager through UserTableProvider**
9. **Test with existing CLI WebSocket integration tests**
10. **Add comprehensive backend integration tests** (bulk, ordering, etc.)

## Success Criteria

- ✅ All CLI WebSocket integration tests pass (currently 4 failing)
- ✅ INSERT operations trigger real-time notifications (user + stream tables)
- ✅ UPDATE operations trigger real-time notifications (user + stream tables)
- ✅ DELETE operations trigger real-time notifications (user + stream tables)
- ✅ Bulk INSERT delivers all notifications without loss
- ✅ Multiple updates on same row all delivered
- ✅ No performance degradation on INSERT/UPDATE/DELETE (async fire-and-forget)
- ✅ Storage succeeds even if notification fails (reliability requirement)
- ✅ Stream and user tables use identical notification pattern

## Performance Validation

After implementation, run benchmarks:

```bash
# Baseline: INSERT without subscribers
cargo bench --bench performance -- insert_baseline

# With subscribers: Should be < 5% overhead
cargo bench --bench performance -- insert_with_subscribers

# Bulk operations
cargo bench --bench performance -- bulk_insert
```

Expected results:
- Single INSERT overhead: < 1ms (tokio::spawn is ~10-50 microseconds)
- Bulk INSERT: Linear scaling, no notification backlog
- Storage operation completes in < 1ms regardless of notification delivery

## Notes

- ✅ Notifications are fire-and-forget (don't block write operations)
- ✅ Use `tokio::spawn()` for async, non-blocking delivery
- ✅ Log errors if notification fails, but don't fail the write
- ✅ Clone data before spawning to avoid lifetime issues
- ✅ Identical pattern for stream tables and user tables
- ✅ Test bulk operations and rapid sequential updates

## Required Changes

### 1. Modify User Table Handlers

#### File: `backend/crates/kalamdb-core/src/tables/user_table_insert.rs`

**Change**:
```rust
pub struct UserTableInsertHandler {
    store: Arc<UserTableStore>,
    live_query_manager: Option<Arc<LiveQueryManager>>,  // ADD THIS
}

impl UserTableInsertHandler {
    pub fn new(store: Arc<UserTableStore>, live_query_manager: Option<Arc<LiveQueryManager>>) -> Self {
        Self { store, live_query_manager }
    }
    
    pub async fn insert_row(...) -> Result<String, KalamDbError> {
        // ... existing insert logic ...
        
        // ADD THIS: Broadcast notification after successful insert
        if let Some(manager) = &self.live_query_manager {
            let table_name = format!("{}.{}", namespace_id.as_str(), table_name.as_str());
            let notification = ChangeNotification::insert(table_name, row_data.clone());
            let _ = manager.notify_table_change(&table_name, notification).await;
        }
        
        Ok(row_id)
    }
}
```

#### File: `backend/crates/kalamdb-core/src/tables/user_table_update.rs`

**Change**:
```rust
pub struct UserTableUpdateHandler {
    store: Arc<UserTableStore>,
    live_query_manager: Option<Arc<LiveQueryManager>>,  // ADD THIS
}

impl UserTableUpdateHandler {
    pub async fn update_row(...) -> Result<(), KalamDbError> {
        // Get old data BEFORE update
        let old_data = self.store.get(...)?;
        
        // ... existing update logic ...
        
        // ADD THIS: Broadcast notification with old and new data
        if let Some(manager) = &self.live_query_manager {
            if let Some(old) = old_data {
                let table_name = format!("{}.{}", namespace_id.as_str(), table_name.as_str());
                let notification = ChangeNotification::update(table_name, old, new_data.clone());
                let _ = manager.notify_table_change(&table_name, notification).await;
            }
        }
        
        Ok(())
    }
}
```

#### File: `backend/crates/kalamdb-core/src/tables/user_table_delete.rs`

**Change**:
```rust
pub struct UserTableDeleteHandler {
    store: Arc<UserTableStore>,
    live_query_manager: Option<Arc<LiveQueryManager>>,  // ADD THIS
}

impl UserTableDeleteHandler {
    pub async fn delete_row(...) -> Result<(), KalamDbError> {
        // Get data BEFORE delete
        let row_data = self.store.get(...)?;
        
        // ... existing delete logic ...
        
        // ADD THIS: Broadcast notification with deleted data
        if let Some(manager) = &self.live_query_manager {
            if let Some(data) = row_data {
                let table_name = format!("{}.{}", namespace_id.as_str(), table_name.as_str());
                let notification = ChangeNotification::delete_soft(table_name, data);
                let _ = manager.notify_table_change(&table_name, notification).await;
            }
        }
        
        Ok(())
    }
}
```

### 2. Pass LiveQueryManager to Table Providers

#### File: `backend/crates/kalamdb-core/src/tables/user_table_provider.rs`

**Change**: Accept `live_query_manager` in constructor and pass to insert/update/delete handlers

### 3. Wire Up in SQL Executor

#### File: `backend/crates/kalamdb-core/src/sql/executor.rs`

**Change**: When creating UserTableProvider instances, pass the LiveQueryManager

### 4. Update SessionContext Creation

Ensure LiveQueryManager is available when creating sessions that execute SQL

### 5. WebSocket Message Delivery

#### File: `backend/crates/kalamdb-api/src/websocket/session.rs` (or wherever WebSocket sessions are managed)

**Change**: Implement actual WebSocket message sending when `notify_table_change` is called.

Currently `notify_table_change` only increments counters. It needs to:
1. Find active WebSocket connections for the LiveId
2. Format ChangeNotification as JSON WebSocket message
3. Send via WebSocket to connected client

## Integration Tests Needed

### File: `backend/tests/integration/websocket/test_real_time_notifications.rs` (NEW)

```rust
#[tokio::test]
async fn test_insert_notification_broadcast() {
    // 1. Create WebSocket subscription
    // 2. INSERT a row via SQL
    // 3. Wait for WebSocket message
    // 4. Assert: received ChangeEvent::Insert with correct data
}

#[tokio::test]
async fn test_update_notification_broadcast() {
    // 1. INSERT initial row
    // 2. Create WebSocket subscription
    // 3. UPDATE the row via SQL
    // 4. Wait for WebSocket message
    // 5. Assert: received ChangeEvent::Update with old_rows and rows
}

#[tokio::test]
async fn test_delete_notification_broadcast() {
    // 1. INSERT initial row
    // 2. Create WebSocket subscription
    // 3. DELETE the row via SQL
    // 4. Wait for WebSocket message
    // 5. Assert: received ChangeEvent::Delete with old_rows
}

#[tokio::test]
async fn test_filtered_subscription_only_matches() {
    // 1. Create subscription with WHERE user_id = 'user1'
    // 2. INSERT row with user_id = 'user1' → should receive notification
    // 3. INSERT row with user_id = 'user2' → should NOT receive notification
}

#[tokio::test]
async fn test_multiple_subscribers_all_notified() {
    // 1. Create 3 WebSocket subscriptions to same table
    // 2. INSERT a row
    // 3. Assert: all 3 subscribers receive notification
}
```

## Testing Checklist

- [ ] Unit test: `LiveQueryManager::notify_table_change` filters correctly
- [ ] Integration test: INSERT triggers WebSocket notification
- [ ] Integration test: UPDATE triggers WebSocket notification  
- [ ] Integration test: DELETE triggers WebSocket notification
- [ ] Integration test: Filtered subscriptions only receive matching changes
- [ ] Integration test: Multiple subscribers all receive notifications
- [ ] Integration test: InitialData sent on subscription creation
- [ ] CLI integration tests pass (currently failing)

## Files to Modify

1. `backend/crates/kalamdb-core/src/tables/user_table_insert.rs` - Add notification broadcasting
2. `backend/crates/kalamdb-core/src/tables/user_table_update.rs` - Add notification broadcasting
3. `backend/crates/kalamdb-core/src/tables/user_table_delete.rs` - Add notification broadcasting
4. `backend/crates/kalamdb-core/src/tables/user_table_provider.rs` - Pass LiveQueryManager
5. `backend/crates/kalamdb-core/src/sql/executor.rs` - Wire up LiveQueryManager
6. `backend/crates/kalamdb-api/src/websocket/session.rs` - Implement WebSocket message sending
7. `backend/crates/kalamdb-live/src/manager.rs` - Add WebSocket delivery to notify_table_change
8. `backend/tests/integration/websocket/test_real_time_notifications.rs` - NEW integration tests

## Implementation Priority

1. **High**: Add `live_query_manager` parameter to insert/update/delete handlers
2. **High**: Call `notify_table_change` after successful operations
3. **High**: Implement WebSocket message delivery in notify_table_change
4. **Medium**: Write integration tests
5. **Medium**: Fix InitialData delivery (currently flaky)
6. **Low**: Optimize batch notifications

## Estimated Complexity

- **Code Changes**: Medium (8 files, ~200 lines of code)
- **Testing**: High (comprehensive integration tests needed)
- **Risk**: Low (additive changes, can be feature-flagged)

## Success Criteria

✅ CLI integration tests pass:
- `test_websocket_insert_notification`
- `test_websocket_update_notification`
- `test_websocket_delete_notification`
- `test_websocket_initial_data_snapshot`

✅ Backend integration tests pass:
- All real-time notification tests

✅ Manual testing:
- Bruno/Postman can subscribe and receive live updates
- Multiple clients receive same notifications

---

**Next Steps**:
1. Implement changes in order listed above
2. Write integration tests as changes are made
3. Verify CLI tests pass
4. Deploy to staging for end-to-end testing
