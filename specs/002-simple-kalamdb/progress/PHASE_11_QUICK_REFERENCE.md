# Phase 11 Quick Reference: ALTER TABLE

## Syntax

### Add Column
```sql
ALTER TABLE users ADD COLUMN age INT;
ALTER TABLE users ADD COLUMN email VARCHAR DEFAULT 'unknown@example.com';
ALTER TABLE myapp.users ADD COLUMN score FLOAT;
```

### Drop Column
```sql
ALTER TABLE users DROP COLUMN age;
ALTER TABLE myapp.users DROP COLUMN temp_field;
```

### Modify Column Type
```sql
ALTER TABLE users MODIFY COLUMN age BIGINT;        -- Int32 → Int64 (allowed)
ALTER TABLE users MODIFY COLUMN score DOUBLE;      -- Float32 → Float64 (allowed)
ALTER TABLE users MODIFY COLUMN name VARCHAR;      -- Change type
```

### Describe Table (with history)
```sql
DESCRIBE TABLE users;              -- Show current schema
DESCRIBE TABLE users HISTORY;      -- Show all schema versions
DESC TABLE users;                  -- Shorthand
DESC TABLE myapp.users HISTORY;    -- Qualified name with history
```

## Parser API

```rust
use kalamdb_core::sql::ddl::alter_table::{AlterTableStatement, ColumnOperation};

// Parse ALTER TABLE
let stmt = AlterTableStatement::parse("ALTER TABLE users ADD COLUMN age INT")?;

match stmt.operation {
    ColumnOperation::Add { column_name, data_type, default_value } => {
        println!("Adding column: {} of type {}", column_name, data_type);
    }
    ColumnOperation::Drop { column_name } => {
        println!("Dropping column: {}", column_name);
    }
    ColumnOperation::Modify { column_name, new_data_type } => {
        println!("Modifying column: {} to {}", column_name, new_data_type);
    }
}
```

## Service API

```rust
use kalamdb_core::services::schema_evolution_service::SchemaEvolutionService;
use kalamdb_core::catalog::{NamespaceId, TableName};
use kalamdb_sql::KalamSql;
use std::sync::Arc;

// Create service
let kalam_sql = Arc::new(KalamSql::new(db.clone()));
let service = SchemaEvolutionService::new(kalam_sql.clone());

// Execute ALTER TABLE
let stmt = AlterTableStatement::parse(
    "ALTER TABLE users ADD COLUMN age INT"
)?;

service.alter_table(
    stmt.namespace_id,
    stmt.table_name,
    stmt.operation
)?;
```

## Validation Rules

### ✅ Allowed Operations

1. **Add Column**
   - Any data type
   - Optional DEFAULT value
   - Cannot add system columns (_updated, _deleted)

2. **Drop Column**
   - Must not be primary key
   - Must not be used by active live queries
   - Cannot drop system columns (_updated, _deleted)

3. **Modify Column Type**
   - Int32 → Int64 (widening)
   - Float32 → Float64 (widening)
   - String type changes
   - Cannot modify system columns
   - Cannot modify primary key type

### ❌ Rejected Operations

1. **System Column Modification**
   ```sql
   ALTER TABLE users DROP COLUMN _updated;    -- ERROR
   ALTER TABLE users MODIFY COLUMN _deleted;  -- ERROR
   ```

2. **Stream Table Alteration**
   ```sql
   ALTER TABLE events ADD COLUMN data VARCHAR;  -- ERROR (if events is stream)
   ```

3. **Active Subscription Conflicts**
   ```sql
   ALTER TABLE users DROP COLUMN name;  -- ERROR (if live query uses 'name')
   ```

4. **Primary Key Modification**
   ```sql
   ALTER TABLE users DROP COLUMN user_id;  -- ERROR (if user_id is PK)
   ```

5. **Type Narrowing**
   ```sql
   ALTER TABLE users MODIFY COLUMN age INT;  -- ERROR (if currently BIGINT)
   ALTER TABLE users MODIFY COLUMN score FLOAT;  -- ERROR (if currently DOUBLE)
   ```

## Schema Versioning

### Automatic Version Tracking

Every ALTER TABLE creates a new schema version:

```
Version 1: Initial schema
  - id INT (PK)
  - name VARCHAR
  - _updated TIMESTAMP
  - _deleted BOOLEAN

Version 2: After "ALTER TABLE users ADD COLUMN age INT"
  - id INT (PK)
  - name VARCHAR
  - age INT
  - _updated TIMESTAMP
  - _deleted BOOLEAN

Version 3: After "ALTER TABLE users DROP COLUMN name"
  - id INT (PK)
  - age INT
  - _updated TIMESTAMP
  - _deleted BOOLEAN
```

### Schema History Storage

- **Column Family**: `system_table_schemas`
- **Key**: `{table_id}:{version}`
- **Value**: `ArrowSchemaWithOptions` (JSON-serialized Arrow schema)

### Query Schema History

```rust
use kalamdb_sql::KalamSql;

let kalam_sql = KalamSql::new(db);

// Get all schema versions for a table
let schemas = kalam_sql.get_table_schemas_for_table(table_id)?;

for schema in schemas {
    println!("Version {}: {} columns at {}",
        schema.version,
        schema.schema_json.fields.len(),
        schema.created_at
    );
}
```

## Backwards Compatibility (Schema Projection)

### Reading Old Parquet Files

```rust
use kalamdb_core::schema::projection::{project_batch, schemas_compatible};
use arrow::record_batch::RecordBatch;
use arrow::datatypes::SchemaRef;

// Check if old schema can be projected to new schema
if schemas_compatible(&old_schema, &new_schema)? {
    // Read old Parquet file
    let old_batch = read_parquet_file(path)?;
    
    // Project to current schema
    let new_batch = project_batch(old_batch, &new_schema)?;
    
    // new_batch now matches current schema:
    // - Added columns filled with NULL
    // - Dropped columns ignored
    // - Type widening applied
}
```

### Projection Rules

1. **Added Columns**: Fill with NULL
2. **Dropped Columns**: Ignore (data discarded)
3. **Type Widening**: Cast values (Int32→Int64, Float32→Float64)
4. **Reordered Columns**: Handled automatically
5. **Type Narrowing**: Rejected as incompatible

## DESCRIBE TABLE Enhancement

### Show Current Schema
```sql
DESCRIBE TABLE users;
```

**Output Fields**:
- `namespace_id`: Optional namespace
- `table_name`: Table name
- `show_history`: false

### Show Schema History
```sql
DESCRIBE TABLE users HISTORY;
```

**Output Fields**:
- `namespace_id`: Optional namespace
- `table_name`: Table name
- `show_history`: true

**Parser API**:
```rust
use kalamdb_core::sql::ddl::describe_table::DescribeTableStatement;

let stmt = DescribeTableStatement::parse("DESCRIBE TABLE users HISTORY")?;

if stmt.show_history {
    // Fetch and display all schema versions
    let schemas = kalam_sql.get_table_schemas_for_table(table_id)?;
    // Display: version | created_at | changes | column_count
} else {
    // Show current schema only
    let table = kalam_sql.get_table(table_id)?;
    // Display: column_name | data_type | constraints
}
```

## Testing

### Run Phase 11 Tests
```bash
cd backend

# All Phase 11 tests
cargo test --lib -- alter_table schema_evolution projection describe_table

# Specific test suites
cargo test --lib -- alter_table           # 12 tests
cargo test --lib -- schema_evolution       # 9 tests
cargo test --lib -- projection             # 9 tests
cargo test --lib -- describe_table         # 11 tests
```

### Example Test Case
```rust
#[test]
fn test_add_column() {
    let db = TestDb::new(&["system_tables", "system_table_schemas"]);
    let kalam_sql = Arc::new(KalamSql::new(db.db.clone()));
    let service = SchemaEvolutionService::new(kalam_sql.clone());

    // Create test table with initial schema
    let table_id = setup_test_table(&kalam_sql);

    // Parse and execute ALTER TABLE
    let stmt = AlterTableStatement::parse(
        "ALTER TABLE users ADD COLUMN age INT"
    ).unwrap();

    service.alter_table(
        None,
        TableName::new("users"),
        stmt.operation
    ).unwrap();

    // Verify schema updated
    let table = kalam_sql.get_table(table_id).unwrap();
    assert_eq!(table.schema_version, 2);
    
    // Verify new column exists
    let schema: ArrowSchemaWithOptions = serde_json::from_str(&table.arrow_schema).unwrap();
    assert!(schema.fields.iter().any(|f| f.name == "age"));
}
```

## Error Handling

```rust
use kalamdb_core::error::KalamDbError;

match service.alter_table(namespace_id, table_name, operation) {
    Ok(_) => println!("Schema updated successfully"),
    Err(KalamDbError::InvalidSchemaEvolution(msg)) => {
        eprintln!("Schema evolution failed: {}", msg);
    }
    Err(KalamDbError::ActiveSubscriptions(msg)) => {
        eprintln!("Cannot alter: {}", msg);
    }
    Err(KalamDbError::TableNotFound { .. }) => {
        eprintln!("Table does not exist");
    }
    Err(e) => eprintln!("Error: {:?}", e),
}
```

## Common Error Messages

```
"Cannot alter system columns: _updated, _deleted"
"Cannot alter stream tables (table_type: Stream)"
"Cannot drop column 'name' - used by 2 active live queries"
"Cannot drop primary key column 'id'"
"Type narrowing not supported: BIGINT -> INT"
"Incompatible type change: VARCHAR -> INT"
```

## Files Reference

```
backend/crates/kalamdb-core/src/
├── sql/ddl/
│   ├── alter_table.rs         # ALTER TABLE parser
│   └── describe_table.rs      # Enhanced DESCRIBE TABLE parser
├── services/
│   └── schema_evolution_service.rs  # ALTER TABLE execution
└── schema/
    └── projection.rs          # Backwards compatibility
```

## Next Steps

After implementing ALTER TABLE, typical workflow:

1. **Integrate with SQL Executor** (T183)
   - Add cache invalidation after ALTER TABLE
   - Reload DataFusion TableProvider with new schema

2. **Implement DESCRIBE TABLE Execution**
   - Fetch schema history from kalamdb-sql
   - Format and display version information

3. **Test with Real Data**
   - Create table with data
   - ALTER TABLE ADD COLUMN
   - Verify old Parquet files readable
   - Insert new data with new column

4. **Add Constraints Support** (Future)
   - UNIQUE constraints
   - CHECK constraints
   - FOREIGN KEY constraints
