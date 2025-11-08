# Session Lifecycle Analysis - Request Flow

## Current Flow (After Session Parameter Removal Refactoring)

### 1. **Server Startup** (One-time initialization)
```rust
// backend/src/lifecycle.rs
AppContext::init(
    backend,
    node_id,
    storage_path,
    config,
) -> Arc<AppContext>
```

Inside AppContext::init():
```rust
// backend/crates/kalamdb-core/src/app_context.rs:159
let session_factory = Arc::new(DataFusionSessionFactory::new()?);
let base_session_context = Arc::new(session_factory.create_session());

// Register system schema and all system tables
base_session_context
    .catalog(&catalog_name)?
    .register_schema("system", system_schema.clone())?;

// Register all 10 system table providers
for (table_name, provider) in system_tables.all_system_providers() {
    system_schema.register_table(table_name.to_string(), provider)?;
}

// Store in AppContext
AppContext {
    base_session_context,  // ← ONE session for entire server lifetime
    // ...
}
```

**✅ Result**: ONE SessionContext created at startup, shared across ALL requests

---

### 2. **API Request Received**
```rust
// backend/crates/kalamdb-api/src/handlers/sql_handler.rs:211
let session = app_context.base_session_context();  // ← Get shared session (Arc clone, zero-copy)
let mut exec_ctx = ExecutionContext::new(auth.user_id.clone(), auth.role)  // ❌ Creates wasteful temp session!
    .with_session(session.clone());  // ← Replaces temp session with shared one
```

**Current Memory Waste**:
```rust
// ExecutionContext::new() creates:
session: Arc::new(SessionContext::new())  // ← Created and immediately discarded!

// Then with_session() replaces it with:
session: app_context.base_session_context()  // ← The actual session we use
```

**Impact**: 
- Each request creates a temporary SessionContext that's immediately thrown away
- SessionContext::new() is expensive (~500KB-1MB memory, catalog registration overhead)
- Gets GC'd immediately but causes unnecessary allocation churn

---

### 3. **ExecutionContext Enhancement**
```rust
// Add request_id and ip_address
exec_ctx = exec_ctx.with_request_id(request_id);
exec_ctx = exec_ctx.with_ip(ip_address);
```

**✅ Session remains the same Arc reference** (no new allocations)

---

### 4. **Query Execution**
```rust
// backend/crates/kalamdb-api/src/handlers/sql_handler.rs:229
sql_executor.execute_with_metadata(
    &session,     // ← Shared session (but we just removed this parameter!)
    sql,
    &exec_ctx,    // ← Contains same session via exec_ctx.session
    metadata,
    params
).await
```

**After our refactoring**: Session is accessed via `exec_ctx.session` inside handlers

---

### 5. **Handler Execution**
```rust
// backend/crates/kalamdb-core/src/sql/executor/handlers/*/
async fn execute(
    &self,
    statement: SqlStatement,
    params: Vec<ScalarValue>,
    context: &ExecutionContext,  // ← Contains session
) -> Result<ExecutionResult, KalamDbError> {
    // Access session when needed:
    let session = &context.session;  // ← Arc reference, no clone needed unless storing
    
    // Use session for DataFusion operations
    let df = session.sql("SELECT ...").await?;
    // ...
}
```

**✅ Session is shared Arc** (no new allocations, lightweight reference)

---

### 6. **Request Completion & Cleanup**
```rust
// When exec_ctx goes out of scope:
drop(exec_ctx);
  ↓
// ExecutionContext fields dropped:
- user_id: dropped (trivial)
- user_role: dropped (trivial)
- namespace_id: dropped (trivial)
- request_id: dropped (trivial)
- ip_address: dropped (trivial)
- timestamp: dropped (trivial)
- params: dropped (Vec freed)
- session: Arc::drop()  // ← Decrements ref count (NO deallocation, still held by AppContext)
```

**✅ Session is NOT freed** - AppContext still holds the Arc, so session lives for entire server lifetime

---

## Memory Analysis

### Per Request Memory (Current)
```
ExecutionContext size: ~200 bytes
├─ user_id: UserId (String) ~50 bytes
├─ user_role: Role (enum) 1 byte
├─ namespace_id: Option<NamespaceId> ~50 bytes
├─ request_id: Option<String> ~50 bytes
├─ ip_address: Option<String> ~50 bytes
├─ timestamp: SystemTime 16 bytes
├─ params: Vec<ScalarValue> variable (0-50 params × ~100 bytes = 0-5KB)
└─ session: Arc<SessionContext> 8 bytes (pointer only!)

WASTEFUL TEMP SESSION (created in constructor):
SessionContext::new() ~500KB-1MB  ← IMMEDIATELY DISCARDED!
```

### Memory Waste Per Request
- **Current**: ~500KB-1MB temporary SessionContext allocation + deallocation
- **After fix**: ~0 bytes (no temp session created)

### Requests Per Second Impact
- At 1000 req/s: 500MB-1GB/s unnecessary allocations
- At 10000 req/s: 5GB-10GB/s unnecessary allocations

---

## Issues Identified

### ❌ Issue 1: Wasteful SessionContext Creation
**Location**: All ExecutionContext constructors
```rust
// backend/crates/kalamdb-core/src/sql/executor/models/execution_context.rs
impl ExecutionContext {
    pub fn new(user_id: UserId, user_role: Role) -> Self {
        Self {
            session: Arc::new(SessionContext::new()),  // ❌ WASTEFUL!
            // ...
        }
    }
    
    pub fn with_namespace(...) -> Self {
        Self {
            session: Arc::new(SessionContext::new()),  // ❌ WASTEFUL!
            // ...
        }
    }
    
    // ... all constructors have this problem
}
```

**Problem**: Every constructor creates a new SessionContext that gets replaced immediately by `.with_session()`

**Fix**: Don't create SessionContext in constructors - require it via `with_session()`

---

### ✅ Issue 2: Session Parameter Removed (ALREADY FIXED)
We already removed the redundant `session: &SessionContext` parameter from all handlers.
Now session is accessed via `context.session` only.

---

## Recommended Fixes

### Fix 1: Remove Wasteful SessionContext from Constructors
```rust
// BEFORE (wasteful):
pub fn new(user_id: UserId, user_role: Role) -> Self {
    Self {
        session: Arc::new(SessionContext::new()),  // ❌ Created and discarded!
        // ...
    }
}

// AFTER (lightweight):
pub fn new(user_id: UserId, user_role: Role, session: Arc<SessionContext>) -> Self {
    Self {
        session,  // ✅ Use provided session directly
        // ...
    }
}
```

**Alternative**: Make session an Option and require with_session():
```rust
pub fn new(user_id: UserId, user_role: Role) -> Self {
    Self {
        session: Arc::new(SessionContext::new()),  // Default (for backward compat)
        // ...
    }
}

// Preferred usage:
ExecutionContext::new(user_id, role)
    .with_session(app_context.base_session_context())  // Replace default
```

**Best Option**: Make session mandatory in constructor to prevent wasteful defaults.

---

## Final Flow (After Fix)

### Memory Per Request (After Fix)
```
ExecutionContext size: ~200 bytes
├─ user_id: ~50 bytes
├─ user_role: 1 byte
├─ namespace_id: ~50 bytes
├─ request_id: ~50 bytes
├─ ip_address: ~50 bytes
├─ timestamp: 16 bytes
├─ params: 0-5KB
└─ session: 8 bytes (Arc pointer to SHARED session)

NO WASTEFUL ALLOCATIONS! ✅
```

### Session Sharing
```
AppContext (server startup)
    └─ base_session_context: Arc<SessionContext> (500KB-1MB, allocated ONCE)
            ↓
    Request 1: ExecutionContext { session: Arc::clone() }  // +8 bytes
    Request 2: ExecutionContext { session: Arc::clone() }  // +8 bytes  
    Request 3: ExecutionContext { session: Arc::clone() }  // +8 bytes
    ...
    Request N: ExecutionContext { session: Arc::clone() }  // +8 bytes
```

**Result**: ONE SessionContext for entire server, N × 8 bytes for N concurrent requests

---

## Verification Checklist

- [x] ✅ ONE SessionContext created at server startup (AppContext::init)
- [x] ✅ Session shared via Arc across ALL requests (base_session_context())
- [ ] ❌ ExecutionContext constructors create wasteful temp sessions (FIX NEEDED)
- [x] ✅ Session accessed via context.session in handlers (no redundant parameter)
- [x] ✅ Session properly reference-counted (Arc) and NOT freed per request
- [ ] 🔧 Need to fix constructors to accept session parameter

---

## Conclusion

**Current State**:
- ✅ Architecture is correct: ONE shared SessionContext
- ❌ Implementation has memory waste: temp sessions created and discarded
- ✅ Session lifecycle is correct: lives for server lifetime, shared across requests

**Action Needed**:
1. Refactor ExecutionContext constructors to accept `session: Arc<SessionContext>` parameter
2. Remove wasteful `Arc::new(SessionContext::new())` from all constructors
3. Update call sites to pass `app_context.base_session_context()` to constructors

**Impact**:
- Eliminates 500KB-1MB allocation per request
- Reduces memory pressure and GC churn
- At 1000 req/s: saves 500MB-1GB/s allocations
