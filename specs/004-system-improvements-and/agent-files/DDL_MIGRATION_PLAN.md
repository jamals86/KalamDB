# DDL Parser Migration Issue

## Problem Statement

DDL parsers are currently scattered between two crates, violating the separation of concerns principle:

**Principle**: `kalamdb-sql` (parsing) vs `kalamdb-core` (execution)

### Current State

**kalamdb-sql/src/ddl/** (3 files) ✅:
- `create_namespace.rs` - Uses `DdlResult<T>` (anyhow::Result)
- `create_user_table.rs` - Uses `DdlResult<T>` (anyhow::Result)
- `drop_table.rs` - Uses `DdlResult<T>` (anyhow::Result)

**kalamdb-core/src/sql/ddl/** (14 files) ❌:
- `alter_namespace.rs` - Uses `Result<_, KalamDbError>`
- `alter_table.rs` - Uses `Result<_, KalamDbError>`
- `backup_namespace.rs` - Uses `Result<_, KalamDbError>`
- `create_shared_table.rs` - Uses `Result<_, KalamDbError>`
- `create_stream_table.rs` - Uses `Result<_, KalamDbError>`
- `describe_table.rs` - Uses `Result<_, KalamDbError>`
- `drop_namespace.rs` - Uses `Result<_, KalamDbError>`
- `kill_live_query.rs` - Uses `Result<_, KalamDbError>`
- `restore_namespace.rs` - Uses `Result<_, KalamDbError>`
- `show_backup.rs` - Uses `Result<_, KalamDbError>`
- `show_namespaces.rs` - Uses `Result<_, KalamDbError>`
- `show_tables.rs` - Uses `Result<_, KalamDbError>`
- `show_table_stats.rs` - Uses `Result<_, KalamDbError>`
- `mod.rs` - Module organization

## Issues

1. **Violation of Separation of Concerns**
   - Parsing logic mixed with execution crate
   - Harder to test parsers in isolation
   - Creates unnecessary dependency cycle risk

2. **Inconsistent Error Types**
   - kalamdb-sql uses `DdlResult<T>` (anyhow::Result)
   - kalamdb-core uses `Result<_, KalamDbError>`
   - Makes it unclear where parsing ends and execution begins

3. **Code Duplication**
   - Many parsers could use shared `parser/utils.rs` utilities
   - Similar parsing patterns repeated across files

4. **Import Inconsistency**
   - kalamdb-sql imports from `kalamdb_commons::models::*`
   - kalamdb-core imports from `crate::catalog::*`
   - Different import patterns for same types

## Solution Plan

### Phase 1: Move DDL Parsers (T067a-1 to T067a-14)

Move all DDL statement parsers from `kalamdb-core/src/sql/ddl/` to `kalamdb-sql/src/ddl/`:

1. **T067a-1**: Move `alter_namespace.rs` + refactor to use DdlResult & shared utils
2. **T067a-2**: Move `backup_namespace.rs` + refactor
3. **T067a-3**: Move `restore_namespace.rs` + refactor
4. **T067a-4**: Move `drop_namespace.rs` + refactor
5. **T067a-5**: Move `show_namespaces.rs` + refactor
6. **T067a-6**: Move `show_tables.rs` + refactor
7. **T067a-7**: Move `show_table_stats.rs` + refactor
8. **T067a-8**: Move `show_backup.rs` + refactor
9. **T067a-9**: Move `describe_table.rs` + refactor
10. **T067a-10**: Move `create_shared_table.rs` + refactor
11. **T067a-11**: Move `create_stream_table.rs` + refactor
12. **T067a-12**: Move `alter_table.rs` + refactor
13. **T067a-13**: Move `kill_live_query.rs` + refactor
14. **T067a-14**: Update kalamdb-sql/src/ddl/mod.rs to export all DDL statements

### Phase 2: Update kalamdb-core (T067a-15 to T067a-16)

15. **T067a-15**: Update kalamdb-core imports to use `kalamdb_sql::ddl::*`
16. **T067a-16**: Remove old kalamdb-core/src/sql/ddl/ directory
17. **T067a-17**: Update executor.rs to import DDL statements from kalamdb-sql

### Refactoring Pattern

For each moved file, apply these changes:

```rust
// OLD (kalamdb-core):
use crate::catalog::NamespaceId;
use crate::error::KalamDbError;
pub fn parse(sql: &str) -> Result<Self, KalamDbError> {
    return Err(KalamDbError::InvalidSql("...".to_string()));
}

// NEW (kalamdb-sql):
use crate::ddl::DdlResult;
use kalamdb_commons::models::NamespaceId;
use anyhow::anyhow;
pub fn parse(sql: &str) -> DdlResult<Self> {
    return Err(anyhow!("..."));
}
```

Also apply `parser/utils.rs` functions where appropriate:
- `normalize_sql()` instead of manual `.trim().split_whitespace()...`
- `extract_quoted_keyword_value()` for extracting quoted strings
- `extract_identifier()` for extracting tokens

## Benefits

1. **Clear Separation of Concerns**
   - All parsing in kalamdb-sql
   - All execution in kalamdb-core
   - Easier to understand codebase

2. **Consistent Error Handling**
   - All parsers use `DdlResult<T>` (anyhow::Result)
   - Consistent error patterns across codebase

3. **Code Reuse**
   - Shared parsing utilities reduce duplication
   - Easier to maintain and test

4. **Better Testability**
   - Parser tests don't require kalamdb-core dependencies
   - Faster compilation for parser-only tests

## Estimated Effort

- **Lines to move**: ~1,500-2,000 lines
- **Files to move**: 14 files
- **Import updates**: ~50-100 import statements in kalamdb-core
- **Test updates**: ~50-80 tests

**Time estimate**: 2-3 hours for careful migration with testing

## Risks

1. **Breaking Changes**: Executors in kalamdb-core depend on these parsers
   - Mitigation: Keep function signatures compatible, only change error types
   
2. **Test Failures**: Tests may need updates for new error types
   - Mitigation: Run tests after each file migration
   
3. **Circular Dependencies**: kalamdb-sql → kalamdb-commons ← kalamdb-core
   - Status: Already safe, kalamdb-sql doesn't depend on kalamdb-core

## Next Steps

1. Start with simplest parsers (SHOW commands)
2. Move one file at a time, test after each
3. Update executor imports incrementally
4. Remove old directory once all migrations complete
