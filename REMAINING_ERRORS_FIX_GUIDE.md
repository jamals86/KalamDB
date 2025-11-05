# Fix Guide for Remaining 29 Compilation Errors

## Quick Reference

These errors are **pre-existing** and **not caused by** the SystemTable → TableDefinition migration.

## Category 1: UnifiedJobManager (6 errors) - Priority: HIGH

### Error Pattern
```
error[E0432]: unresolved import `crate::jobs::UnifiedJobManager`
```

### Locations
- `app_context.rs:8`
- `ddl.rs:1116, 1524, 1560, 1573`

### Fix Options

**Option A: Remove UnifiedJobManager references** (RECOMMENDED)
```rust
// app_context.rs
// Remove: use crate::jobs::UnifiedJobManager;
// Keep: use crate::jobs::JobManager; // or whatever exists

// ddl.rs - Replace UnifiedJobManager with actual job manager type
let job_manager: Arc<dyn JobManager> = app_ctx.job_manager();
```

**Option B: Implement UnifiedJobManager**
- Follow specs/009-core-architecture/PHASE9_COMPLETE_SUMMARY.md
- Create jobs/unified_manager.rs  
- Implement the interface described in docs

## Category 2: kalamdb_sql::Table (8 errors) - Priority: CRITICAL

### Error Pattern
```
error[E0422]: cannot find struct, variant or union type `Table` in crate `kalamdb_sql`
```

### Locations
- `ddl.rs:694, 822, 957, 1439`

### Current (Broken) Code
```rust
// Line 694
let table = kalamdb_sql::Table {
    id: table_id_str.clone(),
    storage_id: storage_id.0.clone(),
    use_user_storage: matches!(storage_mode, StorageMode::User),
    flush_policy: flush_policy.clone(),
    deleted_retention_hours: retention_hours.map(|h| h as u32),
    access_level,
    table_definition: table_def.clone(),
};
```

### Fix: Use TableDefinition Directly
```rust
// Tables store only needs TableDefinition now
let table_id = TableId::new(
    &session_ctx.user_id,
    namespace,
    table_name
);

// Store in TablesStore (from SystemTablesRegistry)
app_ctx.system_tables()
    .tables()
    .create_table(&table_id, &table_def)?;
```

### Additional Context Fields
The old `kalamdb_sql::Table` had these fields that TableDefinition doesn't:
- `storage_id` - Stored in table_options or separate store
- `use_user_storage` - Derived from StorageMode, not persisted
- `flush_policy` - Stored in table_options.flush_policy  
- `deleted_retention_hours` - Stored in table_options.retention_hours
- `access_level` - May need separate SharedTableMetadata store

**IMPORTANT**: These auxiliary fields need a home. Options:
1. Add them to `table_options: HashMap<String, String>` as JSON
2. Create separate stores (StorageMetadataStore, AccessControlStore)
3. Add optional fields to TableDefinition struct

## Category 3: Job API (11 errors) - Priority: MEDIUM

### Error Pattern
```
error[E0599]: no method named `new` found for struct `Job`
error[E0599]: no method named `complete` found for struct `Job`
```

### Locations
- Various files in jobs/, flush/, and executor code

### Old API (Broken)
```rust
let job = Job::new(JobType::Flush, namespace, None);
job.complete("Done");
job.fail("Error");
```

### New API (Need to Find)
```bash
# Find how Job is actually constructed
grep -r "impl Job" backend/crates/kalamdb-commons/src/
```

Then update all call sites to use the correct constructor and status methods.

## Category 4: DDL Type Mismatches (3 errors) - Priority: HIGH

### Error 1: create_table argument count
```
error[E0061]: this method takes 2 arguments but 1 argument was supplied
--> ddl.rs:708
```

**Fix**: Pass both TableId and TableDefinition
```rust
// Old (broken)
tables_provider.create_table(&table)?;

// New  
let table_id = TableId::new(&session_ctx.user_id, namespace, table_name);
tables_provider.create_table(&table_id, &table_def)?;
```

### Error 2: String vs TableId  
```
error[E0308]: mismatched types expected `TableId`, found `String`
--> ddl.rs:1162
```

**Fix**: Convert String to TableId
```rust
// Old
let table = tables_provider.get_table_by_id(&table_id_str)?;

// New
let table_id = TableId::parse(&table_id_str)?; // or TableId::new()
let table = tables_provider.get_table_by_id(&table_id)?;
```

### Error 3: &str vs TableId
```
error[E0308]: mismatched types expected `TableId`, found `&str`
--> ddl.rs:1496  
```

**Fix**: Convert &str to TableId
```rust
// Old
tables_provider.delete_table(table_id_str)?;

// New
let table_id = TableId::parse(table_id_str)?;
tables_provider.delete_table(&table_id)?;
```

### Error 4: Missing storage_id field
```
error[E0609]: no field `storage_id` on type `TableDefinition`
--> ddl.rs:1234
```

**Fix**: Retrieve from table_options or separate store
```rust
// Option 1: From table_options
let storage_id_str = table_def.table_options
    .get("storage_id")
    .ok_or_else(|| KalamDbError::InvalidOperation("Missing storage_id".into()))?;
let storage_id = StorageId(storage_id_str.clone());

// Option 2: Separate metadata store
let storage_id = app_ctx.storage_metadata()
    .get_storage_for_table(&table_id)?;
```

## Category 5: Import Path (1 error) - Priority: LOW

### Error
```
error[E0432]: unresolved import `kalamdb_sql::flush::FlushPolicy`
--> ddl.rs:22
```

### Fix
```rust
// Find correct import
use kalamdb_commons::system::FlushPolicy; // or wherever it lives
```

## Execution Plan

### Phase 1: Unblock Compilation (2-3 hours)
1. Fix UnifiedJobManager imports (remove or stub)
2. Fix kalamdb_sql::Table references (use TableDefinition)
3. Fix type mismatches in DDL handlers
4. Fix import paths

### Phase 2: Job API Migration (4-6 hours)  
5. Research new Job construction pattern
6. Update all Job::new() call sites
7. Update all .complete()/.fail() call sites

### Phase 3: Architecture Decisions (Planning)
8. Decide where to store auxiliary table metadata (storage_id, access_level, etc.)
9. Implement chosen storage strategy
10. Update all code to use new pattern

### Phase 4: Testing
11. Run `cargo check` - should have 0 errors
12. Run `cargo test` - verify all tests pass
13. Run integration tests

## Commands to Run After Fixes

```bash
# Check compilation
cargo check 2>&1 | tee check_results.txt

# Count remaining errors
cargo check 2>&1 | grep "^error" | wc -l

# Run tests
cargo test --workspace

# Run specific test suites
cargo test -p kalamdb-core --test test_schema_consolidation
```

## Success Criteria

- [ ] 0 compilation errors
- [ ] All tests compile
- [ ] All tests pass
- [ ] Migration is validated in production-like environment

---
**Created**: November 6, 2025  
**For**: Post-migration cleanup tasks
