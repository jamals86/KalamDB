# SQL Functions Quick Reference

## Three New Context Functions

### CURRENT_USER()
Returns the authenticated username.

```sql
-- Select current username
SELECT CURRENT_USER();

-- Insert with username context
INSERT INTO logs (username) VALUES (CURRENT_USER());

-- Use in WHERE clause
DELETE FROM temp_data WHERE owner = CURRENT_USER();
```

**Authorization**: Available to all authenticated users
**Error**: Fails if username not set in session context
**Example Return**: `"john.doe"`

---

### CURRENT_USER_ID()
Returns the authenticated user's ID (restricted function).

```sql
-- Get user ID (admin only)
SELECT CURRENT_USER_ID();

-- Record who made admin action
INSERT INTO admin_log (actor_id, action) 
VALUES (CURRENT_USER_ID(), 'config_update');
```

**Authorization**: ✅ system, ✅ dba, ✅ service roles only
**Unauthorized**: ❌ user, ❌ anonymous → Returns error
**Example Return**: `"usr_12345"`

---

### CURRENT_ROLE()
Returns the current user's role as a lowercase string.

```sql
-- Check user's role
SELECT CURRENT_ROLE();

-- Use in WHERE for role-based filtering
SELECT * FROM sensitive_data 
WHERE role_allows_access = CURRENT_ROLE();

-- CASE-based access control
SELECT 
  CASE CURRENT_ROLE()
    WHEN 'system' THEN full_data
    WHEN 'dba' THEN masked_data
    ELSE null
  END
FROM data;
```

**Possible Values**: 
- `'user'` - Regular user
- `'service'` - Service account
- `'dba'` - Database administrator
- `'system'` - System administrator
- `'anonymous'` - Unauthenticated

**Authorization**: Available to all roles
**Example Return**: `"dba"`

---

## Three ID Generation Functions

### SNOWFLAKE_ID()
Distributed ID (64-bit, time-ordered, globally unique).

```sql
-- Use as primary key
CREATE TABLE events (
  id BIGINT PRIMARY KEY,  -- SNOWFLAKE_ID()
  data STRING
);

INSERT INTO events VALUES (SNOWFLAKE_ID(), 'data1');

-- Properties:
-- - Guaranteed unique across all servers
-- - Time-ordered (can sort by ID timestamp)
-- - 64-bit integer (fits in BIGINT)
```

**Return Type**: BIGINT
**Uniqueness**: Global across distributed systems
**Ordering**: Time-ordered (millisecond precision)
**Best For**: Request IDs, high-volume auto-increment
**Example Return**: `1234567890123456789`

---

### UUID_V7()
Time-ordered UUID (36-char string with hyphens).

```sql
-- Use for tracing
INSERT INTO events (trace_id) VALUES (UUID_V7());

-- Format: 123e4567-e89b-12d3-a456-426614174000
-- Properties:
-- - 36 characters (with hyphens)
-- - Time-ordered variant
-- - Globally unique
```

**Return Type**: STRING (36 characters)
**Format**: `12345678-1234-1234-1234-123456789012`
**Hyphens**: 4 (at positions 8, 13, 18, 23)
**Uniqueness**: Global
**Ordering**: Time-ordered
**Best For**: Request correlation, distributed tracing
**Example Return**: `"550e8400-e29b-41d4-a716-446655440000"`

---

### ULID()
User-friendly ID (26-char alphanumeric, no hyphens).

```sql
-- Use for sharing/display
INSERT INTO documents (public_id) VALUES (ULID());

-- Format: 01ARZ3NDEKTSV4RRFFQ6902K5A
-- Properties:
-- - 26 characters
-- - No special characters (URL-safe)
-- - Time-ordered
-- - Human-readable
```

**Return Type**: STRING (26 characters)
**Format**: `01ARZ3NDEKTSV4RRFFQ69_____` (uppercase alphanumeric)
**Hyphens**: None (URL-safe)
**Uniqueness**: Global
**Ordering**: Time-ordered (first 6 chars = timestamp, useful for partitioning)
**Best For**: User-facing IDs, sharelinks, partitioning keys
**Example Return**: `"01ARZ3NDEKTSV4RRFFQ69QTZF"`

---

## All 6 Functions - Feature Matrix

| Function | Returns | Scope | Context | Distributed | Ordering |
|----------|---------|-------|---------|-------------|----------|
| `CURRENT_USER()` | String | All users | Username | N/A | N/A |
| `CURRENT_USER_ID()` | String | Admin only | User ID | N/A | N/A |
| `CURRENT_ROLE()` | String | All users | Role | N/A | N/A |
| `SNOWFLAKE_ID()` | BIGINT | All users | None | ✓ Global | ✓ Time |
| `UUID_V7()` | String(36) | All users | None | ✓ Global | ✓ Time |
| `ULID()` | String(26) | All users | None | ✓ Global | ✓ Time |

---

## Usage Patterns

### Pattern 1: Audit Trail
```sql
INSERT INTO audit (user, role, action, timestamp)
VALUES (CURRENT_USER(), CURRENT_ROLE(), 'DELETE', SNOWFLAKE_ID());
```

### Pattern 2: Tracking
```sql
INSERT INTO requests (trace_id, session_id, user)
VALUES (UUID_V7(), ULID(), CURRENT_USER());
```

### Pattern 3: Multi-Tenant
```sql
INSERT INTO data (id, tenant_id, owner, partition_key)
VALUES (SNOWFLAKE_ID(), CURRENT_USER(), CURRENT_USER(), ULID());
```

### Pattern 4: Access Control
```sql
DELETE FROM records 
WHERE owner = CURRENT_USER()
  AND visible_to_role = CURRENT_ROLE();
```

### Pattern 5: Role-Restricted Admin
```sql
INSERT INTO admin_actions (actor_id, action, timestamp)
VALUES (CURRENT_USER_ID(), 'CONFIG_UPDATE', SNOWFLAKE_ID());
-- Only works for dba/system/service roles
```

---

## WHERE I Can Use Each Function

| Context | CURRENT_USER | CURRENT_USER_ID | CURRENT_ROLE | SNOWFLAKE_ID | UUID_V7 | ULID |
|---------|---|---|---|---|---|---|
| SELECT list | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| WHERE clause | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| INSERT VALUES | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| UPDATE SET | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| DELETE WHERE | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| CASE WHEN | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| Subqueries | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| JOIN ON | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| GROUP BY | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |
| ORDER BY | ✓ | ✓ restricted | ✓ | ✓ | ✓ | ✓ |

---

## Implementation Details

### Files Modified
- `src/sql/functions/current_user.rs` - Now returns username
- `src/sql/functions/mod.rs` - Added new functions
- `src/sql/context/execution_context.rs` - Function registration

### Files Created
- `src/sql/functions/current_user_id.rs` - New restricted function
- `src/sql/functions/current_role.rs` - New role function
- `tests/test_context_functions.rs` - 12 focused tests
- `tests/test_all_sql_functions.rs` - 38 comprehensive tests

### Backend Location
```
backend/crates/
  kalamdb-session/
    src/user_context.rs                    # UserContext with username
    src/auth_session.rs                    # AuthSession helpers
  kalamdb-core/
    src/sql/functions/
      current_user.rs                      # Returns username
      current_user_id.rs                   # NEW: Returns user ID
      current_role.rs                      # NEW: Returns role
      mod.rs                               # Function registrations
    src/sql/context/
      execution_context.rs                 # Function binding
    tests/
      test_context_functions.rs            # 12 focused tests
      test_all_sql_functions.rs            # 38 comprehensive tests
```

---

## Testing

### Run Tests
```bash
# Test context functions (12 tests)
cargo test --test test_context_functions --package kalamdb-core

# Test all SQL functions (38 tests)
cargo test --test test_all_sql_functions --package kalamdb-core

# Run all kalamdb-core tests
cargo test --package kalamdb-core
```

### Test Coverage
- ✅ 50+ total test cases
- ✅ All 6 functions covered
- ✅ All SQL contexts (SELECT, WHERE, CASE, subqueries, etc.)
- ✅ Authorization checks (role-based)
- ✅ Format validation (UUID hyphens, ULID length)
- ✅ Edge cases (NULL, empty, multiple rows)

---

## Common Errors & Solutions

### Error: "CURRENT_USER() requires username to be set"
```sql
-- This fails: username not in session
SELECT CURRENT_USER();

-- Solution: Ensure username is set in AuthSession
auth_session.with_username_and_auth_details(username, user_id, role)?
```

### Error: "Unauthorized: CURRENT_USER_ID() requires system, dba, or service role"
```sql
-- This fails: regular user trying to access
SELECT CURRENT_USER_ID();  -- User with 'user' role

-- Solution: Only call from admin context or wrap in CASE
SELECT CASE 
  WHEN CURRENT_ROLE() IN ('system', 'dba', 'service') 
  THEN CURRENT_USER_ID()
  ELSE 'N/A'
END;
```

### Error: "Function not found"
```sql
-- If function not available, ensure:
-- 1. kalamdb-core module is included
-- 2. ExecutionContext is properly initialized
-- 3. Rust code doesn't override defaults

-- Verify compilation:
cargo check --package kalamdb-core
```

---

## Security Notes

### CURRENT_USER()
- ✓ Safe to expose to all users
- ✓ Shows only the authenticated username
- ✓ Cannot be spoofed (comes from session)

### CURRENT_USER_ID()
- ⚠️ Restricted to admin roles only
- ⚠️ Regular users who call it get an error
- ⚠️ Use for admin-only audit logging

### CURRENT_ROLE()
- ✓ Safe to expose to all users
- ✓ Always lowercase
- ✓ Useful for role-based UI/logic

### ID Functions
- ✓ All globally unique
- ✓ Impossible to predict next ID
- ✓ Safe from collision (distributed)
- ⚠️ SNOWFLAKE_ID() is i64, so will wrap around in ~292 years

---

## Migration Guide

If you were using `CURRENT_USER()` to get user_id:

**Old Code:**
```sql
INSERT INTO events (user_id) VALUES (CURRENT_USER());
```

**New Code - Option 1 (Recommended):**
```sql
INSERT INTO events (user_id) VALUES (CURRENT_USER_ID());
-- Note: Only works for dba/system/service roles
```

**New Code - Option 2:**
```sql
INSERT INTO events (username) VALUES (CURRENT_USER());
-- Now gets username instead of user_id
```

---

## Related Documentation

- **IMPLEMENTATION_SUMMARY.md** - Full implementation details
- **PRACTICAL_SQL_EXAMPLES.md** - Real-world usage examples
- **SQL_FUNCTIONS_USAGE_GUIDE.md** - INSERT/UPDATE/DELETE patterns

---

## Quick Reference Commands

```bash
# Verify implementation
cd backend && cargo check --package kalamdb-core

# Run all function tests
cd backend && cargo test --package kalamdb-core --package kalamdb-session

# Build and check for errors
cd backend && cargo build 2>&1 | grep -E "error|warning"

# See test output
cargo test --test test_all_sql_functions -- --nocapture
```
