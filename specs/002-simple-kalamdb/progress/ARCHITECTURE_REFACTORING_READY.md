# Architecture Refactoring Plan - Ready to Execute

**Date**: 2025-10-17  
**Status**: ✅ Planning Complete - Ready for Implementation  
**Priority**: CRITICAL (Execute before Phase 10)

## Summary

The plan.md and tasks.md have been updated to include **Phase 9.5: Architecture Refactoring** with 33 detailed tasks (T133-T165) to implement the three-layer architecture with the new `kalamdb-store` crate.

## What Changed

### 1. Updated plan.md

**Added Section**: "Architecture Refactoring Phase (Before Phase 10)"

**Location**: After Phase 0 section, before Test Strategy

**Content**:
- Motivation for refactoring (direct RocksDB coupling problems)
- Three-layer architecture diagram
- Benefits (6 key advantages)
- Migration tasks overview (5 phases: A-E)
- Success criteria checklist
- Estimated effort: 33-40 tasks, 6-9 days

**Updated Project Structure**:
- Added kalamdb-store crate with 4 modules
- Updated kalamdb-core to show NO RocksDB imports
- Updated dependency diagram showing three layers

### 2. Updated tasks.md

**Added Phase**: Phase 9.5: Architecture Refactoring - Three-Layer Separation

**Location**: Between Phase 9 (T123-T132) and Phase 10 (now T166-T174)

**Content**: 33 tasks organized in 5 steps:

#### Step A: Create kalamdb-store Crate (T133-T141)
- T133: Create Cargo.toml with dependencies
- T134: Create lib.rs with public exports
- T135: Create key_encoding.rs utilities
- T136: Create user_table_store.rs with full API
- T137: Create shared_table_store.rs
- T138: Create stream_table_store.rs
- T139: Add comprehensive unit tests
- T140: Update workspace Cargo.toml
- T141: Add kalamdb-store to kalamdb-core dependencies

**Total**: 9 tasks

#### Step B: Enhance kalamdb-sql (T142-T149)
- T142-T148: Add scan_all_* methods for all 7 system tables
- T149: Expose in public API

**Total**: 8 tasks

#### Step C: Refactor System Table Providers (T150-T155)
- T150: Refactor UsersTableProvider
- T151: Refactor StorageLocationsTableProvider
- T152: Refactor LiveQueriesTableProvider
- T153: Refactor JobsTableProvider
- T154: Update NamespaceService
- T155: Update main.rs initialization

**Total**: 6 tasks

#### Step D: Refactor User Table Handlers (T156-T160)
- T156: Refactor UserTableInsertHandler
- T157: Refactor UserTableUpdateHandler
- T158: Refactor UserTableDeleteHandler
- T159: Update UserTableService
- T160: Update main.rs initialization

**Total**: 5 tasks

#### Step E: Verify and Document (T161-T165)
- T161: Verify zero RocksDB imports in kalamdb-core
- T162: Verify all 47 tests still pass
- T163: Deprecate CatalogStore
- T164: Add migration guide
- T165: Update spec.md

**Total**: 5 tasks

**Updated Phase 10**: Renumbered from T126-T134 to T166-T174 to accommodate Phase 9.5

## Architecture Before vs After

### Before (Current - Phase 9 Complete)

```
kalamdb-core
    ├── Uses CatalogStore (direct RocksDB)
    ├── User table handlers import rocksdb::DB
    └── Business logic coupled to storage

RocksDB
```

**Problems**:
- ❌ Direct RocksDB coupling in business logic
- ❌ Hard to test without mocking RocksDB
- ❌ CatalogStore is transitional, never properly migrated
- ❌ Code duplication for system table operations

### After (Target - Phase 9.5 Complete)

```
┌─────────────────────────────────────┐
│ kalamdb-core (Business Logic)       │
│ - NO direct RocksDB imports         │
│ - Uses kalamdb-sql for metadata     │
│ - Uses kalamdb-store for data       │
└────────────┬────────────────────────┘
             │
             ├──────────────┬─────────
             ▼              ▼
┌──────────────────┐ ┌──────────────────┐
│ kalamdb-sql      │ │ kalamdb-store    │
│ (System Tables)  │ │ (User Tables)    │
│ Owns RocksDB     │ │ Owns RocksDB     │
└──────────────────┘ └──────────────────┘
             │              │
             └──────┬───────┘
                    ▼
           ┌────────────────┐
           │ RocksDB        │
           └────────────────┘
```

**Benefits**:
- ✅ Clear separation of concerns
- ✅ RocksDB isolated to 2 crates only
- ✅ Easy to mock for testing
- ✅ Clear intent from crate usage
- ✅ CatalogStore properly deprecated
- ✅ No code duplication

## Files Modified

### plan.md
- Added "Architecture Refactoring Phase" section (~120 lines)
- Updated project structure diagram
- Updated key architecture changes list
- Added kalamdb-store to crate descriptions

### tasks.md
- Added Phase 9.5 with 33 tasks (T133-T165)
- Renumbered Phase 10 tasks (T166-T174, was T126-T134)
- Updated Phase 11 task numbers (T175-T178, was T135-T138)
- Added detailed file paths and method signatures for each task

## Implementation Order

### Week 1: kalamdb-store Creation (Days 1-3)
**Tasks**: T133-T141 (9 tasks)
**Deliverable**: kalamdb-store crate with full test coverage
**Test Command**: `cargo test -p kalamdb-store`

### Week 1: kalamdb-sql Enhancement (Days 4-5)
**Tasks**: T142-T149 (8 tasks)
**Deliverable**: scan_all methods for all 7 system tables
**Test Command**: `cargo test -p kalamdb-sql`

### Week 2: System Provider Refactoring (Days 1-2)
**Tasks**: T150-T155 (6 tasks)
**Deliverable**: All providers use kalamdb-sql
**Test Command**: `cargo test -p kalamdb-core tables::system`

### Week 2: User Handler Refactoring (Days 3-4)
**Tasks**: T156-T160 (5 tasks)
**Deliverable**: All handlers use kalamdb-store
**Test Command**: `cargo test -p kalamdb-core tables::user_table`

### Week 2: Verification (Day 5)
**Tasks**: T161-T165 (5 tasks)
**Deliverable**: Clean architecture verified, docs updated
**Test Command**: `cargo test -p kalamdb-core --lib` (all 47 tests pass)

## Success Criteria

After completing all 33 tasks:

- [ ] kalamdb-store crate exists with full test coverage
- [ ] All 7 system tables have scan_all methods in kalamdb-sql
- [ ] All 4 system table providers use kalamdb-sql only
- [ ] All 3 user table handlers use kalamdb-store only
- [ ] kalamdb-core has ZERO direct rocksdb imports
- [ ] All existing 47 Phase 9 tests pass
- [ ] CatalogStore marked as deprecated
- [ ] Migration guide documented

**Verification Command**:
```powershell
# Check for rocksdb imports in kalamdb-core (should be ZERO)
cd backend\crates\kalamdb-core
Get-ChildItem -Recurse -Filter *.rs | Select-String "use rocksdb"

# Run all tests
cd ..\..
cargo test -p kalamdb-core --lib
```

## Next Steps After Refactoring

Once Phase 9.5 is complete, proceed to **Phase 10: Table Deletion** (T166-T174) with clean architecture:
- Table deletion service will use kalamdb-sql for metadata cleanup
- Will use kalamdb-store for data cleanup
- No direct RocksDB coupling

## References

- **Detailed Plan**: `specs/002-simple-kalamdb/plan.md` (search "Architecture Refactoring Phase")
- **Task Breakdown**: `specs/002-simple-kalamdb/tasks.md` (Phase 9.5, lines ~413-520)
- **Architecture Spec**: `specs/002-simple-kalamdb/spec.md` (lines 606-1200, three-layer architecture)
- **Original RFC**: `ARCHITECTURE_REFACTOR_PLAN.md` (initial proposal)

---

**Status**: ✅ Ready to begin implementation  
**Next Action**: Execute T133 (Create kalamdb-store/Cargo.toml)
