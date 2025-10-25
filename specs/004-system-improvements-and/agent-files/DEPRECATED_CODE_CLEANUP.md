# Deprecated Code Cleanup Checklist

**Date**: 2025-10-24  
**Status**: 🔴 IN PROGRESS  
**Rationale**: KalamDB is unreleased - no backward compatibility needed. Remove all legacy code completely.

---

## Deprecated Systems to Remove

### 1. system_storage_locations (Renamed to system_storages) ❌

**Reason**: Naming consistency - using `system.storages` instead of `system.storage_locations`

**Column Family**: `system_storage_locations`  
**Table Name**: `system.storage_locations`

**Files to Remove**:
- [ ] `backend/crates/kalamdb-core/src/tables/system/storage_locations.rs`
- [ ] `backend/crates/kalamdb-core/src/tables/system/storage_locations_provider.rs`

**Code References to Remove**:
- [ ] Mod declarations in `backend/crates/kalamdb-core/src/tables/system/mod.rs`:
  - `pub mod storage_locations;`
  - `pub mod storage_locations_provider;`
- [ ] Pub use statements in system/mod.rs:
  - `pub use storage_locations::StorageLocationsTable;`
  - `pub use storage_locations_provider::{StorageLocationRecord, StorageLocationsTableProvider};`

**Adapter Methods to Remove** (`backend/crates/kalamdb-sql/src/adapter.rs`):
- [ ] All methods referencing `system_storage_locations` CF handle
- [ ] `scan_all_storage_locations()` method

**Public API to Remove** (`backend/crates/kalamdb-sql/src/lib.rs`):
- [ ] `scan_all_storage_locations()` public method
- [ ] Comment: `//! - system_storage_locations: Storage location definitions`

**Constants Removed** (✅ DONE):
- [X] `SystemTableNames::STORAGE_LOCATIONS` from kalamdb-commons/src/constants.rs
- [X] `ColumnFamilyNames::SYSTEM_STORAGE_LOCATIONS` from kalamdb-commons/src/constants.rs
- [X] `system_storage_locations` entry from column_family_manager.rs SYSTEM_COLUMN_FAMILIES

---

### 2. system_table_schemas (Replaced by information_schema_tables) ❌

**Reason**: Architectural consolidation - now part of unified TableDefinition in information_schema_tables

**Column Family**: `system_table_schemas`  
**Table Name**: `system.table_schemas`

**Files to Remove**:
- [ ] None (no dedicated file, only CF and methods)

**Code References to Remove**:
- [ ] All mentions in service files:
  - `backend/crates/kalamdb-core/src/services/user_table_service.rs` (references in comments)
  - `backend/crates/kalamdb-core/src/services/shared_table_service.rs` (insert_table_schema calls + comments)
  - `backend/crates/kalamdb-core/src/services/stream_table_service.rs` (insert_table_schema calls + comments)

**Adapter Methods to Remove** (`backend/crates/kalamdb-sql/src/adapter.rs`):
- [ ] `insert_table_schema()` method (writes to system_table_schemas CF)
- [ ] `scan_all_table_schemas()` method
- [ ] `delete_table_schemas_for_table()` method
- [ ] `get_table_schemas_for_table()` method

**Public API to Remove** (`backend/crates/kalamdb-sql/src/lib.rs`):
- [ ] `scan_all_table_schemas()` public method
- [ ] `delete_table_schemas_for_table()` public method
- [ ] `get_table_schemas_for_table()` public method
- [ ] Comment: `//! - system_table_schemas: Table schema versions`

**Constants Removed** (✅ DONE):
- [X] `SystemTableNames::TABLE_SCHEMAS` from kalamdb-commons/src/constants.rs
- [X] `ColumnFamilyNames::SYSTEM_TABLE_SCHEMAS` from kalamdb-commons/src/constants.rs

**Replacement**: All schema history now stored in `TableDefinition.schema_history` array within information_schema_tables

---

### 3. system.columns (Replaced by information_schema.columns) ❌

**Reason**: Architectural consolidation - column metadata now part of TableDefinition.columns array

**Column Family**: `system_columns` (never actually created in production)  
**Table Name**: `system.columns`

**Files to Remove**:
- [ ] `backend/crates/kalamdb-core/src/tables/system/columns.rs` (ColumnsTable schema definition with 3 passing tests)

**Code References to Remove**:
- [ ] Mod declaration in `backend/crates/kalamdb-core/src/tables/system/mod.rs`:
  - `pub mod columns;`
- [ ] Pub use statement:
  - `pub use columns::ColumnsTable;`

**Adapter Methods to Remove** (`backend/crates/kalamdb-sql/src/adapter.rs`):
- [ ] `insert_column_metadata()` method (added but never used in production)

**Public API to Remove** (`backend/crates/kalamdb-sql/src/lib.rs`):
- [ ] `insert_column_metadata()` public method if exposed

**Replacement**: All column metadata now in `TableDefinition.columns: Vec<ColumnDefinition>`

---

### 4. Obsolete Planning Documents ❌

**Files to Remove**:
- [ ] `backend/PHASE_2B_COLUMN_METADATA.md` (planning doc for system.columns approach, now obsolete)

---

## Specification Updates Required

### Documentation Files to Update:

- [ ] `specs/004-system-improvements-and/contracts/system-tables-schema.md`
  - Remove `system.table_schemas` section
  - Remove `system.storage_locations` section (replaced by system.storages)
  - Add `information_schema.tables` section
  - Add `information_schema.columns` section

- [ ] `specs/004-system-improvements-and/quickstart.md`
  - Replace `system.table_schemas` queries with `information_schema.tables` queries
  - Update schema history examples

- [ ] `specs/004-system-improvements-and/spec.md`
  - Remove FR requirements referencing system.table_schemas
  - Update to reference information_schema pattern

---

## Verification Steps

### Build Verification:
```bash
cd backend
cargo clean
cargo build --workspace
# Should complete with ZERO warnings about unused code
```

### Test Verification:
```bash
cargo test --workspace
# All tests should pass
# No tests should reference deprecated systems
```

### Grep Verification:
```bash
# Search for any remaining references
grep -r "storage_locations" backend/crates/ --exclude-dir=target
grep -r "table_schemas" backend/crates/ --exclude-dir=target
grep -r "system_columns" backend/crates/ --exclude-dir=target
grep -r "system\.columns" backend/crates/ --exclude-dir=target

# Should return ZERO matches (except in comments documenting the change)
```

### RocksDB CF Verification:
After implementing information_schema_tables:
```rust
// Verify only these system CFs exist:
"system_users"
"system_live_queries"
"system_jobs"
"system_namespaces"
"system_storages"
"information_schema_tables"
"user_table_counters"

// These should NOT exist:
"system_storage_locations"  // ❌
"system_table_schemas"       // ❌
"system_columns"             // ❌
```

---

## Task Status

### Constants Cleanup: ✅ COMPLETE
- [X] Removed STORAGE_LOCATIONS from SystemTableNames
- [X] Removed TABLE_SCHEMAS from SystemTableNames
- [X] Removed SYSTEM_STORAGE_LOCATIONS from ColumnFamilyNames
- [X] Removed SYSTEM_TABLE_SCHEMAS from ColumnFamilyNames
- [X] Added INFORMATION_SCHEMA_TABLES to ColumnFamilyNames
- [X] Removed system_storage_locations from SYSTEM_COLUMN_FAMILIES array

### File Removal: ⏳ PENDING
- [ ] Remove storage_locations.rs and storage_locations_provider.rs
- [ ] Remove columns.rs
- [ ] Remove PHASE_2B_COLUMN_METADATA.md

### Method Removal: ⏳ PENDING
- [ ] Remove all system_storage_locations methods from adapter.rs and lib.rs
- [ ] Remove all system_table_schemas methods from adapter.rs and lib.rs
- [ ] Remove insert_column_metadata() method

### Service Updates: ⏳ PENDING
- [ ] Remove insert_table_schema() calls from user_table_service.rs
- [ ] Remove insert_table_schema() calls from shared_table_service.rs
- [ ] Remove insert_table_schema() calls from stream_table_service.rs
- [ ] Update comments to reference information_schema instead

### Specification Updates: ⏳ PENDING
- [ ] Update contracts/system-tables-schema.md
- [ ] Update quickstart.md
- [ ] Update spec.md FR requirements

### Verification: ⏳ PENDING
- [ ] cargo build --workspace (no warnings)
- [ ] cargo test --workspace (all passing)
- [ ] grep verification (zero matches)
- [ ] RocksDB CF list verification

---

## Migration Notes

**NO MIGRATION NEEDED**: KalamDB is unreleased. 

**Approach**: Clean slate
1. Remove all deprecated code
2. Implement information_schema_tables from scratch
3. Fresh database initialization with new CF structure

**No backward compatibility layer required.**

---

**Status Summary**:
- ✅ **Constants**: 6/6 removed
- ⏳ **Files**: 0/4 removed
- ⏳ **Methods**: 0/8 removed
- ⏳ **Services**: 0/3 updated
- ⏳ **Specs**: 0/3 updated
- ⏳ **Verification**: 0/4 complete

**Next Step**: Begin file removal (T533-CLEANUP1 through T533-CLEANUP16)
