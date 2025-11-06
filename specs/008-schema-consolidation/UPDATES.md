# Schema Consolidation Specification Updates

**Date**: 2025-11-01  
**Branch**: 008-schema-consolidation  
**Updated By**: User corrections, clarifications, and enhancements

## Summary of Changes

All planning documents have been updated to reflect the following 10 critical corrections and enhancements:

---

## Latest Update: Type-Safe Table Options (November 1, 2025)

**Change**: Replaced `HashMap<String, String>` table_options with type-safe `TableOptions` enum.

**Rationale**: Compile-time safety prevents setting wrong options for table types. Each TableType has specific, documented options with smart defaults.

**Files Updated**:
- `spec.md`: FR-SC-002 through FR-SC-002f added (6 new requirements)
- `tasks.md`: T013b-T013h, T015b, T021b added (8 new tasks)
- Implementation: `backend/crates/kalamdb-commons/src/models/schemas/table_options.rs` (new file, 400+ lines)

**Type-Safe Options Structure**:
```rust
pub enum TableOptions {
    User(UserTableOptions),      // partition_by_user, max_rows_per_user, enable_rls, compression
    Shared(SharedTableOptions),  // access_level, enable_cache, cache_ttl_seconds, compression, enable_replication
    Stream(StreamTableOptions),  // ttl_seconds (required), eviction_strategy, max_stream_size_bytes, enable_compaction, watermark_delay_seconds, compression
    System(SystemTableOptions),  // read_only, enable_cache, cache_ttl_seconds, localhost_only
}
```

**Benefits**:
- ✅ Compile-time validation (can't set USER options on STREAM tables)
- ✅ IDE autocomplete for available options
- ✅ Type documentation for each field
- ✅ Required fields enforced (e.g., `ttl_seconds` for STREAM tables)
- ✅ Pattern matching for type-specific logic
- ✅ Clean JSON serialization with tagged union

**Test Coverage**: 12 new unit tests + 3 integration tests = 153 total tests passing (was 141)

---

## 1. ❌ No Secondary Indexes

**Change**: Removed all references to secondary indexes on `namespace_id` and `table_type`.

**Rationale**: TableId is globally unique and already contains `namespace.tableName`, making secondary indexes unnecessary. Cannot create duplicate tables like `namespace1.table1` as both USER and SHARED type.

**Files Updated**:
- `spec.md`: FR-STORE-004 updated
- `research.md`: Section 2 completely rewritten
- `data-model.md`: ER diagram, EntityStore integration, relationship sections
- `plan.md`: Complexity tracking table updated

**Implementation Impact**:
- No `SecondaryIndex<TableDefinition, NamespaceId>` needed
- No `SecondaryIndex<TableDefinition, TableType>` needed  
- Namespace queries use in-memory filtering (acceptable for <10,000 tables in Alpha)
- Simpler architecture, no index corruption risk

---

## 2. ✅ Rename FLOAT_ARRAY → EMBEDDING

**Change**: All occurrences of `FLOAT_ARRAY` renamed to `EMBEDDING` for ML/AI vector embeddings.

**Rationale**: More intuitive naming for vector embedding use case (sentence embeddings, OpenAI embeddings, image embeddings).

**Files Updated**:
- `spec.md`: All 20+ occurrences updated
- `research.md`: Type name and examples
- `data-model.md`: KalamDataType enum, examples  
- `quickstart.md`: Code examples
- `plan.md`: Complexity tracking

**Examples**:
```rust
// Before
KalamDataType::FLOAT_ARRAY(1536)

// After
KalamDataType::EMBEDDING(1536)
```

```sql
-- Before
CREATE TABLE docs (embedding FLOAT_ARRAY(1536));

-- After
CREATE TABLE docs (embedding EMBEDDING(1536));
```

---

## 3. ✅ ColumnDefault with Parameters

**Change**: Updated `ColumnDefault::FunctionCall` to support parameterized functions.

**Rationale**: Enable functions like `SNOWFLAKE(datacenter_id, worker_id)` with arguments.

**Files Updated**:
- `spec.md`: Edge case description updated
- `data-model.md`: ColumnDefault enum redefined with struct variant
- `quickstart.md`: Examples added

**New Definition**:
```rust
pub enum ColumnDefault {
    None,
    Literal(Value),
    FunctionCall {
        name: String,
        args: Vec<String>,
    },
}
```

**Examples**:
```rust
// No parameters
ColumnDefault::FunctionCall { name: "NOW".into(), args: vec![] }
ColumnDefault::FunctionCall { name: "UUID".into(), args: vec![] }

// With parameters
ColumnDefault::FunctionCall {
    name: "SNOWFLAKE".into(),
    args: vec!["1".into(), "42".into()],
}

ColumnDefault::FunctionCall {
    name: "RANDOM_STRING".into(),
    args: vec!["16".into()],
}
```

---

## 4. ✅ Four Table Types: SYSTEM/USER/SHARED/STREAM

**Change**: Added STREAM table type to complete the set.

**Rationale**: KalamDB has 4 table types, not 3. All four should have TableDefinition entries.

**Files Updated**:
- `spec.md`: Key Entities section, TableDefinition description
- `data-model.md`: Added TableType enum entity, ER diagram updated

**TableType Enum**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TableType {
    SYSTEM,   // System tables (users, jobs, namespaces, etc.)
    USER,     // User-created regular tables
    SHARED,   // Cross-user shared tables
    STREAM,   // Stream tables with TTL and eviction
}
```

**All four types** have TableDefinition in the schema store.

---

## 5. ✅ EntityStore Location Corrected

**Change**: EntityStore implementation moved from `kalamdb-store` to `kalamdb-core/src/tables/system/schemas`.

**Rationale**: Single source of truth for schema data should be close to DataFusion integration and table system code.

**Files Updated**:
- `spec.md`: FR-STORE-002 location specified
- `plan.md`: Project structure completely rewritten
- `data-model.md`: EntityStore integration section, ER diagram
- `research.md`: Summary table

**New Location**:
```
backend/crates/kalamdb-core/src/tables/system/schemas/
├── mod.rs
├── table_schema_store.rs  # EntityStore<TableId, TableDefinition>
└── registry.rs        # DashMap-based cache
```

---

## 6. ✅ Delete tables_v2/

**Change**: Added DELETE marker for `backend/crates/kalamdb-core/src/tables/system/tables_v2/` directory.

**Rationale**: Obsolete with new schema store consolidation.

**Files Updated**:
- `plan.md`: Project structure with DELETE comment

**Action Required**: Delete entire `tables_v2/` directory during implementation.

---

## 7. ✅ Delete information_*.rs Files

**Change**: Added DELETE marker for `backend/crates/kalamdb-core/src/tables/system/information_*.rs` files.

**Rationale**: Information schema queries will use consolidated schema models from EntityStore, not separate files.

**Files Updated**:
- `plan.md`: Project structure with DELETE comment

**Action Required**: Delete all `information_*.rs` files during implementation.

---

## 8. ✅ Update arrow_json_conversion.rs

**Change**: Added UPDATE marker for `backend/crates/kalamdb-core/src/tables/arrow_json_conversion.rs`.

**Rationale**: This file will use new KalamDataType conversions instead of old type representations.

**Files Updated**:
- `plan.md`: Project structure with UPDATE comment

**Action Required**: Refactor `arrow_json_conversion.rs` to use `KalamDataType::to_arrow_type()` and `from_arrow_type()` methods.

---

## 9. ✅ Schema Models as Single Source of Truth

**Change**: Emphasized everywhere that consolidated schema models are the ONLY source of truth.

**Rationale**: All schema queries must use `kalamdb-commons/src/models/schemas/` models.

**Files Updated**:
- `spec.md`: Success criteria, key entities
- `plan.md`: Structure decision notes
- `data-model.md`: EntityStore location notes
- `quickstart.md`: "Single source of truth" emphasis

**Enforcement**:
- All DESCRIBE TABLE queries use TableDefinition
- All information_schema queries use TableDefinition
- All internal APIs use TableDefinition
- No legacy model references allowed

---

## Updated File Summary

| File | Changes | Lines Changed |
|------|---------|---------------|
| spec.md | EMBEDDING rename, FR-STORE updates, ColumnDefault params, TableType additions | ~50 |
| plan.md | Project structure rewrite, EntityStore location, complexity tracking | ~80 |
| research.md | Removed secondary indexes, added TableId section, EMBEDDING rename | ~100 |
| data-model.md | ER diagram update, TableType entity, ColumnDefault params, no indexes | ~150 |
| quickstart.md | EMBEDDING examples (sed replacement) | ~20 |

**Total Changes**: ~400 lines across 5 files

---

## Implementation Checklist

Before starting implementation, verify:

- [ ] EntityStore will be in `kalamdb-core/src/tables/system/schemas/`
- [ ] No secondary indexes will be created
- [ ] EMBEDDING type name used (not FLOAT_ARRAY)
- [ ] ColumnDefault supports `FunctionCall { name, args }`
- [ ] All 4 table types supported: SYSTEM/USER/SHARED/STREAM
- [ ] `tables_v2/` directory will be deleted
- [ ] `information_*.rs` files will be deleted
- [ ] `arrow_json_conversion.rs` will use KalamDataType
- [ ] Consolidated schema models are single source of truth

---

## Next Command

Run `/speckit.tasks` to generate executable task breakdown with these corrections incorporated.

---

**Updates Completed**: 2025-11-01  
**All Planning Documents Synchronized**: ✅  
**Ready for Task Generation**: ✅
