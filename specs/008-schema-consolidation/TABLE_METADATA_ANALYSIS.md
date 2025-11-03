# TableMetadata Analysis and Recommendation

## Current Situation

There are two similar but distinct structures:

### 1. `TableDefinition` (kalamdb-commons)
**Purpose**: Schema definition (single source of truth for table structure)

**Fields**:
- `namespace_id`, `table_name`, `table_type`
- `columns: Vec<ColumnDefinition>` - **Schema information**
- `schema_version`, `schema_history` - **Version tracking**
- `table_options: TableOptions` - **Type-safe options**
- `table_comment`, `created_at`, `updated_at`

### 2. `TableMetadata` (kalamdb-core)
**Purpose**: Runtime table metadata

**Fields**:
- `table_name`, `table_type`, `namespace`
- `created_at`
- `storage_location: String` - **Runtime/Storage info**
- `flush_policy: FlushPolicy` - **Runtime/Storage info**
- `schema_version: u32` - **DUPLICATE with TableDefinition**
- `deleted_retention_hours: Option<u32>` - **Could be in TableOptions**

## Analysis

### Not a Complete Duplicate ✅

These structures serve **different purposes**:
- **TableDefinition** = What the table looks like (schema)
- **TableMetadata** = Where/how the table is stored (runtime)

### Issues Found 🔴

1. **Schema version duplication**: Both have `schema_version`
2. **Deleted retention**: Should be in `SharedTableOptions` instead of `TableMetadata`
3. **Missing link**: No reference from `TableMetadata` → `TableDefinition`
4. **Flush policy**: Conceptually overlaps with `TableOptions` compression/compaction settings

## Recommendation

### Option A: Keep Both, Fix Duplication (RECOMMENDED)

**TableDefinition** (commons - schema only):
```rust
pub struct TableDefinition {
    pub namespace_id: String,
    pub table_name: String,
    pub table_type: TableType,
    pub columns: Vec<ColumnDefinition>,  // Schema
    pub schema_version: u32,
    pub schema_history: Vec<SchemaVersion>,
    pub table_options: TableOptions,     // Type-specific options
    pub table_comment: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**TableMetadata** (core - runtime/storage only):
```rust
pub struct TableMetadata {
    pub table_name: TableName,
    pub table_type: TableType,
    pub namespace: NamespaceId,
    pub created_at: DateTime<Utc>,
    
    // Runtime/Storage fields only
    pub storage_location: String,
    pub flush_policy: FlushPolicy,
    
    // Link to schema
    pub schema_ref: Option<TableId>,  // Reference to TableDefinition
}
```

**Changes**:
1. Remove `schema_version` from `TableMetadata` (use `TableDefinition` instead)
2. Remove `deleted_retention_hours` from `TableMetadata` (move to `SharedTableOptions`)
3. Add `schema_ref` to link `TableMetadata` → `TableDefinition`
4. Keep `storage_location` and `flush_policy` in `TableMetadata` (runtime concerns)

### Option B: Merge Into TableDefinition (NOT RECOMMENDED)

Add runtime fields to `TableDefinition`:
- Violates separation of concerns
- Mixes schema (pure data structure) with runtime (deployment-specific)
- Makes `TableDefinition` less portable across environments

### Option C: Remove TableMetadata (NOT VIABLE)

Cannot remove because:
- `storage_location` is actively used (20+ usages)
- `flush_policy` is runtime configuration, not schema
- Backup service depends on these fields

## Action Items

### Phase 7 (Current) - Document Only
- ✅ Create this analysis document
- ⏸️ Defer refactoring to post-merge (not blocking)

### Post-Merge - Refactoring
1. Add `deleted_retention_hours` to `SharedTableOptions`
2. Remove `deleted_retention_hours` from `TableMetadata`
3. Remove `schema_version` from `TableMetadata`
4. Add `schema_ref: TableId` to `TableMetadata`
5. Update all usages to fetch schema from `TableDefinition` via `TableSchemaStore`

## Conclusion

**Status**: ✅ **NOT a duplicate** - serves different purpose

**Decision**: Keep both structures, document the distinction

**Rationale**:
- TableDefinition = schema (portable, version-controlled)
- TableMetadata = runtime configuration (deployment-specific)
- Separation of concerns is architecturally sound
- Minor overlaps (schema_version, deleted_retention) can be cleaned up post-merge

**Priority**: P2 (cleanup task, not blocking Phase 7 completion)
