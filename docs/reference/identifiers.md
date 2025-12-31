# Identifier Case Sensitivity

KalamDB uses **case-insensitive identifiers** for table names, namespace names, and column names.

## Behavior

All identifiers are normalized to **lowercase** on creation:

```sql
-- These all refer to the same table:
CREATE TABLE Users (...)
SELECT * FROM users
SELECT * FROM USERS
```

```sql
-- These all refer to the same column:
SELECT FirstName FROM users
SELECT firstname FROM users
SELECT FIRSTNAME FROM users
```

## Comparison with Other Databases

| Database   | Unquoted Identifiers | Quoted Identifiers |
|------------|---------------------|-------------------|
| KalamDB    | Lowercase (case-insensitive) | N/A |
| PostgreSQL | Lowercase (case-insensitive) | Preserved (case-sensitive) |
| MySQL      | OS-dependent | OS-dependent |
| SQL Standard | Uppercase | Preserved |

## Rationale

- **Predictable**: No OS-dependent behavior
- **Simple**: Users don't need to remember exact casing
- **Compatible**: Matches DataFusion's default behavior
- **Portable**: Queries work consistently across platforms
