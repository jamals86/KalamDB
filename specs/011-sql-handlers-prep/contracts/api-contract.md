# API Contracts: SQL Handlers Prep

**Feature**: 011-sql-handlers-prep  
**Date**: 2025-11-07  
**Phase**: 1 (Design & Contracts)

## REST API Endpoint

### POST /v1/api/sql

Execute SQL statement with optional parameters.

**Authentication**: Required (JWT Bearer token or HTTP Basic Auth)

**Request**:
```json
{
  "sql": "string",           // Required: SQL statement to execute
  "params": [               // Optional: Query parameters (max 50, 512KB each)
    123,                    // ScalarValue::Int64
    "Alice",                // ScalarValue::Utf8
    3.14,                   // ScalarValue::Float64
    true,                   // ScalarValue::Boolean
    null                    // ScalarValue::Null
  ],
  "namespace": "string"     // Optional: Default namespace for unqualified tables
}
```

**Request Validation**:
- `sql`: Non-empty string, max 1MB
- `params`: Array length ≤ 50
- Each `params[i]`: Serialized size ≤ 512KB
- `namespace`: Valid NamespaceId if provided

**Response - Success (200 OK)**:
```json
{
  "status": "success",
  "operation": "select|insert|update|delete|ddl|flush|subscription|job",
  "data": {
    // Variant-specific fields based on ExecutionResult
  },
  "row_count": 0,           // For SELECT/SHOW/DESCRIBE operations
  "rows_affected": 0,       // For DML/DDL/Job operations
  "request_id": "uuid",
  "elapsed_ms": 0
}
```

**Row Count Semantics** (MySQL-compatible):

| Operation | Field | Meaning |
|-----------|-------|---------|
| SELECT | `row_count` | Rows returned in result set |
| INSERT | `rows_affected` | Rows inserted (multi-value supported) |
| UPDATE | `rows_affected` | Rows with actual changes (not matched) |
| DELETE | `rows_affected` | Rows removed |
| CREATE/DROP NAMESPACE | `rows_affected` | Always 1 |
| CREATE/ALTER/DROP TABLE | `rows_affected` | Always 1 |
| CREATE/ALTER/DROP STORAGE | `rows_affected` | Always 1 |
| CREATE/ALTER/DROP USER | `rows_affected` | Always 1 |
| STORAGE FLUSH TABLE | `rows_affected` | Always 1 |
| STORAGE FLUSH ALL | `rows_affected` | Number of tables flushed |
| KILL JOB | `rows_affected` | Always 1 |
| KILL LIVE QUERY | `rows_affected` | Always 1 |
| SHOW NAMESPACES/TABLES/STORAGES | `row_count` | Rows returned |
| DESCRIBE TABLE | `row_count` | Number of columns |

**Special Cases**:
- CREATE IF NOT EXISTS: `rows_affected = 0` if exists, `1` if created
- UPDATE with no changes: `rows_affected = 0` if values identical

**Response Variants**:

1. **Query Results** (SELECT, SHOW, DESCRIBE):
```json
{
  "status": "success",
  "operation": "select",
  "data": {
    "columns": ["id", "name", "age"],
    "rows": [
      [1, "Alice", 30],
      [2, "Bob", 25]
    ]
  },
  "row_count": 2,
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "elapsed_ms": 45
}
```

2. **DML Results** (INSERT, UPDATE, DELETE):
```json
{
  "status": "success",
  "operation": "insert",
  "rows_affected": 5,
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "elapsed_ms": 12
}
```

3. **DDL Results** (CREATE, ALTER, DROP):
```json
{
  "status": "success",
  "operation": "ddl",
  "message": "Namespace 'prod' created successfully",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "elapsed_ms": 8
}
```

4. **Flush Results** (STORAGE FLUSH TABLE, STORAGE FLUSH ALL):
```json
{
  "status": "success",
  "operation": "flush",
  "tables_flushed": ["users", "orders"],
  "bytes_written": 1048576,
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "elapsed_ms": 200
}
```

5. **Subscription Results** (LIVE SELECT):
```json
{
  "status": "success",
  "operation": "subscription",
  "subscription_id": "sub_abc123",
  "channel": "wss://api.kalamdb.com/v1/subscribe/sub_abc123",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "elapsed_ms": 5
}
```

**Response - Error (4xx/5xx)**:
```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error description",
    "details": {
      // Context-specific error details
    },
    "request_id": "uuid",
    "timestamp": "2025-11-07T12:34:56Z"
  }
}
```

**Error Codes** (see error-codes.md for full list):

| HTTP Status | Error Code | Description |
|-------------|------------|-------------|
| 400 | PARAM_COUNT_MISMATCH | Parameter count doesn't match placeholders |
| 400 | PARAM_COUNT_EXCEEDED | More than 50 parameters provided |
| 400 | PARAM_SIZE_EXCEEDED | Parameter exceeds 512KB limit |
| 400 | PARAMS_NOT_SUPPORTED | Statement type doesn't support parameters |
| 400 | PARAM_TYPE_MISMATCH | Parameter type incompatible with schema |
| 400 | INVALID_SQL_SYNTAX | SQL parsing failed |
| 403 | AUTH_INSUFFICIENT_ROLE | User lacks required role for operation |
| 403 | AUTH_NAMESPACE_ACCESS_DENIED | User cannot access namespace |
| 404 | NOT_FOUND_TABLE | Table doesn't exist |
| 404 | NOT_FOUND_NAMESPACE | Namespace doesn't exist |
| 404 | NOT_FOUND_STORAGE | Storage doesn't exist |
| 408 | TIMEOUT_HANDLER_EXECUTION | Handler exceeded timeout |
| 501 | NOT_IMPLEMENTED_TRANSACTION | Transaction support not yet implemented |
| 500 | INTERNAL_DATAFUSION_ERROR | DataFusion execution error |
| 500 | INTERNAL_STORAGE_ERROR | Storage layer error |

---

## Parameter Binding Examples

### Example 1: Simple SELECT with parameters
```json
POST /v1/api/sql
{
  "sql": "SELECT * FROM users WHERE id = $1 AND age > $2",
  "params": [123, 25]
}
```

**DataFusion Processing**:
1. Parse SQL → LogicalPlan with `Expr::Placeholder` for `$1`, `$2`
2. Replace placeholders: `$1` → `Literal(Int64(123))`, `$2` → `Literal(Int64(25))`
3. Validate types against `users` table schema
4. Execute modified plan

### Example 2: INSERT with parameters
```json
POST /v1/api/sql
{
  "sql": "INSERT INTO users (name, email, age) VALUES ($1, $2, $3)",
  "params": ["Alice", "alice@example.com", 30]
}
```

### Example 3: UPDATE with parameters
```json
POST /v1/api/sql
{
  "sql": "UPDATE users SET email = $1 WHERE id = $2",
  "params": ["newemail@example.com", 123]
}
```

### Example 4: DELETE with parameters
```json
POST /v1/api/sql
{
  "sql": "DELETE FROM users WHERE created_at < $1",
  "params": ["2024-01-01T00:00:00Z"]
}
```

### Example 5: Multiple parameter types
```json
POST /v1/api/sql
{
  "sql": "SELECT * FROM orders WHERE user_id = $1 AND total > $2 AND status = $3 AND shipped = $4",
  "params": [
    123,           // Int64
    99.99,         // Float64
    "pending",     // Utf8 (String)
    false          // Boolean
  ]
}
```

---

## Authorization Matrix

| Operation | User | Service | DBA | System |
|-----------|------|---------|-----|--------|
| SELECT | ✓ (own namespace) | ✓ (configured namespaces) | ✓ (all) | ✓ (all) |
| INSERT/UPDATE/DELETE | ✓ (own namespace) | ✓ (configured namespaces) | ✓ (all) | ✓ (all) |
| CREATE/DROP NAMESPACE | ✗ | ✗ | ✓ | ✓ |
| CREATE/ALTER/DROP STORAGE | ✗ | ✗ | ✓ | ✓ |
| CREATE/ALTER/DROP TABLE | ✓ (own namespace) | ✓ (configured namespaces) | ✓ (all) | ✓ (all) |
| SHOW TABLES/STATS | ✓ (own namespace) | ✓ (configured namespaces) | ✓ (all) | ✓ (all) |
| STORAGE FLUSH TABLE/ALL | ✗ | ✗ | ✓ | ✓ |
| KILL JOB/LIVE QUERY | ✗ (own jobs only) | ✗ (own jobs only) | ✓ (all) | ✓ (all) |
| LIVE SELECT | ✓ (own namespace) | ✓ (configured namespaces) | ✓ (all) | ✓ (all) |
| CREATE/ALTER USER | ✗ (self password only) | ✗ | ✓ | ✓ |
| DROP USER | ✗ | ✗ | ✓ | ✓ |
| BEGIN/COMMIT/ROLLBACK | ✗ (not implemented) | ✗ (not implemented) | ✗ (not implemented) | ✗ (not implemented) |

**Authorization Checks**:
- Performed by `TypedStatementHandler::check_authorization()` before execution
- ExecutionContext.role determines permissions
- Namespace access verified against user's allowed namespaces
- Self-service operations: ALTER USER (password change only), KILL JOB (own jobs)

---

## Error Response Examples

### Example 1: Parameter count mismatch
```json
POST /v1/api/sql
{
  "sql": "SELECT * FROM users WHERE id = $1 AND name = $2",
  "params": [123]  // Missing $2
}

Response: 400 Bad Request
{
  "error": {
    "code": "PARAM_COUNT_MISMATCH",
    "message": "Parameter count mismatch: expected 2 parameters, got 1",
    "details": {
      "expected": 2,
      "actual": 1,
      "placeholders": ["$1", "$2"]
    },
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": "2025-11-07T12:34:56Z"
  }
}
```

### Example 2: Parameter size exceeded
```json
POST /v1/api/sql
{
  "sql": "INSERT INTO logs (message) VALUES ($1)",
  "params": ["<600KB of text>"]
}

Response: 400 Bad Request
{
  "error": {
    "code": "PARAM_SIZE_EXCEEDED",
    "message": "Parameter size exceeded: parameter at index 0 is 614400 bytes (max 524288 bytes)",
    "details": {
      "index": 0,
      "size_bytes": 614400,
      "max_bytes": 524288
    },
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": "2025-11-07T12:34:56Z"
  }
}
```

### Example 3: Parameters not supported
```json
POST /v1/api/sql
{
  "sql": "CREATE NAMESPACE prod",
  "params": [123]  // DDL doesn't support params
}

Response: 400 Bad Request
{
  "error": {
    "code": "PARAMS_NOT_SUPPORTED",
    "message": "Parameters are not supported for CREATE NAMESPACE statements",
    "details": {
      "statement_type": "CreateNamespace",
      "supported_types": ["SELECT", "INSERT", "UPDATE", "DELETE"]
    },
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": "2025-11-07T12:34:56Z"
  }
}
```

### Example 4: Authorization failure
```json
POST /v1/api/sql
Authorization: Bearer <user-token>
{
  "sql": "CREATE NAMESPACE prod"
}

Response: 403 Forbidden
{
  "error": {
    "code": "AUTH_INSUFFICIENT_ROLE",
    "message": "Insufficient role: CREATE NAMESPACE requires DBA or System role, but user has User role",
    "details": {
      "required_roles": ["DBA", "System"],
      "actual_role": "User",
      "operation": "CREATE NAMESPACE"
    },
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": "2025-11-07T12:34:56Z"
  }
}
```

### Example 5: Handler timeout
```json
POST /v1/api/sql
{
  "sql": "SELECT * FROM huge_table WHERE complex_condition"  // Takes >30s
}

Response: 408 Request Timeout
{
  "error": {
    "code": "TIMEOUT_HANDLER_EXECUTION",
    "message": "Handler execution exceeded 30s timeout",
    "details": {
      "timeout_seconds": 30,
      "elapsed_ms": 30042,
      "statement_type": "SELECT"
    },
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": "2025-11-07T12:34:56Z"
  }
}
```

### Example 6: Transaction not implemented
```json
POST /v1/api/sql
{
  "sql": "BEGIN TRANSACTION"
}

Response: 501 Not Implemented
{
  "error": {
    "code": "NOT_IMPLEMENTED_TRANSACTION",
    "message": "Transaction support planned for Phase 11",
    "details": {
      "feature": "BEGIN TRANSACTION",
      "planned_phase": "Phase 11"
    },
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": "2025-11-07T12:34:56Z"
  }
}
```

---

## Configuration

### config.toml

```toml
[execution]
# Handler execution timeout (seconds)
handler_timeout_seconds = 30

# Parameter validation limits
max_parameters = 50
max_parameter_size_bytes = 524288  # 512KB

[api]
# Request body size limit (bytes)
max_request_body_size = 1048576  # 1MB
```

---

## Contract Testing

### Test Cases

**TC-001**: Valid SELECT with parameters returns 200 + data
**TC-002**: Valid INSERT with parameters returns 200 + rows_affected
**TC-003**: Parameter count mismatch returns 400 + PARAM_COUNT_MISMATCH
**TC-004**: Parameter size exceeded returns 400 + PARAM_SIZE_EXCEEDED
**TC-005**: Parameters on DDL returns 400 + PARAMS_NOT_SUPPORTED
**TC-006**: Non-admin CREATE NAMESPACE returns 403 + AUTH_INSUFFICIENT_ROLE
**TC-007**: Handler timeout returns 408 + TIMEOUT_HANDLER_EXECUTION
**TC-008**: BEGIN TRANSACTION returns 501 + NOT_IMPLEMENTED_TRANSACTION
**TC-009**: Invalid SQL syntax returns 400 + INVALID_SQL_SYNTAX
**TC-010**: Missing auth header returns 401 + AUTH_REQUIRED

### Contract Validation

All error responses MUST include:
- ✅ `error.code` in UPPERCASE_SNAKE_CASE format
- ✅ `error.message` non-empty string
- ✅ `error.details` object with context-specific fields
- ✅ `error.request_id` matching request UUID
- ✅ `error.timestamp` in ISO 8601 format

All success responses MUST include:
- ✅ `status: "success"`
- ✅ `request_id` matching request UUID
- ✅ `elapsed_ms` integer ≥ 0
- ✅ Variant-specific fields (data/rows_affected/message/etc.)

---

## Summary

**Endpoint**: POST /v1/api/sql (authenticated)

**Request**: SQL string + optional params (max 50, 512KB each) + optional namespace

**Response**: JSON with status, operation-specific data, request_id, elapsed_ms

**Error Handling**: Structured errors with codes, messages, details, request_id, timestamp

**Authorization**: Role-based (User/Service/DBA/System) with operation matrix

**Parameter Binding**: PostgreSQL-style $1, $2 placeholders, DataFusion validation

**Timeout**: 30s default, configurable, returns TIMEOUT_HANDLER_EXECUTION error

**Contract Tests**: 10 test cases covering success paths, validation errors, auth errors, timeouts
