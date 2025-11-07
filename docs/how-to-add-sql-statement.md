# How to Add a New SQL Statement with Handler

This guide walks you through adding a new SQL statement to KalamDB in 3 simple steps.

## Overview

When adding a new SQL statement (e.g., `CREATE WIDGET <name>`), you need to:

1. **Write the statement parser** - Parse SQL text into an AST struct
2. **Write the handler** - Implement execution logic
3. **Register the handler** - Connect parser to executor

**Time to complete**: ~30 minutes for a simple statement

---

## Step 1: Write the Statement Parser

Location: `backend/crates/kalamdb-sql/src/ddl/`

### 1.1 Create Parser File

Create a new file for your statement parser (e.g., `create_widget.rs`):

```rust
// backend/crates/kalamdb-sql/src/ddl/create_widget.rs

use crate::ddl::DdlResult;
use serde::{Deserialize, Serialize};

/// CREATE WIDGET statement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateWidgetStatement {
    /// Widget name
    pub name: String,
    
    /// Widget type
    pub widget_type: String,
    
    /// Optional IF NOT EXISTS flag
    pub if_not_exists: bool,
}

impl CreateWidgetStatement {
    /// Parse a CREATE WIDGET statement from SQL
    ///
    /// Syntax: CREATE WIDGET [IF NOT EXISTS] <name> TYPE <type>
    ///
    /// # Examples
    /// ```
    /// CREATE WIDGET my_widget TYPE button
    /// CREATE WIDGET IF NOT EXISTS my_widget TYPE slider
    /// ```
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql_upper = sql.trim().to_uppercase();
        
        // Check for CREATE WIDGET prefix
        if !sql_upper.starts_with("CREATE WIDGET") {
            return Err("Expected CREATE WIDGET statement".to_string());
        }

        // Remove CREATE WIDGET prefix
        let rest = sql.trim()
            .strip_prefix("CREATE WIDGET")
            .or_else(|| sql.trim().strip_prefix("create widget"))
            .ok_or_else(|| "Invalid CREATE WIDGET syntax".to_string())?
            .trim();

        // Check for IF NOT EXISTS
        let (if_not_exists, rest) = if rest.to_uppercase().starts_with("IF NOT EXISTS") {
            let rest = rest.strip_prefix("IF NOT EXISTS")
                .or_else(|| rest.strip_prefix("if not exists"))
                .unwrap()
                .trim();
            (true, rest)
        } else {
            (false, rest)
        };

        // Parse name and TYPE keyword
        let parts: Vec<&str> = rest.split_whitespace().collect();
        
        if parts.len() < 3 {
            return Err("Expected: CREATE WIDGET <name> TYPE <type>".to_string());
        }

        let name = parts[0].to_string();
        
        if parts[1].to_uppercase() != "TYPE" {
            return Err("Expected TYPE keyword after widget name".to_string());
        }

        let widget_type = parts[2].to_string();

        Ok(Self {
            name,
            widget_type,
            if_not_exists,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_widget() {
        let stmt = CreateWidgetStatement::parse("CREATE WIDGET my_widget TYPE button").unwrap();
        assert_eq!(stmt.name, "my_widget");
        assert_eq!(stmt.widget_type, "button");
        assert!(!stmt.if_not_exists);
    }

    #[test]
    fn test_parse_create_widget_if_not_exists() {
        let stmt = CreateWidgetStatement::parse("CREATE WIDGET IF NOT EXISTS my_widget TYPE slider").unwrap();
        assert_eq!(stmt.name, "my_widget");
        assert_eq!(stmt.widget_type, "slider");
        assert!(stmt.if_not_exists);
    }

    #[test]
    fn test_parse_invalid_syntax() {
        let result = CreateWidgetStatement::parse("CREATE WIDGET invalid");
        assert!(result.is_err());
    }
}
```

### 1.2 Export Parser from Module

Edit `backend/crates/kalamdb-sql/src/ddl/mod.rs`:

```rust
// Add your parser module
pub mod create_widget;

// Export the statement struct
pub use create_widget::CreateWidgetStatement;
```

### 1.3 Add to SqlStatement Enum

Edit `backend/crates/kalamdb-sql/src/statement_classifier.rs`:

```rust
#[derive(Debug, Clone)]
pub enum SqlStatement {
    // ... existing variants ...
    
    /// CREATE WIDGET <name> TYPE <type>
    CreateWidget(CreateWidgetStatement),
    
    // ... rest of variants ...
}
```

### 1.4 Add Classification Logic

In the same file, add classification and parsing:

```rust
impl SqlStatement {
    pub fn classify_and_parse(
        sql: &str,
        default_namespace: &NamespaceId,
        role: Role,
    ) -> Result<Self, String> {
        // ... existing code ...
        
        match words.as_slice() {
            // ... existing patterns ...
            
            // Widget operations
            ["CREATE", "WIDGET", ..] => {
                // Add authorization if needed
                if !is_admin {
                    return Err("Admin privileges required for widget operations".to_string());
                }
                Ok(CreateWidgetStatement::parse(sql)
                    .map(SqlStatement::CreateWidget)
                    .unwrap_or(SqlStatement::Unknown))
            }
            
            // ... rest of patterns ...
        }
    }
    
    pub fn name(&self) -> &'static str {
        match self {
            // ... existing cases ...
            SqlStatement::CreateWidget(_) => "CREATE WIDGET",
            // ... rest of cases ...
        }
    }
}
```

### 1.5 Implement DdlAst Trait

Edit `backend/crates/kalamdb-sql/src/lib.rs`:

```rust
/// Marker trait for all DDL AST types
pub trait DdlAst: Send + Clone + std::fmt::Debug {}

// Add implementation for your statement
impl DdlAst for CreateWidgetStatement {}
```

✅ **Step 1 Complete!** Your statement can now be parsed.

---

## Step 2: Write the Handler

Location: `backend/crates/kalamdb-core/src/sql/executor/handlers/`

### 2.1 Create Handler File

Create `ddl_typed.rs` if it doesn't exist, or add to existing file:

```rust
// backend/crates/kalamdb-core/src/sql/executor/handlers/ddl_typed.rs

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::CreateWidgetStatement;
use std::sync::Arc;

/// Handler for CREATE WIDGET statements
pub struct CreateWidgetHandler {
    app_context: Arc<AppContext>,
}

impl CreateWidgetHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateWidgetStatement> for CreateWidgetHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        statement: CreateWidgetStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Your handler logic here
        let name = &statement.name;
        let widget_type = &statement.widget_type;

        // Example: Check if widget already exists
        // let existing = self.app_context.widgets().get(name)?;
        // if existing.is_some() {
        //     if statement.if_not_exists {
        //         return Ok(ExecutionResult::Success(
        //             format!("Widget '{}' already exists", name)
        //         ));
        //     } else {
        //         return Err(KalamDbError::AlreadyExists(
        //             format!("Widget '{}' already exists", name)
        //         ));
        //     }
        // }

        // Example: Create widget entity
        // let widget = Widget::new(name, widget_type);
        
        // Example: Store widget
        // self.app_context.widgets().create(widget)?;

        let message = format!(
            "Widget '{}' of type '{}' created successfully",
            name, widget_type
        );
        Ok(ExecutionResult::Success(message))
    }

    async fn check_authorization(
        &self,
        _statement: &CreateWidgetStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Add authorization checks
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Admin privileges required to create widgets".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::init_test_app_context;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn test_context() -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), Role::Dba)
    }

    #[tokio::test]
    async fn test_create_widget() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let handler = CreateWidgetHandler::new(app_ctx);
        let session = SessionContext::new();
        let ctx = test_context();

        let stmt = CreateWidgetStatement {
            name: "test_widget".to_string(),
            widget_type: "button".to_string(),
            if_not_exists: false,
        };

        let result = handler.execute(&session, stmt, vec![], &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_authorization_check() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let handler = CreateWidgetHandler::new(app_ctx);
        let user_ctx = ExecutionContext::new(UserId::from("regular_user"), Role::User);

        let stmt = CreateWidgetStatement {
            name: "test_widget".to_string(),
            widget_type: "button".to_string(),
            if_not_exists: false,
        };

        let result = handler.check_authorization(&stmt, &user_ctx).await;
        assert!(result.is_err());
    }
}
```

### 2.2 Export Handler

Edit `backend/crates/kalamdb-core/src/sql/executor/handlers/mod.rs`:

```rust
// Export your new handler
pub use ddl_typed::{CreateNamespaceHandler, CreateWidgetHandler};
```

✅ **Step 2 Complete!** Your handler can now execute the statement.

---

## Step 3: Register the Handler

Location: `backend/crates/kalamdb-core/src/sql/executor/handler_registry.rs`

### 3.1 Import Handler

Add import at the top of the file:

```rust
use crate::sql::executor::handlers::ddl_typed::{CreateNamespaceHandler, CreateWidgetHandler};
```

### 3.2 Register in HandlerRegistry::new()

Add registration in the `new()` method:

```rust
impl HandlerRegistry {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        let registry = Self {
            handlers: DashMap::new(),
            app_context: app_context.clone(),
        };

        // ... existing registrations ...

        // Register CREATE WIDGET handler
        registry.register_typed(
            SqlStatement::CreateWidget(CreateWidgetStatement {
                name: "_placeholder".to_string(),
                widget_type: "_placeholder".to_string(),
                if_not_exists: false,
            }),
            CreateWidgetHandler::new(app_context.clone()),
            |stmt| match stmt {
                SqlStatement::CreateWidget(s) => Some(s),
                _ => None,
            },
        );

        registry
    }
}
```

### 3.3 Test Registration

Add a test to verify registration:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // ... existing imports ...

    #[tokio::test]
    async fn test_registry_create_widget() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let registry = HandlerRegistry::new(app_ctx);
        let session = SessionContext::new();
        let ctx = ExecutionContext::new(UserId::from("test_user"), Role::Dba);

        let stmt = SqlStatement::CreateWidget(CreateWidgetStatement {
            name: "test_widget".to_string(),
            widget_type: "button".to_string(),
            if_not_exists: false,
        });

        // Check handler is registered
        assert!(registry.has_handler(&stmt));

        // Execute via registry
        let result = registry.handle(&session, stmt, vec![], &ctx).await;
        assert!(result.is_ok());
    }
}
```

✅ **Step 3 Complete!** Your statement is now fully integrated.

---

## Testing Your Statement

### Test the Full Flow

```rust
#[tokio::test]
async fn test_full_create_widget_flow() {
    init_test_app_context();
    let app_ctx = AppContext::get();
    let executor = SqlExecutor::new(app_ctx, false);
    let session = SessionContext::new();
    let ctx = ExecutionContext::new(UserId::from("admin"), Role::Dba);

    // Execute SQL statement
    let result = executor.execute(
        &session,
        "CREATE WIDGET my_button TYPE button",
        &ctx,
        vec![],
    ).await;

    assert!(result.is_ok());
    
    match result.unwrap() {
        ExecutionResult::Success(msg) => {
            assert!(msg.contains("my_button"));
            assert!(msg.contains("created successfully"));
        }
        _ => panic!("Expected Success result"),
    }
}
```

### Run Tests

```bash
# Test parser
cd backend/crates/kalamdb-sql
cargo test create_widget

# Test handler
cd backend/crates/kalamdb-core
cargo test create_widget

# Test full integration
cargo test --package kalamdb-core test_full_create_widget_flow
```

---

## Checklist

Use this checklist to ensure you've completed all steps:

### Step 1: Parser ✅
- [ ] Created parser file in `kalamdb-sql/src/ddl/`
- [ ] Implemented `parse()` method
- [ ] Added unit tests for parser
- [ ] Exported from `ddl/mod.rs`
- [ ] Added variant to `SqlStatement` enum
- [ ] Added classification logic
- [ ] Added statement name to `name()` method
- [ ] Implemented `DdlAst` trait

### Step 2: Handler ✅
- [ ] Created handler struct with `app_context` field
- [ ] Implemented `TypedStatementHandler<T>`
- [ ] Implemented `execute()` method
- [ ] Implemented `check_authorization()` method
- [ ] Added unit tests for handler
- [ ] Exported from `handlers/mod.rs`

### Step 3: Registration ✅
- [ ] Imported handler in `handler_registry.rs`
- [ ] Called `register_typed()` in `HandlerRegistry::new()`
- [ ] Added extractor function
- [ ] Added registration test
- [ ] Verified handler is discoverable via `has_handler()`

### Testing ✅
- [ ] Parser tests pass
- [ ] Handler tests pass
- [ ] Registration tests pass
- [ ] Full integration test passes
- [ ] Authorization checks work correctly

---

## Common Patterns

### Pattern 1: IF NOT EXISTS

```rust
if existing.is_some() {
    if statement.if_not_exists {
        return Ok(ExecutionResult::Success(
            format!("Resource '{}' already exists", name)
        ));
    } else {
        return Err(KalamDbError::AlreadyExists(
            format!("Resource '{}' already exists", name)
        ));
    }
}
```

### Pattern 2: CASCADE Delete

```rust
if !statement.cascade && has_dependencies {
    return Err(KalamDbError::InvalidOperation(
        format!("Resource has dependencies. Use CASCADE to force delete.")
    ));
}
```

### Pattern 3: WITH OPTIONS Clause

```rust
// In parser
let options = parse_options(options_str)?; // HashMap<String, JsonValue>

// In handler
let max_size = statement.options
    .get("max_size")
    .and_then(|v| v.as_i64())
    .unwrap_or(1000);
```

### Pattern 4: Namespace-Scoped Resources

```rust
// Parse namespace-qualified name
let (namespace, name) = parse_qualified_name(&statement.name, default_namespace)?;

// Access provider
let provider = app_context.get_provider(&namespace)?;
```

---

## Error Handling

### Parser Errors

Return descriptive error messages:

```rust
if parts.len() < 2 {
    return Err("Expected: CREATE WIDGET <name> TYPE <type>".to_string());
}
```

### Handler Errors

Use appropriate `KalamDbError` variants:

```rust
// Resource not found
Err(KalamDbError::NotFound(format!("Widget '{}' not found", name)))

// Already exists
Err(KalamDbError::AlreadyExists(format!("Widget '{}' already exists", name)))

// Invalid input
Err(KalamDbError::InvalidOperation("Widget name cannot be empty".to_string()))

// Unauthorized
Err(KalamDbError::Unauthorized("Admin privileges required".to_string()))

// Storage error
Err(KalamDbError::StorageError(format!("Failed to save widget: {}", e)))
```

---

## Best Practices

1. **Parser**:
   - Handle case-insensitive keywords (use `.to_uppercase()`)
   - Provide clear error messages with expected syntax
   - Add comprehensive unit tests
   - Support optional clauses (IF NOT EXISTS, CASCADE, etc.)

2. **Handler**:
   - Validate input before executing
   - Use `app_context` for all data access
   - Return descriptive success/error messages
   - Implement proper authorization checks
   - Add unit tests for success and error cases

3. **Registration**:
   - Use descriptive placeholder values
   - Keep extractor function simple (single match expression)
   - Add registration test to verify handler is accessible

4. **Testing**:
   - Test parser with valid and invalid input
   - Test handler with different user roles
   - Test full SQL statement execution
   - Test edge cases (empty strings, special characters, etc.)

---

## Example: Complete Statement (CREATE NAMESPACE)

See these files for a complete working example:

- **Parser**: `backend/crates/kalamdb-sql/src/ddl/create_namespace.rs`
- **Handler**: `backend/crates/kalamdb-core/src/sql/executor/handlers/ddl_typed.rs`
- **Registration**: `backend/crates/kalamdb-core/src/sql/executor/handler_registry.rs`

---

## Need Help?

- **Architecture**: See `backend/crates/kalamdb-core/src/sql/executor/ARCHITECTURE.md`
- **Registry Pattern**: See `backend/crates/kalamdb-core/src/sql/executor/GENERIC_ADAPTER_SOLUTION.md`
- **Quick Reference**: See `backend/crates/kalamdb-core/src/sql/executor/QUICKREF.md`
- **Full Guide**: See `backend/crates/kalamdb-core/src/sql/executor/HANDLER_REGISTRY_GUIDE.md`
