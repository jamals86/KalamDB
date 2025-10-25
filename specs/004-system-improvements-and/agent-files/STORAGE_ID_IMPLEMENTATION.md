# Storage ID Implementation for Table Creation

## Summary
Implemented support for specifying `storage_id` when creating both user and shared tables, with automatic defaulting to 'local' storage if not specified.

## Changes Made

### 1. CREATE SHARED TABLE Parser (`kalamdb-sql/src/ddl/create_shared_table.rs`)
- **Added** `storage_id: Option<String>` field to `CreateSharedTableStatement`
- **Deprecated** `location: Option<String>` field (kept for backward compatibility)
- **Added** `parse_storage_clause()` method to parse `STORAGE <storage_id>` from SQL
- **Added** test cases for storage clause parsing

### 2. SQL Executor (`kalamdb-core/src/sql/executor.rs`)
- **Updated** shared table creation logic (2 code paths):
  1. When `TABLE_TYPE SHARED` is explicitly specified
  2. Default case when no `TABLE_TYPE` is specified
- **Added** validation to check if storage_id exists in `system.storages`
- **Added** automatic default to 'local' if storage_id is not specified
- **Changed** from hardcoded `storage_id: Some("local")` to using parsed value

### 3. Test Fixtures (`kalamdb-core/src/services/shared_table_service.rs`)
- **Updated** 4 test cases to include `storage_id: None` in `CreateSharedTableStatement` construction

## SQL Syntax

### CREATE USER TABLE (Already Supported)
```sql
CREATE USER TABLE messages (
    id BIGINT AUTO_INCREMENT,
    content TEXT
) STORAGE s3_prod;
```

### CREATE SHARED TABLE (New Support)
```sql
CREATE TABLE config (
    key TEXT,
    value TEXT
) STORAGE s3_prod;

-- Or with explicit TABLE_TYPE
CREATE TABLE config (
    key TEXT,
    value TEXT
) TABLE_TYPE SHARED STORAGE s3_prod;
```

### Default Behavior
If `STORAGE` clause is omitted, both table types default to 'local' storage:
```sql
-- Defaults to STORAGE local
CREATE TABLE config (key TEXT, value TEXT);
CREATE USER TABLE messages (id BIGINT, content TEXT);
```

## Validation
- Storage ID must exist in `system.storages` table
- Error message: `"Storage '{storage_id}' does not exist. Create it first with CREATE STORAGE."`
- Error type: `KalamDbError::NotFound`

## Notes Addressed
✅ **Note 29**: "When creating a table either it's user/shared table you should specify a storage_id for it, if not then the local storage will be used for that"
✅ **Note 29**: "there is no storage_location column need to be there" - The `location` field is deprecated but kept for backward compatibility
✅ **Note 29**: "make sure the create user/shared table can have storage_id with it"

## Testing
All tests passing:
- `test_parse_storage_clause` - Verify parsing of `STORAGE identifier`
- `test_parse_storage_clause_with_quotes` - Verify parsing of `STORAGE 'identifier'`
- `test_parse_no_storage_clause_defaults_to_none` - Verify default behavior
- All existing shared table service tests updated and passing

## Migration Path
Existing code continues to work:
1. Old code without `STORAGE` clause defaults to 'local'
2. The deprecated `location` field is still present but should not be used
3. Storage validation happens at table creation time

## Future Improvements
1. Remove deprecated `location` field in next major version
2. Add storage_id to CREATE STREAM TABLE (currently hardcoded to 'local')
3. Consider adding storage migration commands (ALTER TABLE ... MOVE TO STORAGE)
