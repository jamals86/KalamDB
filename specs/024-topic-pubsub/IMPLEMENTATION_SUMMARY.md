# SQL Functions Implementation Summary

## Overview
This implementation adds comprehensive testing and documentation for all SQL functions in KalamDB, including context functions, ID generation functions, and their usage in various SQL statements.

## Files Created/Modified

### 1. Core Function Implementations
- **`backend/crates/kalamdb-core/src/sql/functions/current_user.rs`** (MODIFIED)
  - Updated `CURRENT_USER()` to return username instead of user_id
  - Returns `Option<UserName>` and requires username to be set in session

- **`backend/crates/kalamdb-core/src/sql/functions/current_user_id.rs`** (NEW)
  - New function `CURRENT_USER_ID()` returns the user ID
  - **Security**: Restricted to system, dba, and service roles only
  - Regular users cannot call this function

- **`backend/crates/kalamdb-core/src/sql/functions/current_role.rs`** (NEW)
  - New function `CURRENT_ROLE()` returns the user's role
  - Returns lowercase string: "user", "dba", "system", "service", "anonymous"
  - Accessible to all roles

### 2. Session/Context Updates
- **`backend/crates/kalamdb-session/src/user_context.rs`** (MODIFIED)
  - Added optional `username: Option<UserName>` field to `UserContext`
  - Added `client_with_username()` and `with_username()` helper methods

- **`backend/crates/kalamdb-session/src/auth_session.rs`** (MODIFIED)
  - Added `with_username_and_auth_details()` method to `AuthSession`
  - Allows setting username when creating authenticated sessions

### 3. ExecutionContext Updates
- **`backend/crates/kalamdb-core/src/sql/context/execution_context.rs`** (MODIFIED)
  - Updated function registration in `build_user_session_context()`
  - Registers all three context functions with actual user data
  - `CURRENT_USER()` gets username from session
  - `CURRENT_USER_ID()` gets user_id and role for authorization
  - `CURRENT_ROLE()` gets role from session

### 4. Function Registration
- **`backend/crates/kalamdb-core/src/sql/functions/mod.rs`** (MODIFIED)
  - Exports all three new context functions
  - Maintains all ID generation functions

- **`backend/crates/kalamdb-core/src/sql/mod.rs`** (MODIFIED)
  - Re-exports new functions for public API

- **`backend/crates/kalamdb-core/src/sql/datafusion_session.rs`** (MODIFIED)
  - Registers all functions in base session
  - Ensures functions are available in all queries

### 5. Test Files

#### `backend/crates/kalamdb-core/tests/test_context_functions.rs` (NEW)
Focused tests on context functions:
- `test_current_user_with_username()` - Basic username retrieval
- `test_current_user_without_username_fails()` - Error handling
- `test_current_user_id_with_dba_role()` - Authorization check
- `test_current_user_id_with_system_role()` - System role access
- `test_current_user_id_with_service_role()` - Service role access
- `test_current_user_id_with_user_role_fails()` - Unauthorized access
- `test_current_role_user()` - User role
- `test_current_role_dba()` - DBA role
- `test_current_role_system()` - System role
- `test_all_three_functions_together()` - Combined usage
- `test_functions_in_where_clause()` - WHERE clause usage
- `test_functions_with_no_arguments()` - Function validation

#### `backend/crates/kalamdb-core/tests/test_all_sql_functions.rs` (NEW)
Comprehensive integration tests covering:

**Context Functions**:
- `test_current_user_basic()` - Basic usage
- `test_current_user_id_dba()` - DBA access
- `test_current_user_id_system()` - System access
- `test_current_user_id_service_role()` - Service access
- `test_current_user_id_unauthorized_user_role()` - Unauthorized check
- `test_current_role_user()`, `test_current_role_dba()`, `test_current_role_system()`
- `test_all_context_functions_together()` - All three together

**ID Generation Functions**:
- `test_snowflake_id_function()` - Snowflake ID generation
- `test_uuid_v7_function()` - UUID V7 generation
- `test_ulid_function()` - ULID generation
- `test_multiple_id_functions_in_select()` - All three together

**WHERE Clause Usage**:
- `test_current_user_in_where_clause()` - CURRENT_USER() in WHERE
- `test_current_user_where_no_match()` - Condition checking
- `test_current_role_in_where_clause()` - CURRENT_ROLE() in WHERE
- `test_multiple_functions_in_where()` - Multiple conditions

**SELECT Expressions**:
- `test_multiple_id_functions_in_select()` - Multiple ID functions
- `test_context_and_id_functions_together()` - Mixed functions

**Expressions & Operators**:
- `test_context_function_with_coalesce()` - COALESCE usage
- `test_context_function_with_concat()` - String concatenation

**Multiple Rows**:
- `test_snowflake_id_generates_multiple_unique_ids()` - Uniqueness

**Conditional Statements**:
- `test_context_function_in_case_statement()` - CASE with context
- `test_id_function_in_case_statement()` - CASE with IDs

**Edge Cases**:
- `test_current_user_empty_check()` - NULL checking
- `test_uuid_v7_format_validation()` - UUID format
- `test_ulid_format_validation()` - ULID format

**Subqueries**:
- `test_context_function_in_subquery()` - Context in subquery
- `test_id_function_in_subquery()` - ID functions in subquery

**Documentation Examples**:
- `test_example_all_context_functions()` - Example usage
- `test_example_all_id_functions()` - ID function examples
- `test_example_mixed_functions()` - Mixed function usage

### 6. Documentation

#### `SQL_FUNCTIONS_USAGE_GUIDE.md` (NEW)
Comprehensive guide covering:

**INSERT Statements**:
- Using context functions in VALUES clause
- Using ID generation functions
- Combining context and ID functions
- Full examples with audit records

**UPDATE Statements**:
- Context functions in SET clause
- ID generation functions in updates
- Complex UPDATE scenarios
- Activity tracking updates

**DELETE Statements**:
- Context functions in WHERE clause
- ID lookups in WHERE
- Complex DELETE with multiple conditions
- Cascade delete patterns

**Best Practices**:
- Security with context functions
- Distributed system patterns
- Audit trail implementation
- Soft delete patterns
- Multi-tenant isolation

**Performance Considerations**:
- Cost analysis of all functions
- Bulk operation safety

**Error Handling**:
- CURRENT_USER() errors
- CURRENT_USER_ID() authorization errors

## Functions Available

### Context Functions (3)
| Function | Returns | Scope | Notes |
|----------|---------|-------|-------|
| `CURRENT_USER()` | String (Username) | All roles | Returns authenticated username |
| `CURRENT_USER_ID()` | String (User ID) | system, dba, service only | Returns user ID (restricted) |
| `CURRENT_ROLE()` | String (Role) | All roles | Returns user's role |

### ID Generation Functions (3)
| Function | Returns | Uniqueness | Ordering | Use Case |
|----------|---------|-----------|----------|----------|
| `SNOWFLAKE_ID()` | BIGINT | Global | Time-ordered | Distributed systems |
| `UUID_V7()` | String | Global | Time-ordered | Standard UUID track |
| `ULID()` | String (26 chars) | Global | Time-ordered | Human-readable IDs |

## Testing Summary

### Test Files
- **test_context_functions.rs**: 12 tests focused on context functions
- **test_all_sql_functions.rs**: 38 comprehensive integration tests

### Total Test Coverage
- **50+ test cases** covering:
  - All 6 functions
  - SELECT statements
  - WHERE clauses
  - CASE statements
  - Subqueries
  - Expression combinations
  - Edge cases and errors
  - Format validation
  - Authorization checks

## Security

### CURRENT_USER()
- ✅ Requires username to be set in session
- ✅ Accessible to all authenticated users
- ✅ Fails gracefully if username is not set

### CURRENT_USER_ID()
- ✅ Restricted to system/dba/service roles
- ✅ Regular users receive authorization error
- ✅ Requires both user_id and role context

### CURRENT_ROLE()
- ✅ Accessible to all users
- ✅ Always returns lowercase role name
- ✅ Matches role enum values

## Usage Examples

### Complete Example
```sql
-- Insert audit record with all functions
INSERT INTO audit_log (
  event_id,        -- ULID for readability
  user_id,         -- CURRENT_USER_ID (admin only)
  username,        -- CURRENT_USER() for all users
  role,            -- CURRENT_ROLE() for context
  reference_uuid   -- UUID_V7() for tracing
) VALUES (
  ULID(),
  CURRENT_USER_ID(),
  CURRENT_USER(),
  CURRENT_ROLE(),
  UUID_V7()
);

-- Update with context
UPDATE user_sessions
SET last_user = CURRENT_USER(),
    last_role = CURRENT_ROLE(),
    last_session_id = SNOWFLAKE_ID()
WHERE username = CURRENT_USER();

-- Delete user's own data
DELETE FROM temp_data
WHERE owner = CURRENT_USER()
  AND created_at < DATE_SUB(NOW(), INTERVAL 24 HOUR);
```

## Migration Notes

### For Existing Code
1. If using `CURRENT_USER()` expecting user_id, update to `CURRENT_USER_ID()`
2. Ensure admin roles are set for code using `CURRENT_USER_ID()`
3. Context functions now require username to be set in `UserContext`

### API Breaking Changes
- `CurrentUserFunction` now uses `with_username()` instead of `with_user_id()`
- `UserContext` now has optional `username` field
- `AuthSession` has new `with_username_and_auth_details()` method

## Verification

To run the tests:
```bash
# Test context functions
cargo test --test test_context_functions --package kalamdb-core

# Test all SQL functions
cargo test --test test_all_sql_functions --package kalamdb-core

# Run all core tests
cargo test --package kalamdb-core
```

To check compilation:
```bash
cargo check --package kalamdb-core
cargo check --package kalamdb-session
```
