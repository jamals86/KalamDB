# Data Model: SQL Handlers Prep

**Feature**: 011-sql-handlers-prep  
**Date**: 2025-11-07  
**Phase**: 1 (Design & Contracts)

## Core Entities

### ExecutionContext

**Purpose**: Carries request-scoped context through handler execution pipeline

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| user_id | UserId | required | Authenticated user identifier from JWT/Basic auth |
| role | UserRole | required | User role (User, Service, Dba, System) for RBAC |
| namespace_id | Option<NamespaceId> | optional | Default namespace for unqualified table names |
| request_id | String | required, UUID format | Unique request identifier for tracing/logging |
| ip_address | Option<IpAddr> | optional | Client IP address for audit logging |
| timestamp | DateTime<Utc> | required | Request timestamp for audit trail |
| params | Vec<ScalarValue> | max 50 elements, 512KB per element | Query parameters ($1, $2, ...) |
| session | Arc<SessionContext> | required | DataFusion session for query execution |

**Validation Rules**:
- `params.len()` ≤ 50
- Each `params[i].size()` ≤ 524,288 bytes (512KB)
- `request_id` must be valid UUID v4
- `timestamp` must not be in future

**Lifecycle**: Created once per HTTP request, passed by reference through handler chain

**Usage Example**:
```rust
let ctx = ExecutionContext {
    user_id: UserId::from(auth.user_id),
    role: auth.role,
    namespace_id: Some(NamespaceId::from("default")),
    request_id: Uuid::new_v4().to_string(),
    ip_address: Some(request.peer_addr),
    timestamp: Utc::now(),
    params: vec![
        ScalarValue::Int64(Some(123)),
        ScalarValue::Utf8(Some("Alice".to_string())),
    ],
    session: Arc::new(SessionContext::new()),
};
```

---

### ExecutionResult

**Purpose**: Standardized response from handler execution

**Variants**:
| Variant | Fields | Description |
|---------|--------|-------------|
| Success | message: String | Generic success (e.g., namespace created) |
| Rows | batches: Vec<RecordBatch>, row_count: usize | Query results (SELECT, SHOW) |
| Inserted | rows_affected: usize | INSERT result |
| Updated | rows_affected: usize | UPDATE result |
| Deleted | rows_affected: usize | DELETE result |
| Flushed | tables: Vec<String>, bytes_written: u64 | FLUSH result |
| Subscription | subscription_id: String, channel: String | LIVE SELECT result |
| JobKilled | job_id: String, status: String | KILL JOB result |

**Conversion to HTTP Response**:
```rust
impl ExecutionResult {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ExecutionResult::Success { message } => json!({
                "status": "success",
                "message": message
            }),
            ExecutionResult::Rows { batches, row_count } => json!({
                "status": "success",
                "data": batches_to_json(batches),
                "row_count": row_count
            }),
            ExecutionResult::Inserted { rows_affected } => json!({
                "status": "success",
                "operation": "insert",
                "rows_affected": rows_affected
            }),
            // ... other variants
        }
    }
}
```

---

### ErrorResponse

**Purpose**: Structured error format for API responses

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| code | String | UPPERCASE_SNAKE_CASE | Machine-readable error code (e.g., PARAM_COUNT_MISMATCH) |
| message | String | required | Human-readable error description |
| details | serde_json::Value | optional | Contextual error details (expected/actual values, etc.) |
| request_id | String | UUID format | Request identifier for tracing |
| timestamp | DateTime<Utc> | required | Error occurrence time |

**Error Code Taxonomy**:
```rust
pub enum ErrorCode {
    // Parameter validation (400)
    ParamCountMismatch,
    ParamSizeExceeded,
    ParamTypeM ismatch,
    ParamsNotSupported,
    
    // Authorization (403)
    AuthInsufficientRole,
    AuthNamespaceAccessDenied,
    
    // Resource not found (404)
    NotFoundTable,
    NotFoundNamespace,
    NotFoundUser,
    NotFoundStorage,
    
    // Timeout (408)
    TimeoutHandlerExecution,
    TimeoutQueryPlanning,
    
    // Not implemented (501)
    NotImplementedTransaction,
    NotImplementedFeature,
    
    // Internal errors (500)
    InternalDatafusionError,
    InternalStorageError,
}
```

**Usage Example**:
```rust
let error = ErrorResponse {
    code: "PARAM_COUNT_MISMATCH".to_string(),
    message: "Parameter count mismatch: expected 2 parameters, got 3".to_string(),
    details: json!({
        "expected": 2,
        "actual": 3,
        "placeholders": ["$1", "$2"]
    }),
    request_id: ctx.request_id.clone(),
    timestamp: Utc::now(),
};
```

---

### TypedStatementHandler<T>

**Purpose**: Trait for type-safe handler implementations

**Interface**:
```rust
#[async_trait]
pub trait TypedStatementHandler<T: DdlAst>: Send + Sync {
    /// Execute the statement with provided context
    async fn execute(
        &self, 
        stmt: &T, 
        ctx: &ExecutionContext
    ) -> Result<ExecutionResult, KalamDbError>;
    
    /// Check if user is authorized to execute this statement
    fn check_authorization(&self, ctx: &ExecutionContext) -> Result<(), KalamDbError>;
}
```

**Type Parameter Constraint**:
- `T: DdlAst` ensures only SQL AST types can be used
- Compile-time safety: Can't accidentally pass wrong statement type

**Concrete Implementations** (28 total):
- CreateNamespaceHandler ✅ (complete)
- AlterNamespaceHandler, DropNamespaceHandler, ShowNamespacesHandler
- CreateStorageHandler, AlterStorageHandler, DropStorageHandler, ShowStoragesHandler
- CreateTableHandler, AlterTableHandler, DropTableHandler, ShowTablesHandler, DescribeTableHandler, ShowStatsHandler
- InsertHandler, UpdateHandler, DeleteHandler
- FlushTableHandler, FlushAllTablesHandler
- KillJobHandler, KillLiveQueryHandler
- SubscribeHandler
- CreateUserHandler, AlterUserHandler, DropUserHandler
- BeginTransactionHandler, CommitTransactionHandler, RollbackTransactionHandler

---

### SqlStatementHandler (Trait Object)

**Purpose**: Polymorphic handler interface for registry storage

**Interface**:
```rust
#[async_trait]
pub trait SqlStatementHandler: Send + Sync {
    /// Handle any SqlStatement variant (via dynamic dispatch)
    async fn handle(
        &self,
        stmt: &SqlStatement,
        ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>;
}
```

**Implementation via TypedHandlerAdapter**:
```rust
pub struct TypedHandlerAdapter<H, T, F> {
    handler: H,                    // Concrete handler (e.g., CreateNamespaceHandler)
    extractor: F,                  // Closure: SqlStatement -> Option<&T>
    _phantom: PhantomData<T>,      // Zero-sized type marker
}

#[async_trait]
impl<H, T, F> SqlStatementHandler for TypedHandlerAdapter<H, T, F>
where
    H: TypedStatementHandler<T>,
    T: DdlAst,
    F: Fn(&SqlStatement) -> Option<&T> + Send + Sync,
{
    async fn handle(&self, stmt: &SqlStatement, ctx: &ExecutionContext) 
        -> Result<ExecutionResult, KalamDbError> 
    {
        // Extract concrete type from SqlStatement enum
        let concrete_stmt = (self.extractor)(stmt)
            .ok_or_else(|| KalamDbError::StatementTypeMismatch)?;
        
        // Delegate to typed handler
        self.handler.execute(concrete_stmt, ctx).await
    }
}
```

---

### HandlerRegistry

**Purpose**: Central registry for handler lookup and dispatch

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| handlers | DashMap<Discriminant<SqlStatement>, Arc<dyn SqlStatementHandler>> | Lock-free concurrent map of handlers |
| app_context | Arc<AppContext> | Shared application context |

**Key Methods**:
```rust
impl HandlerRegistry {
    /// Register a typed handler with zero boilerplate
    pub fn register_typed<H, T, F>(
        &mut self,
        handler: H,
        extractor: F,
    ) where
        H: TypedStatementHandler<T> + 'static,
        T: DdlAst + 'static,
        F: Fn(&SqlStatement) -> Option<&T> + Send + Sync + 'static,
    {
        let adapter = TypedHandlerAdapter {
            handler,
            extractor,
            _phantom: PhantomData,
        };
        
        let discriminant = discriminant(&T::placeholder());
        self.handlers.insert(discriminant, Arc::new(adapter));
    }
    
    /// Dispatch statement to registered handler
    pub async fn handle(
        &self,
        stmt: &SqlStatement,
        ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let key = discriminant(stmt);
        let handler = self.handlers.get(&key)
            .ok_or_else(|| KalamDbError::NoHandlerFound {
                statement_type: stmt.type_name(),
            })?;
        
        handler.handle(stmt, ctx).await
    }
    
    /// Check if handler is registered for statement type
    pub fn has_handler(&self, stmt: &SqlStatement) -> bool {
        let key = discriminant(stmt);
        self.handlers.contains_key(&key)
    }
}
```

---

## State Transitions

### Transaction State (Phase 11 - Not Implemented Yet)

**States**: None, Idle, Active, Committed, RolledBack

```
[None] ─(BEGIN)─> [Active]
[Active] ─(COMMIT)─> [Committed] ─> [Idle]
[Active] ─(ROLLBACK)─> [RolledBack] ─> [Idle]
[Active] ─(timeout)─> [RolledBack] ─> [Idle]

Current behavior (Phase 11 placeholder):
[None] ─(BEGIN)─> [NotImplementedError]
[None] ─(COMMIT)─> [NotImplementedError]
[None] ─(ROLLBACK)─> [NotImplementedError]
```

**Note**: All transaction handlers currently return `KalamDbError::NotImplemented` until transaction manager is implemented in Phase 11.

---

### Handler Execution State

**States**: Pending, Validating, Executing, Completed, Failed, TimedOut

```
[Pending] ─(validate params)─> [Validating]
[Validating] ─(params valid)─> [Executing]
[Validating] ─(params invalid)─> [Failed]
[Executing] ─(success)─> [Completed]
[Executing] ─(error)─> [Failed]
[Executing] ─(timeout)─> [TimedOut]
```

**Timeout Behavior**:
- Default: 30 seconds (configurable via config.toml)
- On timeout: Partial results discarded, error returned
- Error code: TIMEOUT_HANDLER_EXECUTION
- Details include: elapsed time, statement type, request ID

---

## Relationships

### Entity Relationship Diagram

```
┌─────────────────┐
│ ExecutionContext│───┐
│  - user_id      │   │
│  - role         │   │
│  - params[]     │   │
│  - session      │   │
└─────────────────┘   │
                      │ passed to
                      ▼
┌─────────────────────────────────┐
│    HandlerRegistry              │
│  - handlers: DashMap            │◄──┐
└─────────────────────────────────┘   │
                │                      │
                │ dispatches to        │ contains
                ▼                      │
┌─────────────────────────────────┐   │
│  TypedHandlerAdapter<H,T,F>     │───┘
│   - handler: H                  │
│   - extractor: F                │
└─────────────────────────────────┘
                │
                │ delegates to
                ▼
┌─────────────────────────────────┐
│  TypedStatementHandler<T>       │
│   - execute(stmt, ctx)          │
│   - check_authorization(ctx)    │
└─────────────────────────────────┘
                │
                │ returns
                ▼
┌─────────────────────────────────┐
│    ExecutionResult              │
│  Variants:                      │
│  - Success                      │
│  - Rows                         │
│  - Inserted/Updated/Deleted     │
│  - Flushed                      │
│  - Subscription                 │
│  - JobKilled                    │
└─────────────────────────────────┘

         OR (on error)

┌─────────────────────────────────┐
│    ErrorResponse                │
│  - code                         │
│  - message                      │
│  - details                      │
│  - request_id                   │
│  - timestamp                    │
└─────────────────────────────────┘
```

---

## Validation Rules Summary

### ExecutionContext
- ✅ `params.len()` ≤ 50
- ✅ Each `params[i].size()` ≤ 512KB
- ✅ `request_id` is valid UUID v4
- ✅ `timestamp` not in future

### SqlStatement + Parameters
- ✅ Parameters only allowed for: SELECT, INSERT, UPDATE, DELETE
- ✅ Other statement types with params → `PARAMS_NOT_SUPPORTED` error
- ✅ Placeholder count must match params.len()

### Handler Execution
- ✅ Authorization checked before execution
- ✅ Timeout enforced (default 30s, configurable)
- ✅ Partial results discarded on timeout/error

### Error Responses
- ✅ `code` in UPPERCASE_SNAKE_CASE format
- ✅ `message` non-empty, human-readable
- ✅ `details` contains contextual info (expected/actual/elapsed)
- ✅ `request_id` matches ExecutionContext.request_id

---

## Schema Migrations

**N/A** - This feature is a refactoring with no database schema changes.

Changes are code-level only:
- Execution models moved to `sql/executor/models/`
- Handler utilities moved to `sql/executor/helpers/`
- New handler files created in category subdirectories
- HandlerRegistry replaces match statement in executor

No data migration scripts needed.

---

## Performance Considerations

### Memory Usage
- ExecutionContext: ~200 bytes + params (max 50 × 512KB = 25MB worst case)
- HandlerRegistry: 8 bytes per registered handler (28 handlers = 224 bytes)
- Error responses: ~500 bytes avg with details

### CPU Usage
- Handler dispatch: O(1) lookup via DashMap (<2μs)
- Parameter validation: O(n) size checks (n ≤ 50, ~10μs)
- Timeout enforcement: tokio::time::timeout (negligible overhead)

### Scalability
- Concurrent requests: Limited by tokio runtime (100K+ concurrent tasks supported)
- Handler registration: One-time cost at startup
- DashMap sharding: Scales linearly with CPU cores

---

## Testing Strategy

### Unit Tests (per handler)
```rust
#[tokio::test]
async fn test_create_namespace_success() {
    let handler = CreateNamespaceHandler::new(app_context);
    let stmt = CreateNamespaceStatement { name: "test".into() };
    let ctx = create_test_context(UserRole::Dba);
    
    let result = handler.execute(&stmt, &ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_namespace_authorization() {
    let handler = CreateNamespaceHandler::new(app_context);
    let ctx = create_test_context(UserRole::User); // Non-admin
    
    let result = handler.check_authorization(&ctx);
    assert!(matches!(result, Err(KalamDbError::AuthInsufficientRole { .. })));
}
```

### Integration Tests (per category)
```rust
#[tokio::test]
async fn test_ddl_handlers_end_to_end() {
    // Test CREATE NAMESPACE via REST API
    let response = client.post("/v1/api/sql")
        .json(&json!({"sql": "CREATE NAMESPACE test"}))
        .send().await?;
    assert_eq!(response.status(), 200);
    
    // Test SHOW NAMESPACES
    let response = client.post("/v1/api/sql")
        .json(&json!({"sql": "SHOW NAMESPACES"}))
        .send().await?;
    let rows = response.json::<ExecutionResult>().await?;
    assert!(rows.contains("test"));
}
```

### Parameter Validation Tests
```rust
#[tokio::test]
async fn test_param_count_exceeded() {
    let params = vec![ScalarValue::Int64(Some(1)); 51]; // 51 params
    let error = validate_params(&params).unwrap_err();
    assert_eq!(error.code(), "PARAM_COUNT_EXCEEDED");
}

#[tokio::test]
async fn test_param_size_exceeded() {
    let large_param = ScalarValue::Utf8(Some("x".repeat(600_000))); // 600KB
    let error = validate_params(&[large_param]).unwrap_err();
    assert_eq!(error.code(), "PARAM_SIZE_EXCEEDED");
}
```

---

## Summary

**Core Entities**: ExecutionContext, ExecutionResult, ErrorResponse, TypedStatementHandler, HandlerRegistry

**Key Relationships**:
- ExecutionContext → HandlerRegistry → TypedHandlerAdapter → TypedStatementHandler → ExecutionResult/ErrorResponse
- One ExecutionContext per HTTP request
- One HandlerRegistry per application (singleton)
- 28 TypedStatementHandler implementations (one per SQL statement type)

**Validation Rules**: Parameter limits (50 count, 512KB size), timeout (30s default), authorization checks, error code format

**State Transitions**: Handler execution (Pending → Validating → Executing → Completed/Failed/TimedOut), Transaction state (placeholder, not implemented)

**Performance**: O(1) handler dispatch, <2μs overhead, 25MB max memory per request

**Testing**: Unit tests per handler (2+ per handler), integration tests per category (7 categories), parameter validation tests
