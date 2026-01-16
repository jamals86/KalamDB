# SQL Executor Refactoring Guide

## Current State
- Single file: `src/sql/executor.rs` (4676 lines)
- All execution logic in one place

## Target Structure

```
src/sql/executor/
├── mod.rs                  // Main entry, SqlExecutor struct, routing
└── handlers/
    ├── mod.rs             // Handler module exports
    ├── types.rs           // ExecutionResult, ExecutionContext, ExecutionMetadata  
    ├── authorization.rs   // check_authorization()
    ├── ddl.rs             // CREATE/ALTER/DROP TABLE/NAMESPACE/STORAGE/USER
    ├── dml.rs             // INSERT/UPDATE/DELETE
    ├── query.rs           // SELECT/SHOW/DESCRIBE
    ├── transaction.rs     // BEGIN/COMMIT/ROLLBACK
    ├── subscription.rs    // SUBSCRIBE TO/KILL LIVE QUERY
    ├── flush.rs           // STORAGE FLUSH TABLE/STORAGE FLUSH ALL  
    ├── table_registry.rs  // register_table_with_datafusion, extract_table_references
    ├── audit.rs           // Audit logging helpers
    ├── helpers.rs         // Parsing utilities, validation
    └── system.rs          // System table helpers
```

## Migration Steps

### Step 1: Rename and Restructure
```bash
# Rename executor.rs to executor/mod.rs
cd backend/crates/kalamdb-core/src/sql
mkdir -p executor/handlers
mv executor.rs executor/mod.rs
```

### Step 2: Create handlers/types.rs
Move these types from mod.rs to handlers/types.rs:
- `ExecutionResult` enum
- `ExecutionContext` struct  
- `ExecutionMetadata` struct

Add to mod.rs:
```rust
pub mod handlers;
pub use handlers::{ExecutionResult, ExecutionContext, ExecutionMetadata};
```

### Step 3: Create handlers/authorization.rs
Move `check_authorization()` method to:
```rust
pub fn check_authorization(ctx: &ExecutionContext, sql: &str) -> Result<(), KalamDbError>
```

Update mod.rs to call `handlers::authorization::check_authorization(exec_ctx, sql)?;`

### Step 4: Create handlers/transaction.rs
Move these methods:
- `execute_begin_transaction()` → `transaction::begin()`
- `execute_commit_transaction()` → `transaction::commit()`  
- `execute_rollback_transaction()` → `transaction::rollback()`

### Step 5: Create handlers/ddl.rs
Move all DDL execute methods:
- Namespace: `create_namespace`, `alter_namespace`, `drop_namespace`
- Table: `create_table`, `alter_table`, `drop_table`
- Storage: `create_storage`, `alter_storage`, `drop_storage`
- User: `create_user`, `alter_user`, `drop_user`

### Step 6: Create handlers/dml.rs  
Move DML execute methods:
- `execute_update()` → `dml::update()`
- `execute_delete()` → `dml::delete()`
- INSERT logic → `dml::insert()`

### Step 7: Create handlers/query.rs
Move query execute methods:
- SELECT logic → `query::select()`
- `execute_show_namespaces()` → `query::show_namespaces()`
- `execute_show_tables()` → `query::show_tables()`
- `execute_show_storages()` → `query::show_storages()`
- `execute_show_table_stats()` → `query::show_stats()`
- `execute_describe_table()` → `query::describe_table()`

### Step 8: Create handlers/subscription.rs
Move subscription methods:
- `execute_subscribe()` → `subscription::subscribe()`
- `execute_kill_live_query()` → `subscription::kill_live_query()`

### Step 9: Create handlers/flush.rs
Move flush methods:
- `execute_flush_table()` → `flush::flush_table()`
- `execute_flush_all_tables()` → `flush::flush_all_tables()`

### Step 10: Create handlers/table_registry.rs
Move table registration helpers:
- `register_table_with_datafusion()`
- `cache_table_metadata()`
- `extract_table_references()`
- `extract_tables_from_set_expr()`
- `extract_table_from_factor()`
- `check_write_permissions()`

### Step 11: Create handlers/helpers.rs
Move utility functions:
- Parsing helpers
- Regex patterns
- Validation functions

### Step 12: Create handlers/audit.rs
Move audit logging helpers

### Step 13: Create handlers/system.rs
Move system table registration helpers

### Step 14: Update mod.rs Routing
Simplify `execute_with_metadata()` to only route to handlers:

```rust
match SqlStatement::classify(sql) {
    SqlStatement::CreateNamespace => ddl::create_namespace(self, session, sql, exec_ctx).await,
    SqlStatement::CreateTable => ddl::create_table(self, session, sql, exec_ctx).await,
    SqlStatement::Select => query::select(self, session, sql, exec_ctx).await,
    SqlStatement::Insert => dml::insert(self, session, sql, exec_ctx).await,
    SqlStatement::Update => dml::update(self, session, sql, exec_ctx).await,
    SqlStatement::Delete => dml::delete(self, session, sql, exec_ctx).await,
    SqlStatement::BeginTransaction => transaction::begin(self).await,
    SqlStatement::CommitTransaction => transaction::commit(self).await,
    SqlStatement::RollbackTransaction => transaction::rollback(self).await,
    SqlStatement::Subscribe => subscription::subscribe(self, session, sql, exec_ctx).await,
    _ => Err(KalamDbError::InvalidSql(format!("Unsupported: {}", sql))),
}
```

## Testing
After each step:
```bash
cargo test -p kalamdb-core
```

## Benefits
- Smaller, focused files (each handler ~200-500 lines vs 4676 lines)
- Easier to test individual modules
- Clear separation of concerns
- Easier onboarding for new developers
- Better IDE navigation and search
