# Contract: SqlExecutor DDL/DML Handlers (009)

Handlers are focused units that translate SQL statements into provider/store operations using AppContext and SchemaRegistry.

## Common Handler Signature

- fn execute_*(session: &SessionContext, sql: &str, ctx: &ExecutionContext) -> Result<ExecutionResult>

## DDL

- execute_create_namespace()
- execute_drop_namespace()
- execute_create_storage()
- execute_create_table()
- execute_alter_table()
- execute_drop_table()

Requirements:
- All lookups for table existence/type via SchemaRegistry
- Cache invalidation on ALTER/DROP
- RBAC checks early via AppContext auth

## DML

- execute_select()
- execute_insert()
- execute_update()
- execute_delete()

Requirements:
- Route to User/Shared/Stream Table stores accordingly
- Apply column defaults and auto-generated fields consistently

## Errors

- Use KalamDbError with clear messages; do not leak internal storage errors

## Success Criteria

- Existing tests for DDL/DML pass without regressions
