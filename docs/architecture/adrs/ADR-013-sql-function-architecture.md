# ADR-013: SQL Function Architecture and DataFusion Alignment

**Status**: Accepted  
**Date**: 2025-10-25  
**Deciders**: KalamDB Core Team  
**Technical Story**: US15 - SQL Functions (DEFAULT, SELECT, WHERE contexts)

## Context and Problem Statement

KalamDB needs to support custom SQL functions (SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER) in multiple contexts:
- DEFAULT clauses in CREATE TABLE
- SELECT expressions for computed columns
- WHERE clauses for filtering

We need a unified function registration and execution architecture that integrates seamlessly with DataFusion's query engine while allowing KalamDB-specific extensions.

## Decision Drivers

1. **Leverage DataFusion**: Avoid reimplementing expression evaluation, type checking, and query optimization
2. **Type Safety**: Ensure function return types match column data types at CREATE TABLE time
3. **Volatility Control**: Distinguish between VOLATILE (new value each call), STABLE (constant per transaction), and IMMUTABLE functions
4. **Context Awareness**: Pass user context (user_id, namespace) to functions like CURRENT_USER()
5. **Performance**: Enable DataFusion's query optimization (constant folding, predicate pushdown)
6. **Extensibility**: Simple API for adding custom functions without modifying core engine

## Considered Options

### Option 1: Custom Function Registry (❌ Rejected)
Create KalamDB-specific `FunctionRegistry` with manual registration and evaluation.

**Pros**:
- Full control over function execution
- Custom type system integration

**Cons**:
- Reimplements DataFusion's expression system
- Loses DataFusion query optimization (constant folding, predicate pushdown)
- Requires custom WHERE/SELECT expression evaluator
- Higher maintenance burden

### Option 2: DataFusion ScalarUDF Integration (✅ Chosen)
Implement custom functions as `ScalarUDFImpl` and register directly with DataFusion `SessionContext`.

**Pros**:
- Seamless integration with DataFusion query engine
- Automatic support for WHERE, SELECT, and computed columns
- Type checking via DataFusion's type coercion system
- Query optimization works automatically (constant folding, predicate pushdown)
- Volatility control via `Signature::volatility()`
- Minimal code - no custom expression evaluator needed

**Cons**:
- Requires understanding DataFusion's UDF API
- Less flexibility for non-standard execution models

## Decision Outcome

**Chosen Option**: **Option 2 - DataFusion ScalarUDF Integration**

We implement all custom functions as `ScalarUDFImpl` traits and register them with DataFusion's `SessionContext.register_udf()`. This provides:

1. **Automatic Expression Evaluation**: Functions work in SELECT, WHERE, and DEFAULT contexts without custom code
2. **Type Safety**: DataFusion validates argument types and return types at query planning time
3. **Volatility Semantics**: 
   - `VOLATILE` (SNOWFLAKE_ID, UUID_V7, ULID, NOW): New value each execution
   - `STABLE` (CURRENT_USER): Constant per transaction
   - `IMMUTABLE`: Same input → same output (for future functions)
4. **Query Optimization**: DataFusion automatically optimizes queries using these functions
5. **Context Passing**: Use `KalamSessionState` to pass user_id to CURRENT_USER()

### Implementation Pattern

```rust
use datafusion::logical_expr::{ScalarUDFImpl, Signature, Volatility};
use arrow::datatypes::DataType;

pub struct SnowflakeIdFunction {
    node_id: u16,
    sequence: Arc<AtomicU16>,
}

impl ScalarUDFImpl for SnowflakeIdFunction {
    fn signature(&self) -> &Signature {
        &Signature::exact(vec![], Volatility::Volatile)
    }

    fn return_type(&self, _arg_types: &[DataType]) -> Result<DataType> {
        Ok(DataType::Int64)
    }

    fn invoke(&self, _args: &[ColumnarValue]) -> Result<ColumnarValue> {
        let id = self.generate_snowflake_id();
        Ok(ColumnarValue::Scalar(ScalarValue::Int64(Some(id))))
    }
}

// Registration (in datafusion_session.rs)
let snowflake_fn = Arc::new(ScalarUDF::from(SnowflakeIdFunction::new(node_id)));
session_ctx.register_udf(snowflake_fn);
```

### Function Volatility Classification

| Function | Volatility | Rationale |
|----------|-----------|-----------|
| `SNOWFLAKE_ID()` | VOLATILE | Generates unique ID each call (timestamp + sequence) |
| `UUID_V7()` | VOLATILE | Generates unique UUID each call (timestamp + random) |
| `ULID()` | VOLATILE | Generates unique ULID each call (time-sortable) |
| `NOW()` | VOLATILE | Returns current server timestamp (DataFusion built-in) |
| `CURRENT_TIMESTAMP()` | VOLATILE | Alias for NOW() (DataFusion built-in) |
| `CURRENT_USER()` | STABLE | Same user throughout transaction |

### DEFAULT Function Evaluation

For DEFAULT clauses, we:
1. Parse DEFAULT expressions during CREATE TABLE
2. Store function name in `ColumnDefinition.column_default`
3. During INSERT, detect omitted columns
4. Evaluate DEFAULT functions via DataFusion expression engine
5. Apply generated values before RocksDB write

This ensures consistent function behavior across all contexts (DEFAULT, SELECT, WHERE).

## Pros and Cons of the Decision

### Pros
- ✅ **Zero custom expression evaluation code**: DataFusion handles everything
- ✅ **Automatic query optimization**: Constant folding, predicate pushdown work immediately
- ✅ **Type safety**: DataFusion validates types at planning time
- ✅ **Consistent behavior**: Functions work identically in DEFAULT, SELECT, WHERE
- ✅ **Extensibility**: Adding new functions is trivial (implement ScalarUDFImpl, register)
- ✅ **Performance**: DataFusion can optimize STABLE functions (cache results)

### Cons
- ❌ **DataFusion dependency**: Tied to DataFusion's UDF API (acceptable tradeoff)
- ❌ **Learning curve**: Developers must understand DataFusion's type system

## Validation

### DEFAULT Context
```sql
CREATE USER TABLE events (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    correlation_id TEXT DEFAULT ULID(),
    created_at TIMESTAMP DEFAULT NOW()
);

INSERT INTO events VALUES ();  -- All DEFAULTs evaluated
```

### SELECT Context
```sql
SELECT 
    SNOWFLAKE_ID() as new_id,
    ULID() as tracking_id,
    NOW() as query_time
FROM events;
```

### WHERE Context
```sql
SELECT * FROM events 
WHERE created_at > NOW() - INTERVAL '1 hour';
```

All three contexts use the same function implementation without special handling.

## Links

- [DataFusion ScalarUDF Documentation](https://docs.rs/datafusion/latest/datafusion/logical_expr/trait.ScalarUDFImpl.html)
- [DataFusion Volatility Semantics](https://docs.rs/datafusion/latest/datafusion/logical_expr/enum.Volatility.html)
- Implementation: `/backend/crates/kalamdb-core/src/sql/functions/`
- Registration: `/backend/crates/kalamdb-core/src/sql/datafusion_session.rs`

## Follow-up

- Monitor DataFusion UDF API stability across versions
- Consider contributing KalamDB functions upstream to DataFusion if generally useful
- Explore DataFusion's aggregate UDF API for future window functions
