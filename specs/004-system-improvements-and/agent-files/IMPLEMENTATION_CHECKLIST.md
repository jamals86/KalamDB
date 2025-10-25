# WebSocket Notification Implementation Checklist

## Quick Reference

This checklist provides step-by-step implementation guidance following the stream table pattern (already working ✅).

## Reference Pattern (Stream Tables - WORKING)

**File**: `backend/crates/kalamdb-core/src/tables/stream_table_provider.rs:207-223`

```rust
// ✅ CORRECT PATTERN - Copy this for user tables!
if let Some(live_query_manager) = &self.live_query_manager {
    let notification = ChangeNotification::insert(
        self.table_name().as_str().to_string(),
        row_data.clone(),
    );

    // Async fire-and-forget (non-blocking)
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

## Implementation Steps

### Step 1: UserTableInsertHandler

**File**: `backend/crates/kalamdb-core/src/tables/user_table_insert.rs`

- [ ] **Line ~30**: Add field to struct
  ```rust
  live_query_manager: Option<Arc<LiveQueryManager>>,
  ```

- [ ] **Line ~40**: Update `new()` constructor
  ```rust
  Self { store, live_query_manager: None }
  ```

- [ ] **Line ~45**: Add builder method
  ```rust
  pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
      self.live_query_manager = Some(manager);
      self
  }
  ```

- [ ] **Line ~90**: Add notification in `insert_row()` AFTER `self.store.put()`
  ```rust
  // RocksDB write (existing)
  self.store.put(...)?;
  
  // Async notification (NEW)
  if let Some(manager) = &self.live_query_manager {
      let notification = ChangeNotification::insert(
          table_name.as_str().to_string(),
          row_data.clone(),
      );
      
      let mgr = Arc::clone(manager);
      let tname = table_name.as_str().to_string();
      tokio::spawn(async move {
          if let Err(e) = mgr.notify_table_change(&tname, notification).await {
              log::warn!("Failed to notify subscribers for INSERT: {}", e);
          }
      });
  }
  
  Ok(row_id)
  ```

- [ ] **Line ~140**: Add notification in `insert_batch()` for EACH row
  ```rust
  for row_data in rows {
      let row_id = self.generate_row_id()?;
      self.store.put(...)?;
      
      // Notify for EACH inserted row
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
  ```

- [ ] **Add import at top of file**
  ```rust
  use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
  ```

### Step 2: UserTableUpdateHandler

**File**: `backend/crates/kalamdb-core/src/tables/user_table_update.rs`

- [ ] **Line ~20**: Add field
  ```rust
  live_query_manager: Option<Arc<LiveQueryManager>>,
  ```

- [ ] **Line ~30**: Update constructor
  ```rust
  Self { store, live_query_manager: None }
  ```

- [ ] **Line ~35**: Add builder method
  ```rust
  pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
      self.live_query_manager = Some(manager);
      self
  }
  ```

- [ ] **Line ~110**: Add notification in `update_row()` AFTER `self.store.put()`
  ```rust
  // RocksDB write (existing)
  self.store.put(...)?;
  
  // Async notification (NEW)
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
  ```

- [ ] **Add import at top of file**
  ```rust
  use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
  ```

### Step 3: UserTableDeleteHandler

**File**: `backend/crates/kalamdb-core/src/tables/user_table_delete.rs`

- [ ] **Add field, constructor, builder method** (same pattern as INSERT/UPDATE)

- [ ] **Add notification in `delete_row()` AFTER delete operation**
  ```rust
  // RocksDB delete (existing)
  self.store.delete(...)?;
  
  // Async notification (NEW)
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
  
  Ok(row_id.to_string())
  ```

- [ ] **Add import at top of file**

### Step 4: UserTableProvider

**File**: `backend/crates/kalamdb-core/src/tables/user_table_provider.rs`

- [ ] **Check if `live_query_manager` field exists** (might already be there)
  ```rust
  pub struct UserTableProvider {
      // ... existing fields ...
      live_query_manager: Option<Arc<LiveQueryManager>>,  // Add if missing
  }
  ```

- [ ] **Add builder method if missing**
  ```rust
  pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
      self.live_query_manager = Some(manager);
      self
  }
  ```

- [ ] **Update `insert_into()` method** to pass manager to handler
  ```rust
  async fn insert_into(&self, ...) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
      let mut insert_handler = UserTableInsertHandler::new(self.store.clone());
      
      // Pass LiveQueryManager if available
      if let Some(manager) = &self.live_query_manager {
          insert_handler = insert_handler.with_live_query_manager(Arc::clone(manager));
      }
      
      // ... rest of insert logic ...
  }
  ```

- [ ] **Update UPDATE and DELETE operations** similarly

### Step 5: Wire Through SQL Executor

**Files to check**:
- `backend/crates/kalamdb-sql/src/lib.rs`
- `backend/crates/kalamdb-core/src/services/table_service.rs` (or similar)

- [ ] **Find where UserTableProvider is created**
  ```rust
  // Search for: UserTableProvider::new
  ```

- [ ] **Pass LiveQueryManager during table registration**
  ```rust
  let provider = UserTableProvider::new(...)
      .with_live_query_manager(Arc::clone(&live_query_manager));
  
  context.register_table(table_name, Arc::new(provider))?;
  ```

## Testing Steps

### Quick Test (CLI Integration Tests)

```bash
cd cli
cargo test --test test_websocket_integration -- --test-threads=1
```

**Expected**: All 4 previously failing tests now pass ✅

### Backend Integration Tests

```bash
cd backend
cargo test --test test_user_table_notifications -- --test-threads=1
cargo test --test test_stream_table_notifications -- --test-threads=1
```

**Expected**: All bulk insert, rapid update, performance tests pass ✅

## Verification Checklist

- [ ] CLI WebSocket tests pass (4 tests that were failing)
- [ ] INSERT triggers notifications
- [ ] UPDATE triggers notifications
- [ ] DELETE triggers notifications
- [ ] Bulk INSERT delivers all notifications (no loss)
- [ ] Multiple UPDATE on same row delivers all notifications
- [ ] Performance overhead < 5%
- [ ] Storage succeeds even if notification fails
- [ ] Stream tables still work (no regression)

## Common Pitfalls

❌ **Don't block the main operation waiting for notification**
```rust
// WRONG - blocks INSERT
manager.notify_table_change(&table_name, notification).await?;
```

✅ **Do use tokio::spawn for async fire-and-forget**
```rust
// CORRECT - non-blocking
tokio::spawn(async move {
    if let Err(e) = manager.notify_table_change(&table_name, notification).await {
        log::warn!("Failed: {}", e);
    }
});
```

---

❌ **Don't propagate notification errors to caller**
```rust
// WRONG - INSERT fails if notification fails
manager.notify_table_change(&table_name, notification).await?;
```

✅ **Do log errors but continue**
```rust
// CORRECT - INSERT succeeds regardless
if let Err(e) = manager.notify_table_change(...).await {
    log::warn!("Notification failed: {}", e);
}
```

---

❌ **Don't forget to clone data before spawning**
```rust
// WRONG - lifetime issues
tokio::spawn(async move {
    manager.notify_table_change(&table_name, notification).await  // ❌ &table_name borrowed
});
```

✅ **Do clone before spawn**
```rust
// CORRECT
let tname = table_name.as_str().to_string();  // Clone
tokio::spawn(async move {
    manager.notify_table_change(&tname, notification).await  // ✅ Owned value
});
```

## Performance Notes

- `tokio::spawn()` overhead: ~10-50 microseconds
- Target: INSERT latency < 1ms (mostly RocksDB write time)
- With 5 subscribers: overhead should be < 5%
- Notification delivery: async, happens after INSERT returns

## Next Steps After Implementation

1. Run all tests to verify functionality
2. Run performance benchmarks
3. Update documentation
4. Create PR with reference to this checklist
5. Mark tasks complete in `specs/002-simple-kalamdb/tasks.md`
