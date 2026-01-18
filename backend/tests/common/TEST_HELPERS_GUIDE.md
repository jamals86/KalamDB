# Test Helpers Guide

This guide explains how to work with query responses in KalamDB tests.

## Core Principle

**Use `kalam_link::models::QueryResponse` built-in methods whenever possible.**

The `QueryResponse` type already provides safe, convenient methods for accessing query results. Only add test-specific helpers when the built-in methods aren't sufficient.

## Built-in QueryResponse Methods (from kalam_link)

```rust
use kalam_link::models::QueryResponse;

// Check if query succeeded
if response.success() { ... }

// Get all rows as Vec<HashMap<String, JsonValue>>
let rows = response.rows_as_maps();

// Get first row as HashMap
if let Some(row) = response.first_row_as_map() {
    let name = row.get("name");
}

// Get row count
let count = response.row_count();

// Get typed values from first row
let user_id = response.get_i64("user_id");      // Returns Option<i64>
let username = response.get_string("username"); // Returns Option<String>
let value = response.get_value("field");        // Returns Option<JsonValue>
```

## Common Test Helpers (from backend/tests/common)

Located in `backend/tests/common/testserver/query_helpers.rs`:

```rust
use super::test_support::{
    get_count_value, assert_query_success, assert_query_has_results, assert_row_count
};

// Get COUNT(*) value safely (tries multiple column names)
let count = get_count_value(&response, 0); // returns i64

// Assert query succeeded
assert_query_success(&response, "SELECT should succeed");

// Assert query succeeded and has results
assert_query_has_results(&response, "SELECT should return data");

// Assert specific row count
assert_row_count(&response, 10, "Should have 10 users");

// Get values with defaults
let id = get_i64_or_default(&response, "id", 0);
let name = get_string_or_default(&response, "name", "unknown");
```

## Migration from Old Patterns

### ❌ Old (Unsafe)
```rust
let rows = response.results[0].rows_as_maps();  // Panics if no results
let count = rows[0].get("count").unwrap();       // Panics if no rows
```

### ✅ New (Safe)
```rust
let rows = response.rows_as_maps();              // Returns empty vec if no results
let count = get_count_value(&response, 0);       // Returns default if not found
```

## When to Add New Helpers

Only add helpers to `query_helpers.rs` when:
1. The pattern is used in 3+ test files
2. The built-in `QueryResponse` methods don't cover it
3. It provides meaningful value over calling methods directly

## Import Guidelines

```rust
// Standard test imports
use super::test_support::{fixtures, TestServer, get_count_value};
use kalam_link::models::{QueryResponse, ResponseStatus};

// Only import HashMap if you actually need it for test logic
use std::collections::HashMap;
```

## Common Pitfalls

1. **Don't access `.results[0]` directly** - use `rows_as_maps()` or `first_row_as_map()`
2. **Don't unwrap() in tests** - use helpers with defaults or explicit error messages
3. **Don't duplicate helpers** - check if `QueryResponse` already has what you need
4. **Don't import QueryResultTestExt** - it's for internal use only

## Examples

### Count Query
```rust
let response = server.execute_sql("SELECT COUNT(*) as total FROM users").await;
assert_query_success(&response, "Count query");
let count = get_count_value(&response, 0);
assert_eq!(count, 10);
```

### Select with Typed Access
```rust
let response = server.execute_sql("SELECT id, name FROM users WHERE id = 1").await;
assert_query_has_results(&response, "User lookup");
let user_id = response.get_i64("id").expect("Should have user_id");
let name = response.get_string("name").unwrap_or_default();
```

### Multiple Rows
```rust
let response = server.execute_sql("SELECT * FROM users ORDER BY id").await;
assert_row_count(&response, 5, "Should have 5 users");
let rows = response.rows_as_maps();
for row in rows {
    let name = row.get("name").and_then(|v| v.as_str());
    println!("User: {:?}", name);
}
```
