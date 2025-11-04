# SQL Executor Modular Structure Refactoring

## Overview

This document outlines the planned refactoring of the SQL executor from a single monolithic file (`executor.rs` - 4676 lines) into a clean modular structure.

## Proposed Structure

```
src/sql/executor/
│
├── mod.rs                  // Main entry point — SqlExecutor and routing
│
└── handlers/               // All submodules
    ├── ddl.rs              // CREATE / ALTER / DROP
    ├── dml.rs              // INSERT / UPDATE / DELETE
    ├── query.rs            // SELECT / SHOW / DESCRIBE
    ├── transaction.rs      // BEGIN / COMMIT / ROLLBACK
    ├── subscription.rs     // SUBSCRIBE / KILL LIVE QUERY
    ├── system.rs           // System table registration
    ├── authorization.rs    // check_authorization(), RBAC helper
    ├── table_registry.rs   // register_table_with_datafusion(), cache helpers
    ├── audit.rs            // log_audit_event()
    ├── helpers.rs          // parsing, regex utils
    └── types.rs            // ExecutionContext, ExecutionResult, ExecutionMetadata
```

## Implementation Plan

### Phase 1: Create Module Structure ✅
- [x] Create `executor/handlers/` directory
- [x] Create all handler module files with skeletons
- [x] Create `handlers/types.rs` with common types
- [x] Create `handlers/mod.rs` to export all modules

### Phase 2: Prepare mod.rs Entry Point (TODO)
- [ ] Rename `executor.rs` to `executor/mod.rs`
- [ ] Add `pub mod handlers;` declaration
- [ ] Import handler modules
- [ ] Update type references to use `handlers::types::{ExecutionResult, ExecutionContext, ExecutionMetadata}`

### Phase 3: Move Types (TODO)
- [ ] Remove `ExecutionResult` enum from mod.rs
- [ ] Remove `ExecutionContext` struct from mod.rs
- [ ] Remove `ExecutionMetadata` struct from mod.rs
- [ ] Re-export types: `pub use handlers::{ExecutionResult, ExecutionContext, ExecutionMetadata};`
- [ ] Update all internal references

### Phase 4: Move Transaction Handlers (TODO)
- [ ] Move `execute_begin_transaction` to `handlers/transaction.rs::begin()`
- [ ] Move `execute_commit_transaction` to `handlers/transaction.rs::commit()`
- [ ] Move `execute_rollback_transaction` to `handlers/transaction.rs::rollback()`
- [ ] Update dispatch in `execute_with_metadata()`

### Phase 5: Move DDL Handlers (TODO)
- [ ] Move namespace operations to `handlers/ddl.rs`:
  - `execute_create_namespace` → `ddl::create_namespace()`
  - `execute_alter_namespace` → `ddl::alter_namespace()`
  - `execute_drop_namespace` → `ddl::drop_namespace()`
- [ ] Move table operations to `handlers/ddl.rs`:
  - `execute_create_table` → `ddl::create_table()`
  - `execute_alter_table` → `ddl::alter_table()`
  - `execute_drop_table` → `ddl::drop_table()`
- [ ] Move storage operations to `handlers/ddl.rs`:
  - `execute_create_storage` → `ddl::create_storage()`
  - `execute_alter_storage` → `ddl::alter_storage()`
  - `execute_drop_storage` → `ddl::drop_storage()`

### Phase 6: Move DML Handlers (TODO)
- [ ] Move `execute_update` to `handlers/dml.rs::update()`
- [ ] Move `execute_delete` to `handlers/dml.rs::delete()`
- [ ] Move INSERT logic to `handlers/dml.rs::insert()`

### Phase 7: Move Query Handlers (TODO)
- [ ] Move SELECT logic to `handlers/query.rs::select()`
- [ ] Move `execute_show_namespaces` to `handlers/query.rs::show_namespaces()`
- [ ] Move `execute_show_tables` to `handlers/query.rs::show_tables()`
- [ ] Move `execute_show_storages` to `handlers/query.rs::show_storages()`
- [ ] Move `execute_show_table_stats` to `handlers/query.rs::show_stats()`
- [ ] Move `execute_describe_table` to `handlers/query.rs::describe_table()`

### Phase 8: Move Subscription Handlers (TODO)
- [ ] Move `execute_subscribe` to `handlers/subscription.rs::subscribe()`
- [ ] Move `execute_kill_live_query` to `handlers/subscription.rs::kill_live_query()`

### Phase 9: Move User Management (TODO)
- [ ] Move `execute_create_user` to `handlers/ddl.rs::create_user()` or separate `user.rs`
- [ ] Move `execute_alter_user` to `handlers/ddl.rs::alter_user()`
- [ ] Move `execute_drop_user` to `handlers/ddl.rs::drop_user()`

### Phase 10: Move Authorization Logic (TODO)
- [ ] Move `check_authorization` method to `handlers/authorization.rs::check_authorization()`
- [ ] Move `check_write_permissions` to `handlers/authorization.rs`

### Phase 11: Move Table Registry Logic (TODO)
- [ ] Move `register_table_with_datafusion` to `handlers/table_registry.rs`
- [ ] Move `cache_table_metadata` to `handlers/table_registry.rs`
- [ ] Move `extract_table_references` to `handlers/table_registry.rs`
- [ ] Move `extract_tables_from_set_expr` to `handlers/table_registry.rs`
- [ ] Move `extract_table_from_factor` to `handlers/table_registry.rs`

### Phase 12: Move Helpers (TODO)
- [ ] Identify and move parsing utilities to `handlers/helpers.rs`
- [ ] Move regex patterns to `handlers/helpers.rs`
- [ ] Move common validation functions to `handlers/helpers.rs`

### Phase 13: Update mod.rs Routing (TODO)
- [ ] Update `execute_with_metadata` to use handler functions
- [ ] Remove old `execute_*` methods
- [ ] Keep only routing logic in mod.rs

### Phase 14: Testing & Validation (TODO)
- [ ] Run full test suite
- [ ] Fix any broken imports
- [ ] Update test code to use new structure
- [ ] Verify all SQL operations still work

## Benefits

1. **Clean Modular Organization**: Each handler file focuses on one category of SQL operations
2. **Easier Testing**: Individual handlers can be unit tested in isolation
3. **Better Maintainability**: Smaller files are easier to understand and modify
4. **Logical Grouping**: Related functionality grouped together
5. **Extensible**: Easy to add new handler types (e.g., `analytics.rs`, `admin.rs`)
6. **Reduced Coupling**: Clear separation of concerns

## Current Status

**Phase 1 Complete**: Skeleton structure created with placeholder files. Ready for function migration.

## Next Steps

1. Complete Phase 2-3 to establish the new module structure
2. Begin migrating functions starting with simple ones (transactions, Phase 4)
3. Gradually move all handlers following the phases above
4. Test thoroughly after each phase
