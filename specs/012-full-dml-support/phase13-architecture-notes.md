# Phase 13: Provider Architecture - Design Notes

## Core Architecture Decision: Stateless Providers + SessionState User Context

### The Problem
We needed to determine how user context (user_id, role) should flow to table providers for RLS enforcement without:
1. Creating per-user provider instances (memory overhead)
2. Storing user_id in provider fields (violates single-responsibility)
3. Passing ExecutionContext everywhere (tight coupling)

### The Solution: DataFusion's SessionState Extensions

KalamDB already uses DataFusion's extension system to pass user context from HTTP handlers → providers:

```rust
// ExecutionContext creates per-request SessionContext
pub fn create_session_with_user(&self) -> SessionContext {
    let mut session_state = self.base_session_context.state();
    
    // Inject user context into DataFusion's extension system
    session_state
        .config_mut()
        .options_mut()
        .extensions
        .insert(SessionUserContext {
            user_id: self.user_id.clone(),
            role: self.user_role.clone(),
        });
    
    SessionContext::new_with_state(session_state)
}
```

### Provider Architecture

**Providers are table-scoped and stateless:**
- Registered once in base_session_context (zero-copy per request)
- Hold only table-level metadata (schema, table_id, column_defaults)
- Share services via Arc<TableProviderCore> (AppContext, LiveQueryManager, etc.)

**User context flows per-operation:**

1. **SQL Queries** (via DataFusion):
   ```rust
   // DataFusion calls TableProvider::scan() with SessionState
   fn scan(&self, state: &dyn Session, ...) -> ExecutionPlan {
       // Extract user_id from SessionState extensions
       let (user_id, role) = Self::extract_user_context(state)?;
       
       // Apply RLS filtering
       self.scan_rocksdb_as_batch(&user_id, ...)?;
   }
   ```

2. **DML Operations** (direct calls):
   ```rust
   // SQL executor extracts user_id from ExecutionContext
   let user_id = context.user_id();
   
   // Passes it explicitly to provider methods
   provider.insert(&user_id, row_data)?;
   provider.update(&user_id, &key, updates)?;
   provider.delete(&user_id, &key)?;
   ```

### BaseTableProvider Trait Design

```rust
pub trait BaseTableProvider<K: StorageKey, V>: Send + Sync + TableProvider {
    // Core metadata (table-scoped, no user context)
    fn table_id(&self) -> &TableId;
    fn schema_ref(&self) -> SchemaRef;
    fn table_type(&self) -> TableType;
    fn app_context(&self) -> &Arc<AppContext>;
    fn primary_key_field_name(&self) -> &str;
    
    // DML operations (user_id passed per-operation)
    fn insert(&self, user_id: &UserId, row_data: JsonValue) -> Result<K, KalamDbError>;
    fn update(&self, user_id: &UserId, key: &K, updates: JsonValue) -> Result<K, KalamDbError>;
    fn delete(&self, user_id: &UserId, key: &K) -> Result<(), KalamDbError>;
    
    // Scan operations (extract user_id from SessionState for SQL, or pass directly for DML)
    fn scan_rows(&self, state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError>;
    fn scan_with_version_resolution_to_kvs(&self, user_id: &UserId, filter: Option<&Expr>) -> Result<Vec<(K, V)>, KalamDbError>;
}
```

### Benefits

1. **Memory Efficiency**:
   - Providers registered once, Arc-cloned per request (~1-2μs)
   - No per-user provider instances
   - Shared services via TableProviderCore

2. **Clean Separation**:
   - Executor handles authentication and context
   - Provider handles storage and versioning
   - No tight coupling between layers

3. **AS USER Support**:
   - Executor extracts subject_user_id (for AS USER) or actor_user_id (normal operation)
   - Passes it to provider methods
   - Provider treats all user_ids equally (no special "current user" concept)

4. **Performance**:
   - SessionState clone: ~1-2μs per request
   - At 100K QPS: 10-20% CPU overhead (acceptable)
   - Zero locking (providers are stateless)

5. **DataFusion Integration**:
   - Same provider struct serves both custom DML and DataFusion SQL
   - TableProvider::scan() gets SessionState from DataFusion
   - Direct DML calls pass user_id explicitly

### Shared Tables Behavior

Shared tables ignore the user_id parameter:
```rust
impl BaseTableProvider<SeqId, SharedTableRow> for SharedTableProvider {
    fn insert(&self, _user_id: &UserId, row_data: JsonValue) -> Result<SeqId, KalamDbError> {
        // user_id ignored - no RLS
        unified_dml::insert_shared_table_row(...)
    }
    
    fn scan_rows(&self, state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError> {
        // No user extraction - scan all rows
        scan_with_version_resolution(&self.store, &self.table_id, filter)
    }
}
```

### Stream Tables Behavior

Stream tables use user_id for RLS (similar to User tables):
```rust
impl BaseTableProvider<StreamTableRowId, StreamTableRow> for StreamTableProvider {
    fn insert(&self, user_id: &UserId, row_data: JsonValue) -> Result<StreamTableRowId, KalamDbError> {
        // Uses user_id for per-user event streams
        unified_dml::insert_stream_event(user_id, ...)
    }
    
    fn scan_rows(&self, state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError> {
        // Extract user_id for RLS
        let (user_id, _role) = Self::extract_user_context(state)?;
        
        // Scan hot storage only (ephemeral data) with TTL and RLS filtering
        scan_with_ttl_and_rls(&self.store, &user_id, self.ttl_seconds, filter)
    }
}
```

## Implementation Order

1. **Phase 13.1** (T200-T204): Define BaseTableProvider trait with user_id parameters ✅
2. **Phase 13.2** (T205-T210): Refactor StreamTableStore to MVCC (already complete) ✅
3. **Phase 13.3** (T211-T218): Create providers/ module with new implementations
4. **Phase 13.4** (T219-T225): Migrate call sites to new providers/ module
5. **Phase 13.5** (T226-T232): Finalize StreamTableProvider implementation
6. **Phase 13.6** (T233-T239): Cleanup old code and measure reduction

## Key Takeaway

**NO special current_user_id() method needed in the trait.**

User context flows naturally through:
- DataFusion's SessionState extensions (for SQL queries)
- Explicit user_id parameters (for DML operations)

This keeps providers stateless, table-scoped, and user-agnostic while enabling efficient RLS enforcement.
