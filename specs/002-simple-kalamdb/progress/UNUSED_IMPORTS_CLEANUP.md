# Unused Imports Cleanup List

**Date**: 2025-10-19  
**Branch**: `002-simple-kalamdb`

## Summary

Found **multiple unused imports** in `kalamdb-core` that should be removed to clean up the codebase.

---

## Unused Import Fixes

### 1. `catalog/catalog_store.rs:16`
```rust
// REMOVE: unused imports
use crate::catalog::{NamespaceId, TableName};

// These types are not used in this file
```

### 2. `flush/user_table_flush.rs:12`
```rust
// REMOVE: unused imports
use datafusion::arrow::datatypes::{Field, Schema};

// Not needed - likely leftover from refactoring
```

### 3. `storage/column_family_manager.rs:8`
```rust
// REMOVE: unused import
use rocksdb::ColumnFamilyDescriptor;

// This import is not used in the file
```

### 4. `storage/parquet_writer.rs:6`
```rust
// REMOVE: unused import
use datafusion::arrow::array::TimestampMillisecondArray;

// Not used in this file
```

### 5. `storage/parquet_writer.rs:13`
```rust
// REMOVE: unused import
use std::sync::Arc;

// Arc is not used in this file
```

### 6. `tables/parquet_scan.rs:6`
```rust
// REMOVE: unused import
use std::sync::Arc;

// Arc is not used in this file
```

### 7. `tables/system/storage_locations_provider.rs:9`
```rust
// REMOVE: unused import
use crate::catalog::LocationType;

// LocationType is not used
```

---

## Deprecated Patterns to Remove

### Pattern 1: Dead Code Markers
Multiple files have `#[allow(dead_code)]` annotations that should be reviewed:

- `catalog/mod.rs:15` - Check if the code is truly dead and remove if so
- `flush/trigger.rs:98` - Review and remove if unnecessary
- `services/user_table_service.rs:31` - Review and remove if unnecessary
- `storage/rocksdb_init.rs:94` - Review and remove if unnecessary
- `tables/user_table_provider.rs:43` - Review and remove if unnecessary

**Action**: For each, either:
1. Remove the function/struct if truly unused, OR
2. Use it somewhere, OR
3. Add a comment explaining why it exists but isn't used yet

---

## Unused Variables

### 1. `tables/system/live_queries_provider.rs:106`
```rust
// Variable `live_id` is unused
// Either use it or remove the parameter
```

### 2. `tables/system/live_queries_provider.rs:141`
```rust
// Variable `connection_id` is unused
// Either use it or remove the parameter
```

---

## Quick Fix Commands

Run these commands to automatically fix simple cases:

```bash
cd /Users/jamal/git/KalamDB/backend

# Fix unused imports automatically
cargo fix --allow-dirty --package kalamdb-core

# Check for remaining warnings
cargo clippy --package kalamdb-core -- -W unused-imports
```

---

## Manual Review Required

Some warnings may need manual review:

1. **Dead code in catalog/mod.rs**: May be part of public API
2. **Dead code in flush/trigger.rs**: May be used in future phases
3. **Unused variables**: May indicate incomplete implementations

---

## Checklist

### Automatic Cleanup
- [ ] Run `cargo fix --allow-dirty --package kalamdb-core`
- [ ] Review and commit automatic changes

### Manual Cleanup
- [ ] Remove `NamespaceId, TableName` from `catalog/catalog_store.rs`
- [ ] Remove `Field, Schema` from `flush/user_table_flush.rs`
- [ ] Remove `ColumnFamilyDescriptor` from `storage/column_family_manager.rs`
- [ ] Remove `TimestampMillisecondArray` from `storage/parquet_writer.rs`
- [ ] Remove unused `Arc` imports from `storage/parquet_writer.rs` and `tables/parquet_scan.rs`
- [ ] Remove `LocationType` from `tables/system/storage_locations_provider.rs`

### Dead Code Review
- [ ] Review `#[allow(dead_code)]` in `catalog/mod.rs`
- [ ] Review `#[allow(dead_code)]` in `flush/trigger.rs`
- [ ] Review `#[allow(dead_code)]` in `services/user_table_service.rs`
- [ ] Review `#[allow(dead_code)]` in `storage/rocksdb_init.rs`
- [ ] Review `#[allow(dead_code)]` in `tables/user_table_provider.rs`

### Unused Variables
- [ ] Fix `live_id` in `tables/system/live_queries_provider.rs:106`
- [ ] Fix `connection_id` in `tables/system/live_queries_provider.rs:141`

### Verification
- [ ] Run `cargo clippy --package kalamdb-core -- -W unused-imports`
- [ ] Verify zero warnings
- [ ] Run full test suite: `cargo test --package kalamdb-core`

---

## Impact Assessment

**Risk**: LOW  
**Effort**: 30-60 minutes  
**Dependencies**: None  
**Breaking Changes**: None (removing unused code has no impact)

---

## Notes

- These cleanups can be done independently of the RocksDB removal refactoring
- Use `cargo fix` for automatic fixes first, then manual review
- Some "dead code" may be intentional for future use - add comments to clarify
