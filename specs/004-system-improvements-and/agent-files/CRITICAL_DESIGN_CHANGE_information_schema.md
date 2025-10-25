# Critical Design Change: Unified information_schema.tables

## Problem Statement

**Current fragmented design (REMOVED)**:
- `system_tables` - table metadata only
- `system_table_schemas` - schema versions separately
- `system_columns` - column metadata separately

**Issues**:
1. ❌ Multiple writes on CREATE TABLE (3 separate CF writes)
2. ❌ Multiple reads to get full table definition (3 CF reads + joins)
3. ❌ Inconsistency risk (partial writes on failure)
4. ❌ Complex ALTER TABLE (update 3 CFs atomically)
5. ❌ Harder to maintain referential integrity

## Solution: Single Source of Truth

**New unified design** (following MySQL/PostgreSQL `information_schema`):
- Single `information_schema_tables` column family
- One JSON document per table contains EVERYTHING
- Atomic writes/reads
- Easier to query, easier to maintain

## Data Model

### RocksDB Storage

**Column Family**: `information_schema_tables`

**Key Format**: `{namespace_id}:{table_name}`

**Value**: Complete table definition as JSON

```json
{
  "table_id": "namespace:table_name",
  "table_name": "users",
  "namespace_id": "app",
  "table_type": "USER",
  "created_at": 1729785600000,
  "updated_at": 1729785600000,
  "schema_version": 1,
  "storage_id": "local",
  "use_user_storage": false,
  "flush_policy": {
    "row_threshold": 10000,
    "interval_seconds": 300
  },
  "deleted_retention_hours": 72,
  "ttl_seconds": null,
  "columns": [
    {
      "column_name": "id",
      "ordinal_position": 1,
      "data_type": "Int64",
      "is_nullable": false,
      "column_default": "SNOWFLAKE_ID()",
      "is_primary_key": true
    },
    {
      "column_name": "email",
      "ordinal_position": 2,
      "data_type": "Utf8",
      "is_nullable": false,
      "column_default": null,
      "is_primary_key": false
    },
    {
      "column_name": "created_at",
      "ordinal_position": 3,
      "data_type": "Timestamp(Millisecond, None)",
      "is_nullable": false,
      "column_default": "NOW()",
      "is_primary_key": false
    }
  ],
  "schema_history": [
    {
      "version": 1,
      "created_at": 1729785600000,
      "changes": "Initial schema",
      "arrow_schema_json": "{...}"
    }
  ]
}
```

## Benefits

### 1. Atomic Operations
✅ CREATE TABLE = 1 write operation  
✅ ALTER TABLE = 1 read + 1 write (atomic update)  
✅ DROP TABLE = 1 delete operation  
✅ No partial state possible

### 2. Query Simplicity
```sql
-- Get complete table definition (1 read)
SELECT * FROM information_schema.tables 
WHERE table_schema = 'app' AND table_name = 'users';

-- Get all columns (parse JSON, no joins)
SELECT column_name, data_type, column_default
FROM information_schema.columns
WHERE table_schema = 'app' AND table_name = 'users'
ORDER BY ordinal_position;
```

### 3. ALTER TABLE Simplicity
```rust
// Read table definition
let mut table_def = get_table_definition(namespace, table_name)?;

// Modify in memory
table_def.columns.push(new_column);
table_def.schema_version += 1;
table_def.updated_at = now();

// Write back atomically
put_table_definition(namespace, table_name, &table_def)?;
```

### 4. MySQL/PostgreSQL Compatibility
Matches standard `information_schema.tables` and `information_schema.columns` views.

## Implementation Plan

### Phase 1: Create New Model (PRIORITY)

**File**: `backend/crates/kalamdb-commons/src/models.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    pub table_id: String,
    pub table_name: String,
    pub namespace_id: String,
    pub table_type: TableType,
    pub created_at: i64,
    pub updated_at: i64,
    pub schema_version: u32,
    pub storage_id: String,
    pub use_user_storage: bool,
    pub flush_policy: Option<FlushPolicyDef>,
    pub deleted_retention_hours: Option<u32>,
    pub ttl_seconds: Option<u64>,
    pub columns: Vec<ColumnDefinition>,
    pub schema_history: Vec<SchemaVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub column_name: String,
    pub ordinal_position: u32,
    pub data_type: String, // Arrow DataType as string
    pub is_nullable: bool,
    pub column_default: Option<String>, // DEFAULT expression
    pub is_primary_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaVersion {
    pub version: u32,
    pub created_at: i64,
    pub changes: String,
    pub arrow_schema_json: String,
}
```

### Phase 2: Update kalamdb-sql Adapter

**File**: `backend/crates/kalamdb-sql/src/adapter.rs`

Replace these methods:
- ❌ `insert_table()` 
- ❌ `insert_table_schema()`
- ❌ `insert_column_metadata()`

With single method:
```rust
/// Insert or update complete table definition
pub fn upsert_table_definition(&self, table_def: &TableDefinition) -> Result<()> {
    let cf = self.db.cf_handle("information_schema_tables")?;
    let key = format!("{}:{}", table_def.namespace_id, table_def.table_name);
    let value = serde_json::to_vec(table_def)?;
    self.db.put_cf(&cf, key.as_bytes(), &value)?;
    Ok(())
}

/// Get complete table definition
pub fn get_table_definition(
    &self, 
    namespace_id: &str, 
    table_name: &str
) -> Result<Option<TableDefinition>> {
    let cf = self.db.cf_handle("information_schema_tables")?;
    let key = format!("{}:{}", namespace_id, table_name);
    match self.db.get_cf(&cf, key.as_bytes())? {
        Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
        None => Ok(None),
    }
}
```

### Phase 3: Update CREATE TABLE Services

**Files**: 
- `backend/crates/kalamdb-core/src/services/user_table_service.rs`
- `backend/crates/kalamdb-core/src/services/shared_table_service.rs`
- `backend/crates/kalamdb-core/src/services/stream_table_service.rs`

Replace fragmented writes with single write:

```rust
pub fn create_table(&self, stmt: CreateTableStatement) -> Result<TableMetadata> {
    // Build complete table definition
    let table_def = TableDefinition {
        table_id: format!("{}:{}", stmt.namespace_id, stmt.table_name),
        table_name: stmt.table_name.as_str().to_string(),
        namespace_id: stmt.namespace_id.as_str().to_string(),
        table_type: stmt.table_type,
        created_at: now_millis(),
        updated_at: now_millis(),
        schema_version: 1,
        storage_id: stmt.storage_id.as_str().to_string(),
        columns: extract_columns_from_schema(&stmt.schema, &stmt.column_defaults),
        schema_history: vec![SchemaVersion {
            version: 1,
            created_at: now_millis(),
            changes: "Initial schema".to_string(),
            arrow_schema_json: serialize_arrow_schema(&stmt.schema)?,
        }],
        // ... other fields
    };
    
    // Single atomic write
    self.kalam_sql.upsert_table_definition(&table_def)?;
    
    Ok(TableMetadata::from(table_def))
}
```

### Phase 4: Update DataFusion Provider

**File**: `backend/crates/kalamdb-core/src/tables/system/information_schema_tables_provider.rs`

Create provider that queries `information_schema_tables` CF and exposes as:
1. `information_schema.tables` view (table-level metadata)
2. `information_schema.columns` view (flattened columns from JSON)

### Phase 5: Migration Strategy

**Option A: Clean slate** (if no production data):
1. Delete old CFs: `system_tables`, `system_table_schemas`, `system_columns`
2. Create new CF: `information_schema_tables`
3. Update all code to use new model

**Option B: Migration script** (if production data exists):
1. Read from old CFs
2. Merge into TableDefinition
3. Write to new CF
4. Verify
5. Drop old CFs

## SQL Compatibility

### Standard Queries (MySQL/PostgreSQL compatible)

```sql
-- List all tables in a schema
SELECT table_name, table_type, created_at
FROM information_schema.tables
WHERE table_schema = 'app';

-- Get table columns
SELECT column_name, data_type, is_nullable, column_default
FROM information_schema.columns
WHERE table_schema = 'app' AND table_name = 'users'
ORDER BY ordinal_position;

-- Get primary key columns
SELECT column_name
FROM information_schema.columns
WHERE table_schema = 'app' 
  AND table_name = 'users'
  AND is_primary_key = true;
```

## Auto-completion Support

**Single query to get all columns**:
```rust
let table_def = kalam_sql.get_table_definition(namespace, table_name)?;
let column_names: Vec<String> = table_def.columns
    .iter()
    .map(|c| c.column_name.clone())
    .collect();
```

## Task Updates Required

### Remove Obsolete Tasks
- ❌ T533 - Create system.columns table (NOT NEEDED)
- ❌ T533a - Register system_columns CF (NOT NEEDED)
- ❌ T533b - insert_column_metadata() (NOT NEEDED)
- ❌ T533c - Write column metadata separately (NOT NEEDED)
- ❌ All tasks related to system_table_schemas (REPLACED)

### New Tasks Required

#### T533-NEW: Create Unified information_schema Model
- [ ] Create TableDefinition struct in kalamdb-commons/src/models.rs
- [ ] Create ColumnDefinition struct
- [ ] Create SchemaVersion struct
- [ ] Add serde derives and JSON serialization

#### T534-NEW: Update kalamdb-sql Adapter
- [ ] Replace insert_table() with upsert_table_definition()
- [ ] Replace get_table() with get_table_definition()
- [ ] Remove insert_table_schema() (obsolete)
- [ ] Remove insert_column_metadata() (obsolete)
- [ ] Add scan_all_tables() for listing

#### T535-NEW: Update CREATE TABLE Services
- [ ] Update user_table_service.rs to build TableDefinition
- [ ] Update shared_table_service.rs to build TableDefinition
- [ ] Update stream_table_service.rs to build TableDefinition
- [ ] Single write to information_schema_tables

#### T536-NEW: Update ALTER TABLE Services
- [ ] Read TableDefinition
- [ ] Modify in memory
- [ ] Increment schema_version
- [ ] Add to schema_history
- [ ] Write back atomically

#### T537-NEW: Create information_schema Providers
- [ ] Create InformationSchemaTablesProvider
- [ ] Create InformationSchemaColumnsProvider (flattens JSON)
- [ ] Register with DataFusion

#### T538-NEW: Migration Script (if needed)
- [ ] Read from system_tables, system_table_schemas, system_columns
- [ ] Merge into TableDefinition
- [ ] Write to information_schema_tables
- [ ] Verify data integrity
- [ ] Drop old CFs

## Testing Strategy

```rust
#[test]
fn test_unified_table_definition() {
    let table_def = TableDefinition {
        table_name: "users".to_string(),
        columns: vec![
            ColumnDefinition {
                column_name: "id".to_string(),
                ordinal_position: 1,
                data_type: "Int64".to_string(),
                is_nullable: false,
                column_default: Some("SNOWFLAKE_ID()".to_string()),
                is_primary_key: true,
            },
        ],
        // ...
    };
    
    // Write
    kalam_sql.upsert_table_definition(&table_def)?;
    
    // Read back
    let retrieved = kalam_sql.get_table_definition("app", "users")?;
    assert_eq!(retrieved.unwrap().columns.len(), 1);
    
    // Query via SQL
    let rows = kalam_sql.execute(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'users'"
    )?;
    assert_eq!(rows.len(), 1);
}
```

## Files to Remove

1. ❌ `backend/crates/kalamdb-core/src/tables/system/columns.rs`
2. ❌ References to system_columns in mod.rs
3. ❌ `backend/PHASE_2B_COLUMN_METADATA.md`

## Files to Update

1. ✅ `backend/crates/kalamdb-core/src/storage/column_family_manager.rs` (DONE)
2. 🔄 `backend/crates/kalamdb-commons/src/models.rs` (add TableDefinition)
3. 🔄 `backend/crates/kalamdb-sql/src/adapter.rs` (new methods)
4. 🔄 All table services (user/shared/stream)
5. 🔄 DataFusion providers

## Backward Compatibility

**Breaking Change**: YES - requires database migration or fresh start

**Rationale**: Worth it for:
- Simpler code (less complexity)
- Better performance (fewer reads/writes)
- Standard compliance (MySQL/PostgreSQL compatibility)
- Easier maintenance

---

**Status**: 🔴 CRITICAL DESIGN CHANGE - Ready for implementation

**Next Step**: Get approval, then implement T533-NEW through T537-NEW
