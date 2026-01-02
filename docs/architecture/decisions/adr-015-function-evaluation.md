# ADR-015: Function Evaluation Contexts - DEFAULT, SELECT, WHERE

**Status**: Accepted  
**Date**: 2025-10-25  
**Deciders**: KalamDB Core Team  
**Technical Story**: US15 - Unified Function Evaluation Across SQL Contexts

## Context and Problem Statement

SQL functions (SNOWFLAKE_ID, UUID_V7, ULID, NOW, CURRENT_USER) need to work consistently across three different execution contexts:

1. **DEFAULT Context**: Column defaults in CREATE TABLE, evaluated during INSERT
2. **SELECT Context**: Computed columns in query projections
3. **WHERE Context**: Filter predicates for row selection

We need a unified evaluation strategy that ensures:
- Consistent function behavior across all contexts
- Correct timing (when functions are evaluated)
- Proper context passing (user_id, transaction state)
- Performance optimization opportunities

## Decision Drivers

1. **Consistency**: Same function should behave identically in DEFAULT, SELECT, WHERE
2. **Performance**: Leverage query engine optimizations (constant folding, predicate pushdown)
3. **Timing Guarantees**: VOLATILE functions evaluated per-row, STABLE per-transaction
4. **Type Safety**: Validate function return types match column types at CREATE TABLE time
5. **Context Awareness**: Pass user context to CURRENT_USER(), transaction state to STABLE functions
6. **Simplicity**: Minimize custom evaluation code

## Decision Outcome

**We use DataFusion's unified expression evaluation for all three contexts.** This provides automatic support across DEFAULT, SELECT, and WHERE without context-specific code.

### Architecture

```
┌─────────────────────────────────────────────────┐
│            DataFusion Query Engine              │
│  (Handles Expression Evaluation in All Contexts)│
└─────────────────────────────────────────────────┘
                       │
         ┌─────────────┼─────────────┐
         │             │             │
    ┌────▼────┐   ┌───▼────┐   ┌───▼────┐
    │ DEFAULT │   │ SELECT │   │ WHERE  │
    │ Context │   │ Context│   │ Context│
    └─────────┘   └────────┘   └────────┘
         │             │             │
         └─────────────┴─────────────┘
                       │
         ┌─────────────▼──────────────┐
         │    ScalarUDF Registration  │
         │  (SNOWFLAKE_ID, UUID_V7,   │
         │   ULID, CURRENT_USER)      │
         └────────────────────────────┘
```

### 1. DEFAULT Context - Column Defaults

**Timing**: Evaluated during INSERT for omitted columns  
**Implementation**: Store function name in `ColumnDefinition.column_default`, evaluate via DataFusion

```sql
CREATE USER TABLE events (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    correlation_id TEXT DEFAULT ULID(),
    created_at TIMESTAMP DEFAULT NOW(),
    user_id TEXT DEFAULT CURRENT_USER()
);

-- INSERT without providing any columns
INSERT INTO events (event_type) VALUES ('login');

-- Execution flow:
-- 1. Detect omitted columns: id, correlation_id, created_at, user_id
-- 2. For each omitted column with DEFAULT:
--    - Build DataFusion expression: "SNOWFLAKE_ID()"
--    - Execute expression via SessionContext
--    - Collect generated value
-- 3. Apply all DEFAULT values before RocksDB write
-- 4. Single atomic write with complete row
```

**Key Properties**:
- ✅ **Atomic**: All defaults evaluated before any write
- ✅ **Consistent**: Same DataFusion evaluation as SELECT/WHERE
- ✅ **Type-Safe**: Validated at CREATE TABLE time
- ✅ **Context-Aware**: CURRENT_USER() receives user_id from session state

**Validation** (at CREATE TABLE):
```rust
// Verify DEFAULT function exists
let udf = session_ctx.udf("SNOWFLAKE_ID")?;

// Verify return type matches column type
if udf.return_type() != column.data_type {
    return Err("DEFAULT function type mismatch");
}

// Special validation for specific functions
match (function_name, column_type) {
    ("NOW", DataType::Timestamp) => Ok(()),
    ("SNOWFLAKE_ID", DataType::Int64) => Ok(()),
    ("UUID_V7" | "ULID", DataType::Utf8) => Ok(()),
    _ => Err("Invalid DEFAULT function for column type"),
}
```

### 2. SELECT Context - Computed Columns

**Timing**: Evaluated during query execution (per-row or per-batch)  
**Implementation**: DataFusion handles expression planning and execution automatically

```sql
SELECT 
    id,
    SNOWFLAKE_ID() as new_id,       -- VOLATILE: New value per row
    ULID() as tracking_id,           -- VOLATILE: New value per row
    NOW() as query_time,             -- VOLATILE: New value per execution
    CURRENT_USER() as username       -- STABLE: Same per transaction
FROM events;
```

**Execution Flow**:
1. Parser converts SQL to DataFusion LogicalPlan
2. Optimizer applies constant folding, common subexpression elimination
3. Physical planner creates execution plan
4. Expression evaluator runs function per-row or per-batch

**Optimization Opportunities**:
- **STABLE Functions**: DataFusion caches result for transaction duration
  ```sql
  -- CURRENT_USER() evaluated once, reused for all rows
  SELECT id, CURRENT_USER() FROM events;  
  ```
- **IMMUTABLE Functions**: DataFusion evaluates at planning time
  ```sql
  -- Constant folding: 1 + 1 evaluated at plan time, not runtime
  SELECT id, 1 + 1 as const_value FROM events;
  ```

### 3. WHERE Context - Filter Predicates

**Timing**: Evaluated during filtering (after table scan, before projection)  
**Implementation**: DataFusion predicate evaluation with pushdown optimization

```sql
-- Find recent events
SELECT * FROM events 
WHERE created_at > NOW() - INTERVAL '1 hour';

-- Find user's own events
SELECT * FROM events 
WHERE user_id = CURRENT_USER();

-- Time-range filtering with SNOWFLAKE_ID
SELECT * FROM events
WHERE id > snowflake_id_from_timestamp('2025-10-01T00:00:00Z');
```

**Execution Flow**:
1. Parse WHERE clause into DataFusion expression
2. Apply predicate pushdown if possible
3. Evaluate per-row during scan
4. Short-circuit: false predicate skips projection

**Optimization**:
- **Predicate Pushdown**: Push STABLE function evaluation before scan
  ```sql
  -- CURRENT_USER() evaluated once, used as constant in scan
  WHERE user_id = CURRENT_USER()  
  →  WHERE user_id = 'john@example.com' (constant after evaluation)
  ```

### Volatility Semantics

Functions are classified by volatility, controlling evaluation timing:

| Volatility | Behavior | Example | Caching |
|------------|----------|---------|---------|
| **VOLATILE** | New value every call | SNOWFLAKE_ID(), NOW() | Never cached |
| **STABLE** | Constant per transaction | CURRENT_USER() | Cached per session |
| **IMMUTABLE** | Same input → same output | UPPER(), LENGTH() | Fully cached |

```rust
impl ScalarUDFImpl for SnowflakeIdFunction {
    fn signature(&self) -> &Signature {
        // VOLATILE: new value every execution
        &Signature::exact(vec![], Volatility::Volatile)
    }
}

impl ScalarUDFImpl for CurrentUserFunction {
    fn signature(&self) -> &Signature {
        // STABLE: same value per transaction
        &Signature::exact(vec![], Volatility::Stable)
    }
}
```

### Context Passing - CURRENT_USER()

**Problem**: CURRENT_USER() needs authenticated user_id, which varies per session.

**Solution**: Store user context in DataFusion `SessionConfig` custom state.

```rust
// When creating SessionContext
let mut session_config = SessionConfig::new();
session_config = session_config.with_option(
    "kalamdb.user_id", 
    user_id.to_string()
);

let session_ctx = SessionContext::new_with_config(session_config);

// In CURRENT_USER() implementation
impl ScalarUDFImpl for CurrentUserFunction {
    fn invoke(&self, _args: &[ColumnarValue]) -> Result<ColumnarValue> {
        let user_id = self.session_state
            .config()
            .options()
            .get("kalamdb.user_id")?;
            
        Ok(ColumnarValue::Scalar(ScalarValue::Utf8(Some(user_id))))
    }
}
```

## Examples

### Complete Workflow Example

```sql
-- 1. CREATE TABLE with DEFAULT functions
CREATE USER TABLE audit_log (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    action TEXT NOT NULL,
    user_id TEXT DEFAULT CURRENT_USER(),
    created_at TIMESTAMP DEFAULT NOW()
);

-- 2. INSERT (DEFAULT context)
INSERT INTO audit_log (action) VALUES ('user_login');
-- Evaluates: SNOWFLAKE_ID(), CURRENT_USER(), NOW()
-- Result: (12345678901234567, 'user_login', 'john@example.com', '2025-10-25T10:30:00Z')

-- 3. SELECT (SELECT context)
SELECT 
    id,
    action,
    CURRENT_USER() as current_session_user,  -- STABLE: same as row user_id
    NOW() as query_time                       -- VOLATILE: current time
FROM audit_log;

-- 4. WHERE (WHERE context)
SELECT * FROM audit_log
WHERE user_id = CURRENT_USER()               -- STABLE: evaluated once
  AND created_at > NOW() - INTERVAL '1 hour'; -- VOLATILE: current time
```

### Edge Cases

**Multiple DEFAULT Functions**:
```sql
CREATE TABLE test (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    uuid TEXT DEFAULT UUID_V7(),
    ulid TEXT DEFAULT ULID()
);

INSERT INTO test VALUES ();
-- All three functions evaluated in single pass, then atomic write
```

**Override DEFAULT with Explicit Value**:
```sql
INSERT INTO audit_log (id, action) VALUES (999, 'test');
-- id=999 used (explicit), CURRENT_USER() and NOW() still evaluated
```

**NULL DEFAULT**:
```sql
CREATE TABLE test (
    id BIGINT PRIMARY KEY,
    optional TEXT DEFAULT NULL
);

INSERT INTO test (id) VALUES (1);
-- optional column receives NULL (no function evaluation)
```

## Performance Considerations

### DEFAULT Evaluation Cost
- **Per-INSERT Overhead**: ~1-5μs for function evaluation
- **Batching**: Evaluate all DEFAULT functions before single RocksDB write
- **Caching**: Not applicable (DEFAULT always evaluated)

### SELECT Optimization
- **STABLE Functions**: Cached per transaction (1 evaluation for entire result set)
- **VOLATILE Functions**: Evaluated per-row (acceptable for ID generation)
- **Constant Folding**: DataFusion eliminates redundant expressions

### WHERE Optimization
- **Predicate Pushdown**: STABLE functions converted to constants before scan
- **Short-Circuit**: false predicates skip expensive projections

## Validation

Integration tests verify consistent behavior:

```rust
#[test]
async fn test_function_consistency_across_contexts() {
    // 1. Test DEFAULT context
    execute("CREATE TABLE test (id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID())");
    execute("INSERT INTO test VALUES ()");
    let default_id = query("SELECT id FROM test")[0]["id"];
    
    // 2. Test SELECT context  
    let select_id = query("SELECT SNOWFLAKE_ID() as id")[0]["id"];
    
    // 3. Verify both are valid Snowflake IDs (same structure)
    assert!(default_id > 0);
    assert!(select_id > 0);
    assert_ne!(default_id, select_id); // VOLATILE: different values
}
```

## Links

- Implementation: `/backend/crates/kalamdb-core/src/execution/insert.rs` (DEFAULT evaluation)
- DataFusion: `/backend/crates/kalamdb-core/src/sql/datafusion_session.rs` (registration)
- Tests: `/backend/tests/integration/schema/test_schema_integrity.rs`
- ADR-013: SQL Function Architecture (ScalarUDF design)
- ADR-014: ID Generation Functions (function-specific details)
