# Error Codes Reference

**Feature**: 011-sql-handlers-prep  
**Date**: 2025-11-07

## Error Code Taxonomy

All error codes follow the format: `CATEGORY_SPECIFIC_ERROR`

---

## Parameter Validation Errors (400 Bad Request)

### PARAM_COUNT_MISMATCH
**Description**: Number of parameters doesn't match number of placeholders in SQL

**Details Fields**:
- `expected`: Number of placeholders found (e.g., 2 for `$1` and `$2`)
- `actual`: Number of parameters provided
- `placeholders`: Array of placeholder strings (e.g., `["$1", "$2"]`)

**Example**:
```json
{
  "code": "PARAM_COUNT_MISMATCH",
  "message": "Parameter count mismatch: expected 2 parameters, got 3",
  "details": {
    "expected": 2,
    "actual": 3,
    "placeholders": ["$1", "$2"]
  }
}
```

---

### PARAM_COUNT_EXCEEDED
**Description**: Total parameter count exceeds maximum allowed (50)

**Details Fields**:
- `max`: Maximum allowed parameters (50)
- `actual`: Number of parameters provided

**Example**:
```json
{
  "code": "PARAM_COUNT_EXCEEDED",
  "message": "Parameter count exceeded: maximum 50 parameters allowed, got 51",
  "details": {
    "max": 50,
    "actual": 51
  }
}
```

---

### PARAM_SIZE_EXCEEDED
**Description**: Individual parameter size exceeds maximum allowed (512KB)

**Details Fields**:
- `index`: Zero-based parameter index
- `size_bytes`: Actual parameter size in bytes
- `max_bytes`: Maximum allowed size (524288 bytes = 512KB)

**Example**:
```json
{
  "code": "PARAM_SIZE_EXCEEDED",
  "message": "Parameter size exceeded: parameter at index 0 is 614400 bytes (max 524288 bytes)",
  "details": {
    "index": 0,
    "size_bytes": 614400,
    "max_bytes": 524288
  }
}
```

---

### PARAM_TYPE_MISMATCH
**Description**: Parameter type incompatible with table schema column type

**Details Fields**:
- `index`: Zero-based parameter index
- `placeholder`: Placeholder string (e.g., `$1`)
- `expected_type`: Column data type from schema (e.g., `Int64`)
- `actual_type`: Parameter data type (e.g., `Utf8`)
- `column_name`: Target column name (if known)

**Example**:
```json
{
  "code": "PARAM_TYPE_MISMATCH",
  "message": "Parameter type mismatch: $1 expects Int64 but got Utf8",
  "details": {
    "index": 0,
    "placeholder": "$1",
    "expected_type": "Int64",
    "actual_type": "Utf8",
    "column_name": "user_id"
  }
}
```

---

### PARAMS_NOT_SUPPORTED
**Description**: Statement type doesn't support parameterized execution

**Details Fields**:
- `statement_type`: SQL statement type (e.g., `CreateNamespace`)
- `supported_types`: Array of statement types that support parameters

**Example**:
```json
{
  "code": "PARAMS_NOT_SUPPORTED",
  "message": "Parameters are not supported for CREATE NAMESPACE statements",
  "details": {
    "statement_type": "CreateNamespace",
    "supported_types": ["SELECT", "INSERT", "UPDATE", "DELETE"]
  }
}
```

---

### INVALID_SQL_SYNTAX
**Description**: SQL parsing failed due to syntax error

**Details Fields**:
- `line`: Line number where error occurred (1-indexed)
- `column`: Column number where error occurred (1-indexed)
- `token`: Token that caused the error (if available)
- `expected`: What the parser expected (if available)

**Example**:
```json
{
  "code": "INVALID_SQL_SYNTAX",
  "message": "SQL syntax error at line 1, column 15: expected identifier, got ';'",
  "details": {
    "line": 1,
    "column": 15,
    "token": ";",
    "expected": "identifier"
  }
}
```

---

## Authorization Errors (403 Forbidden)

### AUTH_INSUFFICIENT_ROLE
**Description**: User's role lacks required permissions for operation

**Details Fields**:
- `required_roles`: Array of roles that can perform operation (e.g., `["DBA", "System"]`)
- `actual_role`: User's current role (e.g., `User`)
- `operation`: SQL statement type

**Example**:
```json
{
  "code": "AUTH_INSUFFICIENT_ROLE",
  "message": "Insufficient role: CREATE NAMESPACE requires DBA or System role, but user has User role",
  "details": {
    "required_roles": ["DBA", "System"],
    "actual_role": "User",
    "operation": "CREATE NAMESPACE"
  }
}
```

---

### AUTH_NAMESPACE_ACCESS_DENIED
**Description**: User doesn't have access to specified namespace

**Details Fields**:
- `namespace`: Namespace identifier
- `allowed_namespaces`: Array of namespaces user can access
- `operation`: SQL statement type

**Example**:
```json
{
  "code": "AUTH_NAMESPACE_ACCESS_DENIED",
  "message": "Access denied to namespace 'prod': user only has access to ['dev', 'test']",
  "details": {
    "namespace": "prod",
    "allowed_namespaces": ["dev", "test"],
    "operation": "SELECT"
  }
}
```

---

## Resource Not Found Errors (404 Not Found)

### NOT_FOUND_TABLE
**Description**: Table doesn't exist in specified namespace

**Details Fields**:
- `table_name`: Qualified or unqualified table name
- `namespace`: Namespace where table was searched
- `operation`: SQL statement type

**Example**:
```json
{
  "code": "NOT_FOUND_TABLE",
  "message": "Table 'users' not found in namespace 'default'",
  "details": {
    "table_name": "users",
    "namespace": "default",
    "operation": "SELECT"
  }
}
```

---

### NOT_FOUND_NAMESPACE
**Description**: Namespace doesn't exist

**Details Fields**:
- `namespace`: Namespace identifier
- `operation`: SQL statement type

**Example**:
```json
{
  "code": "NOT_FOUND_NAMESPACE",
  "message": "Namespace 'prod' not found",
  "details": {
    "namespace": "prod",
    "operation": "SHOW TABLES"
  }
}
```

---

### NOT_FOUND_STORAGE
**Description**: Storage backend doesn't exist

**Details Fields**:
- `storage_id`: Storage identifier
- `operation`: SQL statement type

**Example**:
```json
{
  "code": "NOT_FOUND_STORAGE",
  "message": "Storage 's3-bucket-prod' not found",
  "details": {
    "storage_id": "s3-bucket-prod",
    "operation": "ALTER STORAGE"
  }
}
```

---

### NOT_FOUND_USER
**Description**: User doesn't exist

**Details Fields**:
- `username`: Username or user ID
- `operation`: SQL statement type

**Example**:
```json
{
  "code": "NOT_FOUND_USER",
  "message": "User 'alice' not found",
  "details": {
    "username": "alice",
    "operation": "ALTER USER"
  }
}
```

---

## Timeout Errors (408 Request Timeout)

### TIMEOUT_HANDLER_EXECUTION
**Description**: Handler execution exceeded configured timeout

**Details Fields**:
- `timeout_seconds`: Configured timeout value
- `elapsed_ms`: Actual elapsed time in milliseconds
- `statement_type`: SQL statement type

**Example**:
```json
{
  "code": "TIMEOUT_HANDLER_EXECUTION",
  "message": "Handler execution exceeded 30s timeout",
  "details": {
    "timeout_seconds": 30,
    "elapsed_ms": 30042,
    "statement_type": "SELECT"
  }
}
```

---

### TIMEOUT_QUERY_PLANNING
**Description**: DataFusion query planning phase exceeded timeout

**Details Fields**:
- `timeout_seconds`: Configured timeout value
- `elapsed_ms`: Actual elapsed time in milliseconds
- `phase`: Planning phase that timed out (e.g., `optimization`)

**Example**:
```json
{
  "code": "TIMEOUT_QUERY_PLANNING",
  "message": "Query planning exceeded timeout during optimization phase",
  "details": {
    "timeout_seconds": 30,
    "elapsed_ms": 30156,
    "phase": "optimization"
  }
}
```

---

## Not Implemented Errors (501 Not Implemented)

### NOT_IMPLEMENTED_TRANSACTION
**Description**: Transaction support not yet implemented

**Details Fields**:
- `feature`: Specific transaction operation (e.g., `BEGIN TRANSACTION`)
- `planned_phase`: Phase when feature will be implemented

**Example**:
```json
{
  "code": "NOT_IMPLEMENTED_TRANSACTION",
  "message": "Transaction support planned for Phase 11",
  "details": {
    "feature": "BEGIN TRANSACTION",
    "planned_phase": "Phase 11"
  }
}
```

---

### NOT_IMPLEMENTED_FEATURE
**Description**: Generic feature not yet implemented

**Details Fields**:
- `feature`: Feature name or operation
- `planned_phase`: Phase when feature will be implemented (if known)

**Example**:
```json
{
  "code": "NOT_IMPLEMENTED_FEATURE",
  "message": "Named parameters not yet supported (only positional $1, $2, ...)",
  "details": {
    "feature": "Named parameters",
    "planned_phase": "TBD"
  }
}
```

---

## Internal Errors (500 Internal Server Error)

### INTERNAL_DATAFUSION_ERROR
**Description**: DataFusion execution error (query planning, optimization, execution)

**Details Fields**:
- `datafusion_error`: DataFusion error message
- `phase`: Execution phase (e.g., `planning`, `optimization`, `execution`)
- `statement_type`: SQL statement type

**Example**:
```json
{
  "code": "INTERNAL_DATAFUSION_ERROR",
  "message": "DataFusion execution error during query planning",
  "details": {
    "datafusion_error": "Schema inference failed: column 'unknown_col' not found",
    "phase": "planning",
    "statement_type": "SELECT"
  }
}
```

---

### INTERNAL_STORAGE_ERROR
**Description**: Storage layer error (RocksDB, Parquet, file system)

**Details Fields**:
- `storage_error`: Storage error message
- `operation`: Storage operation that failed (e.g., `write`, `read`, `flush`)
- `table_name`: Affected table (if applicable)

**Example**:
```json
{
  "code": "INTERNAL_STORAGE_ERROR",
  "message": "Storage write failed: disk full",
  "details": {
    "storage_error": "No space left on device",
    "operation": "write",
    "table_name": "users"
  }
}
```

---

### INTERNAL_HANDLER_ERROR
**Description**: Generic handler execution error

**Details Fields**:
- `handler`: Handler type (e.g., `CreateNamespaceHandler`)
- `error_message`: Error message
- `statement_type`: SQL statement type

**Example**:
```json
{
  "code": "INTERNAL_HANDLER_ERROR",
  "message": "Handler execution failed unexpectedly",
  "details": {
    "handler": "CreateNamespaceHandler",
    "error_message": "Failed to generate namespace ID",
    "statement_type": "CREATE NAMESPACE"
  }
}
```

---

## Error Code Summary Table

| Code | HTTP Status | Category | Retryable |
|------|-------------|----------|-----------|
| PARAM_COUNT_MISMATCH | 400 | Validation | No |
| PARAM_COUNT_EXCEEDED | 400 | Validation | No |
| PARAM_SIZE_EXCEEDED | 400 | Validation | No |
| PARAM_TYPE_MISMATCH | 400 | Validation | No |
| PARAMS_NOT_SUPPORTED | 400 | Validation | No |
| INVALID_SQL_SYNTAX | 400 | Validation | No |
| AUTH_INSUFFICIENT_ROLE | 403 | Authorization | No |
| AUTH_NAMESPACE_ACCESS_DENIED | 403 | Authorization | No |
| NOT_FOUND_TABLE | 404 | Resource | No |
| NOT_FOUND_NAMESPACE | 404 | Resource | No |
| NOT_FOUND_STORAGE | 404 | Resource | No |
| NOT_FOUND_USER | 404 | Resource | No |
| TIMEOUT_HANDLER_EXECUTION | 408 | Timeout | Yes (with longer timeout) |
| TIMEOUT_QUERY_PLANNING | 408 | Timeout | Yes (with longer timeout) |
| NOT_IMPLEMENTED_TRANSACTION | 501 | Not Implemented | No |
| NOT_IMPLEMENTED_FEATURE | 501 | Not Implemented | No |
| INTERNAL_DATAFUSION_ERROR | 500 | Internal | Maybe (depends on cause) |
| INTERNAL_STORAGE_ERROR | 500 | Internal | Maybe (depends on cause) |
| INTERNAL_HANDLER_ERROR | 500 | Internal | Maybe (depends on cause) |

---

## Client Retry Guidelines

**DO NOT RETRY** (4xx errors except 408):
- Parameter validation errors (fix request parameters)
- Authorization errors (check user permissions)
- Resource not found errors (create resource or use correct identifier)
- Not implemented errors (feature unavailable)

**RETRY WITH BACKOFF** (408, 5xx):
- Timeout errors (consider increasing timeout or optimizing query)
- Internal errors (transient issues like disk full, network errors)
- Storage errors (may recover after flush, compaction, etc.)

**Exponential Backoff Strategy**:
1. Wait 1s, retry
2. Wait 2s, retry
3. Wait 4s, retry
4. Give up after 3 attempts

**Example Implementation**:
```rust
async fn execute_with_retry(sql: &str, params: Vec<ScalarValue>) -> Result<ExecutionResult> {
    let mut attempt = 0;
    let max_attempts = 3;
    
    loop {
        match client.execute_sql(sql, params.clone()).await {
            Ok(result) => return Ok(result),
            Err(err) if err.is_retryable() && attempt < max_attempts => {
                let delay = Duration::from_secs(2u64.pow(attempt));
                tokio::time::sleep(delay).await;
                attempt += 1;
            },
            Err(err) => return Err(err),
        }
    }
}
```

---

## Error Response Schema

All error responses follow this structure:

```json
{
  "error": {
    "code": "ERROR_CODE",              // UPPERCASE_SNAKE_CASE
    "message": "string",                // Human-readable description
    "details": {                        // Context-specific fields
      // Error-specific fields
    },
    "request_id": "uuid",               // Matches request UUID
    "timestamp": "ISO8601 string"       // Error occurrence time
  }
}
```

**Field Constraints**:
- `code`: Non-empty, matches one of the defined codes
- `message`: Non-empty, descriptive, no sensitive data
- `details`: Optional object with error-specific context
- `request_id`: Valid UUID v4 matching ExecutionContext.request_id
- `timestamp`: ISO 8601 format (e.g., `2025-11-07T12:34:56Z`)
