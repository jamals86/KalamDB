# ADR-016: NOT NULL Enforcement - Validation Timing and Atomicity

**Status**: Accepted  
**Date**: 2025-10-25  
**Deciders**: KalamDB Core Team  
**Technical Story**: US15 - NOT NULL Constraint Enforcement

## Context and Problem Statement

NOT NULL constraints must be enforced reliably to maintain data integrity. We need to decide:

1. **When to validate**: At parse time, planning time, or execution time?
2. **Where to validate**: In SQL parser, executor, or storage layer?
3. **Atomicity guarantee**: How to prevent partial writes on validation failure?
4. **Error messaging**: How to provide helpful diagnostics?
5. **Performance**: Minimize validation overhead

## Decision Drivers

1. **Data Integrity**: Zero tolerance for NULL values in NOT NULL columns
2. **Atomicity**: Validation failure must prevent any RocksDB write
3. **Performance**: Validation should be O(1) per column, not O(n) per table scan
4. **Error Quality**: Clear messages indicating which column failed and why
5. **Consistency**: Same validation logic for INSERT and UPDATE
6. **Simplicity**: Avoid complex rollback logic

## Considered Options

### Option 1: Parse-Time Validation (❌ Rejected)
Validate NOT NULL at SQL parse time.

**Pros**:
- Fastest failure (no execution needed)

**Cons**:
- Cannot validate dynamic values (expressions, subqueries)
- Parser doesn't have table schema context
- Doesn't handle UPDATE validation (row-dependent)

### Option 2: Storage-Layer Validation (❌ Rejected)
Let RocksDB/storage layer enforce NOT NULL during write.

**Pros**:
- Centralized validation point

**Cons**:
- RocksDB is schema-agnostic (doesn't know NOT NULL)
- Difficult rollback (partial writes may occur)
- Poor error messages (storage layer lacks SQL context)

### Option 3: Executor Pre-Write Validation (✅ Chosen)
Validate NOT NULL in executor before any RocksDB write.

**Pros**:
- Has full schema context (knows NOT NULL columns)
- Can validate expressions and computed values
- Prevents any RocksDB write on failure (atomic)
- Clear error messages with SQL context
- Works for both INSERT and UPDATE

**Cons**:
- Validation overhead during execution (acceptable cost)

## Decision Outcome

**Chosen Option**: **Option 3 - Executor Pre-Write Validation**

We validate NOT NULL constraints in the executor immediately before RocksDB writes. This ensures:
1. **Complete Context**: Executor has table schema and computed values
2. **Atomicity**: Validation failure prevents write (no partial state)
3. **Clear Errors**: SQL-level diagnostics with column names

### Architecture

```
┌────────────────────────────────────────────┐
│        SQL Executor (insert.rs)            │
├────────────────────────────────────────────┤
│ 1. Parse INSERT/UPDATE                     │
│ 2. Evaluate DEFAULT functions              │
│ 3. Build complete row(s)                   │
│ 4. ✅ VALIDATE NOT NULL (Pre-Write)        │
│    ├─ For each column in schema            │
│    ├─ If is_nullable == false              │
│    ├─ Check value != NULL                  │
│    └─ On NULL: return error, HALT          │
│ 5. Write to RocksDB (only if validation OK)│
└────────────────────────────────────────────┘
```

### Implementation

#### 1. Schema Storage
NOT NULL information stored in `ColumnDefinition`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ColumnDefinition {
    pub column_name: String,
    pub ordinal_position: usize,
    pub data_type: String,
    pub is_nullable: bool,  // ← NOT NULL flag
    pub column_default: Option<ColumnDefault>,
    pub is_primary_key: bool,
}
```

#### 2. Validation Function
Pre-write validator in executor:

```rust
fn validate_not_null_constraints(
    schema: &TableDefinition,
    row_batch: &RecordBatch,
) -> Result<(), ValidationError> {
    for (idx, column_def) in schema.columns.iter().enumerate() {
        if !column_def.is_nullable {
            let column_array = row_batch.column(idx);
            
            // Check for any NULL values in this column
            if column_array.null_count() > 0 {
                // Find first NULL position for error message
                for row_idx in 0..column_array.len() {
                    if column_array.is_null(row_idx) {
                        return Err(ValidationError::NotNullViolation {
                            column_name: column_def.column_name.clone(),
                            row_index: row_idx,
                        });
                    }
                }
            }
        }
    }
    
    Ok(())
}
```

#### 3. INSERT Execution Flow

```rust
pub async fn execute_insert(
    table_name: &str,
    columns: Vec<String>,
    values: Vec<Vec<Value>>,
    schema: &TableDefinition,
    db: &DB,
) -> Result<usize> {
    // Step 1: Evaluate DEFAULT functions for omitted columns
    let complete_rows = apply_defaults(columns, values, schema)?;
    
    // Step 2: Build Arrow RecordBatch
    let batch = build_record_batch(&complete_rows, schema)?;
    
    // Step 3: ✅ VALIDATE NOT NULL (pre-write)
    validate_not_null_constraints(schema, &batch)?;
    
    // Step 4: Write to RocksDB (only if validation passed)
    write_batch_to_rocksdb(table_name, &batch, db)?;
    
    Ok(batch.num_rows())
}
```

#### 4. UPDATE Execution Flow

```rust
pub async fn execute_update(
    table_name: &str,
    updates: HashMap<String, Value>,
    where_clause: Option<Expr>,
    schema: &TableDefinition,
    db: &DB,
) -> Result<usize> {
    // Step 1: Read rows matching WHERE clause
    let matching_rows = scan_table_with_filter(table_name, where_clause, db)?;
    
    // Step 2: Apply updates to rows
    let updated_batch = apply_updates(&matching_rows, &updates, schema)?;
    
    // Step 3: ✅ VALIDATE NOT NULL (pre-write)
    validate_not_null_constraints(schema, &updated_batch)?;
    
    // Step 4: Write updated rows back to RocksDB
    write_batch_to_rocksdb(table_name, &updated_batch, db)?;
    
    Ok(updated_batch.num_rows())
}
```

### Error Messages

We provide clear, actionable error messages:

```rust
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("NOT NULL violation: column '{column_name}' cannot be null (row {row_index})")]
    NotNullViolation {
        column_name: String,
        row_index: usize,
    },
}
```

**Example User Experience**:
```sql
INSERT INTO users (id, username, email) VALUES (1, 'john', NULL);
```

**Error Response**:
```
Error: NOT NULL violation: column 'email' cannot be null (row 0)

Hint: Column 'email' is defined as NOT NULL in table 'users'.
      Provide a non-null value or remove the NOT NULL constraint.
```

### Atomicity Guarantee

**Key Property**: Validation occurs **before** any RocksDB write.

```rust
// Pseudocode execution order
fn execute_insert() -> Result<usize> {
    // 1. Prepare data (in-memory)
    let batch = prepare_batch()?;
    
    // 2. Validate (in-memory, no I/O)
    validate_not_null_constraints(&batch)?;
    // ↑ If this fails, we return immediately
    // ↓ RocksDB write never executes
    
    // 3. Write to RocksDB (only if validation passed)
    write_batch_to_rocksdb(&batch)?;
    
    Ok(batch.num_rows())
}
```

**Failure Scenarios**:
- ✅ **Validation Fails**: Error returned, zero RocksDB writes
- ✅ **Partial Batch**: All rows validated before first write
- ✅ **Multi-Column**: All NOT NULL columns validated together

### Performance Characteristics

**Validation Cost**:
- **Per-Column Check**: O(1) - Check Arrow array `null_count()`
- **Per-Row Check**: O(n) - Scan array to find NULL position (only if null_count > 0)
- **Total Cost**: O(c) where c = number of NOT NULL columns

**Optimization**: Arrow's `null_count()` is cached, so checking for NULLs is very fast.

```rust
// Fast path: No NULLs in entire column
if column_array.null_count() == 0 {
    continue;  // Skip row-by-row check
}

// Slow path: Some NULLs present (error case)
for row_idx in 0..column_array.len() {
    if column_array.is_null(row_idx) {
        return Err(...);  // Report first NULL
    }
}
```

### Edge Cases

**1. All Columns Nullable**:
```sql
CREATE TABLE test (
    id BIGINT PRIMARY KEY,
    optional_field TEXT  -- Nullable by default
);

INSERT INTO test (id, optional_field) VALUES (1, NULL);
-- ✅ OK: optional_field is nullable
```

**2. PRIMARY KEY (Implicit NOT NULL)**:
```sql
CREATE TABLE test (
    id BIGINT PRIMARY KEY  -- Implicitly NOT NULL
);

INSERT INTO test (id) VALUES (NULL);
-- ❌ ERROR: NOT NULL violation: column 'id' cannot be null
```

**3. DEFAULT Function + NOT NULL**:
```sql
CREATE TABLE test (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    username TEXT NOT NULL
);

INSERT INTO test (username) VALUES ('john');
-- ✅ OK: id generated by DEFAULT, username provided
-- Step order:
-- 1. Evaluate DEFAULT SNOWFLAKE_ID() → id = 123456789
-- 2. Validate NOT NULL on id (123456789 ✅) and username ('john' ✅)
-- 3. Write to RocksDB
```

**4. UPDATE to NULL**:
```sql
UPDATE users SET email = NULL WHERE id = 1;
-- ❌ ERROR: NOT NULL violation: column 'email' cannot be null
-- No rows modified (validation prevents write)
```

**5. Batch INSERT with Partial Failure**:
```sql
INSERT INTO users (id, username, email) VALUES
    (1, 'alice', 'alice@example.com'),
    (2, 'bob', NULL),  -- ❌ Validation fails here
    (3, 'charlie', 'charlie@example.com');
    
-- Result: Zero rows inserted (atomic validation)
-- Error: NOT NULL violation: column 'email' cannot be null (row 1)
```

## Validation Tests

Integration tests verify NOT NULL enforcement:

```rust
#[tokio::test]
async fn test_not_null_enforcement_insert() {
    let server = TestServer::new();
    
    server.execute_sql(
        "CREATE USER TABLE users (
            id BIGINT PRIMARY KEY,
            email TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Attempt NULL insert
    let result = server.execute_sql(
        "INSERT INTO users (id, email) VALUES (1, NULL)"
    ).await;
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("NOT NULL"));
    
    // Verify zero rows inserted (atomicity)
    let count = server.query_sql("SELECT COUNT(*) FROM users").await.unwrap();
    assert_eq!(count[0]["count"], 0);
}

#[tokio::test]
async fn test_not_null_enforcement_update() {
    let server = TestServer::new();
    
    server.execute_sql(
        "CREATE USER TABLE users (
            id BIGINT PRIMARY KEY,
            email TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Insert valid row
    server.execute_sql(
        "INSERT INTO users (id, email) VALUES (1, 'test@example.com')"
    ).await.unwrap();
    
    // Attempt UPDATE to NULL
    let result = server.execute_sql(
        "UPDATE users SET email = NULL WHERE id = 1"
    ).await;
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("NOT NULL"));
    
    // Verify original value unchanged
    let email = server.query_sql("SELECT email FROM users WHERE id = 1")
        .await.unwrap()[0]["email"];
    assert_eq!(email, "test@example.com");
}
```

## Comparison with Other Databases

| Database | Validation Timing | Atomicity | Error Detail |
|----------|------------------|-----------|--------------|
| **KalamDB** | Pre-write executor | ✅ Full | Column name + row index |
| **PostgreSQL** | Pre-write executor | ✅ Full | Column name + row index |
| **MySQL** | During write | ⚠️ Partial (InnoDB transactions) | Column name |
| **SQLite** | Pre-write executor | ✅ Full | Column name |
| **MongoDB** | Optional (validation rules) | ⚠️ Document-level | Field path |

## Follow-up

- Add CHECK constraints (arbitrary boolean expressions)
- Add UNIQUE constraints (requires index scan)
- Add FOREIGN KEY constraints (cross-table validation)
- Consider asynchronous validation for large batches
- Add performance benchmarks for validation overhead

## Links

- Implementation: `/backend/crates/kalamdb-core/src/execution/insert.rs`
- Implementation: `/backend/crates/kalamdb-core/src/execution/update.rs`
- Tests: `/backend/tests/integration/schema/test_schema_integrity.rs`
- Related: ADR-015 (DEFAULT function evaluation timing)
