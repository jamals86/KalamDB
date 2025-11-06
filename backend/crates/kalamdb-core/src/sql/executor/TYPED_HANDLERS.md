# Typed Handler Pattern - Complete Example

This demonstrates the new typed handler architecture for KalamDB SQL execution.

## Architecture Overview

```
SQL String
    ↓
SqlStatement::classify() [FAST - string ops only]
    ↓
match classified {
    SELECT/INSERT/DELETE → DataFusion (99% of queries)
    CREATE NAMESPACE → Parse once → TypedHandler<CreateNamespaceStatement>
    CREATE TABLE → Parse once → TypedHandler<CreateTableStatement>
    ... other DDL/commands
}
```

## Key Benefits

1. **Single Parse**: Each DDL statement parsed exactly once (not pre-checked then re-parsed)
2. **Fast Path**: SELECT/DML queries bypass DDL parsing entirely
3. **Type Safety**: Handlers receive strongly-typed AST structs, not raw SQL strings
4. **Extensibility**: Add new handlers by implementing `TypedStatementHandler<T>`

## Code Example

### 1. Classifier (kalamdb-sql)
```rust
pub enum SqlStatement {
    Select,
    Insert,
    CreateNamespace,
    CreateTable,
    // ... more variants
}
```

### 2. Parent Trait (kalamdb-sql)
```rust
pub trait DdlAst: Debug + Send + Sync {}

impl DdlAst for CreateNamespaceStatement {}
impl DdlAst for CreateTableStatement {}
// ... all DDL statements implement this
```

### 3. Typed Handler Trait (kalamdb-core)
```rust
#[async_trait]
pub trait TypedStatementHandler<T: DdlAst>: Send + Sync {
    async fn execute(
        &self,
        session: &SessionContext,
        statement: T,              // ← Strongly typed!
        params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>;
    
    async fn check_authorization(
        &self,
        statement: &T,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError>;
}
```

### 4. Concrete Handler (kalamdb-core)
```rust
pub struct CreateNamespaceHandler {
    app_context: Arc<AppContext>,
}

#[async_trait]
impl TypedStatementHandler<CreateNamespaceStatement> for CreateNamespaceHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        statement: CreateNamespaceStatement,  // ← Already parsed!
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let name = statement.name.as_str();
        let if_not_exists = statement.if_not_exists;
        
        // Business logic using parsed statement fields
        // ...
    }
}
```

### 5. Executor Routing (kalamdb-core)
```rust
pub async fn execute_with_metadata(...) -> Result<ExecutionResult, KalamDbError> {
    // Step 1: Classify (fast string operations)
    let classified = SqlStatement::classify(sql);
    
    // Step 2: Check authorization
    AuthorizationHandler::check_authorization(exec_ctx, &classified)?;
    
    // Step 3: Route to handler
    match classified {
        // Hot path: 99% of queries
        SqlStatement::Select | SqlStatement::Insert | SqlStatement::Delete => {
            self.execute_via_datafusion(session, sql, params).await
        }
        
        // DDL: Parse once, dispatch to typed handler
        SqlStatement::CreateNamespace => {
            let stmt = CreateNamespaceStatement::parse(sql)
                .map_err(|e| KalamDbError::InvalidSql(format!("...: {}", e)))?;
            
            let handler = CreateNamespaceHandler::new(self.app_context.clone());
            handler.check_authorization(&stmt, exec_ctx).await?;
            handler.execute(session, stmt, params, exec_ctx).await
        }
        
        // ... more DDL variants
    }
}
```

## Adding New Typed Handlers

To add support for a new statement type (e.g., DROP NAMESPACE):

1. **Ensure Parser Exists** (kalamdb-sql)
   ```rust
   // Already exists: src/ddl/drop_namespace.rs
   pub struct DropNamespaceStatement { ... }
   impl DropNamespaceStatement {
       pub fn parse(sql: &str) -> DdlResult<Self> { ... }
   }
   ```

2. **Implement DdlAst** (kalamdb-sql/src/ddl_parent.rs)
   ```rust
   impl DdlAst for DropNamespaceStatement {}
   ```

3. **Create Typed Handler** (kalamdb-core/src/sql/executor/handlers/ddl_typed.rs)
   ```rust
   pub struct DropNamespaceHandler {
       app_context: Arc<AppContext>,
   }
   
   #[async_trait]
   impl TypedStatementHandler<DropNamespaceStatement> for DropNamespaceHandler {
       async fn execute(..., statement: DropNamespaceStatement, ...) { ... }
       async fn check_authorization(&self, statement: &DropNamespaceStatement, ...) { ... }
   }
   ```

4. **Add Match Arm** (kalamdb-core/src/sql/executor/mod.rs)
   ```rust
   SqlStatement::DropNamespace => {
       let stmt = DropNamespaceStatement::parse(sql)?;
       let handler = DropNamespaceHandler::new(self.app_context.clone());
       handler.check_authorization(&stmt, exec_ctx).await?;
       handler.execute(session, stmt, params, exec_ctx).await
   }
   ```

## Testing

Tests demonstrate three key scenarios:

```rust
#[tokio::test]
async fn test_typed_handler_create_namespace() {
    // Verifies typed handler receives parsed AST
}

#[tokio::test]
async fn test_typed_handler_authorization() {
    // Verifies authorization happens before execution
}

#[tokio::test]
async fn test_classifier_prioritizes_select() {
    // Verifies SELECT queries bypass DDL parsing (fast path)
}
```

Run: `cargo test --test test_typed_handlers`

## Performance Characteristics

- **Classifier**: ~10-50ns (string prefix matching)
- **DDL Parsing**: ~1-5μs (only once per DDL statement)
- **SELECT/DML**: Zero DDL parsing overhead (direct DataFusion)

## Migration Path

Existing handlers in `ddl.rs` can be migrated incrementally:
1. Keep old handler methods for backward compatibility
2. Add new typed handler implementation
3. Update executor match arm to use typed handler
4. Remove old handler method once all call sites migrated
