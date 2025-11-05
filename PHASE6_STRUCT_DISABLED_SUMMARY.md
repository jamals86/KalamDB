# Phase 6: KalamSql & StorageAdapter Structs Disabled

## Summary

Following user suggestion, both `KalamSql` and `StorageAdapter` structs have been completely disabled to let the compiler find ALL usage sites that need migration to SystemTablesRegistry providers.

## Changes Made

### 1. StorageAdapter Disabled
**File**: `backend/crates/kalamdb-sql/src/adapter.rs` → **RENAMED TO** `adapter.rs.disabled`

**Reason**: The file contains 600+ lines with nested comments, which Rust doesn't support. Renamed the file to cleanly disable the module without comment nesting issues.

### 2. Kalamb-sql Module Updated
**File**: `backend/crates/kalamdb-sql/src/lib.rs`

**Changes**:
- Line 35: Commented out `pub mod adapter;` declaration
- Line 56: Commented out `pub use adapter::StorageAdapter;` export
- Line 57: Commented out `pub type RocksDbAdapter = StorageAdapter;` alias
- Lines 92-424: Wrapped entire `KalamSql` struct and impl in block comment
- Added clear PHASE 6 comment headers explaining the changes

**Result**: kalamdb-sql compiles with 4 unused import warnings (expected)

## Compilation Errors Found

### kalamdb-auth (3 errors)

All three files try to import `RocksDbAdapter`:

```rust
// backend/crates/kalamdb-auth/src/extractor.rs:11
use kalamdb_sql::RocksDbAdapter;

// backend/crates/kalamdb-auth/src/service.rs:11  
use kalamdb_sql::RocksDbAdapter;

// backend/crates/kalamdb-auth/src/user_repo.rs:6
use kalamdb_sql::RocksDbAdapter;
```

**Migration Path**:
- extractor.rs: Use UsersTableProvider from AppContext
- service.rs: Use UsersTableProvider from AppContext  
- user_repo.rs: Use UsersTableProvider from AppContext

## Expected Additional Errors

Based on earlier grep searches, we expect errors in:

1. **backend/crates/kalamdb-core/src/app_context.rs:115**
   - Creates `Arc::new(KalamSql::new(storage_backend))`
   - Field: `kalam_sql: Arc<KalamSql>`
   - **Fix**: Remove field and getter, SystemTablesRegistry already initialized

2. **backend/crates/kalamdb-core/src/sql/executor/mod.rs** (~20 call sites)
   - Multiple methods call `self.kalam_sql()`
   - **Fix**: Replace with `self.app_context.system_tables().<provider>()`

3. **Background Jobs**:
   - **stream_eviction.rs**: Uses `kalam_sql.scan_all_tables()`, `kalam_sql.get_table_definition()`
   - **user_cleanup.rs**: Uses `kalam_sql.scan_all_users()`, `kalam_sql.scan_all_tables()`
   - **Fix**: Migrate to TablesTableProvider, UsersTableProvider

4. **Test Files** (15+ matches):
   - Various test modules create `KalamSql::new()` instances
   - **Fix**: Update tests to use AppContext or providers directly

## Next Steps

### Immediate (P0)
1. Fix kalamdb-auth errors (3 files)
2. Fix app_context.rs KalamSql instantiation
3. Remove app_context kalam_sql field and getter

### Phase 7 (P1)
4. Fix SqlExecutor 20+ call sites
5. Fix background jobs (stream_eviction, user_cleanup)

### Phase 7+ (P2)
6. Fix test infrastructure
7. Update service test modules

## Benefits of This Approach

✅ **Comprehensive**: Compiler finds ALL usages (grep might miss some)  
✅ **Prioritized**: Compiler shows dependency order  
✅ **Type-safe**: Compiler ensures replacements are correct  
✅ **Systematic**: Clear checklist of errors to fix  
✅ **Verifiable**: Once compiles, all KalamSql/StorageAdapter usages are gone

## Pattern for Fixes

### Before (using KalamSql):
```rust
let kalam_sql = Arc::new(KalamSql::new(storage_backend)?);
let user = kalam_sql.get_user(username)?;
```

### After (using Providers):
```rust
let users_provider = app_context.system_tables().users();
let user = users_provider.get_user_by_username(username)?;
```

### Before (SqlExecutor):
```rust
let kalam_sql = self.kalam_sql();
let table = kalam_sql.get_table(table_id)?;
```

### After (SqlExecutor):
```rust
let tables_provider = self.app_context.system_tables().tables();
let table = tables_provider.get_table_by_id(&TableId::from(table_id))?;
```

## Success Criteria

- [ ] All kalamdb-auth errors fixed (3 files)
- [ ] AppContext no longer has kalam_sql field
- [ ] SqlExecutor no longer calls kalam_sql() getter
- [ ] Background jobs migrated to providers
- [ ] All tests updated
- [ ] `cargo check` passes with 0 KalamSql/StorageAdapter errors
- [ ] No `Arc<KalamSql>` or `Arc<StorageAdapter>` in execution path

## Files to Restore Later

Once migration is complete, these files can be deleted:
- `backend/crates/kalamdb-sql/src/adapter.rs.disabled` (600+ lines)
- KalamSql impl block in lib.rs (300+ lines)

The kalamdb-sql crate will only export:
- Parsing utilities (SqlStatement, DDL structs)
- SqlParser
- Query cache utilities
- Error formatters
- No data access or execution logic
