# Schema & Data Operations Test Scenarios [CATEGORY:DDL_DML]

This document covers SQL correctness, schema evolution, and data integrity.

## 1. Schema Lifecycle (DDL)
**Goal**: Verify table creation and modification across different table types.
- **Scenario A**: Table Types (USER, SHARED, STREAM).
  1. Create each table type with various column definitions.
  2. Verify correct metadata registration in `system.tables`.
- **Scenario B**: Schema Evolution (ALTER).
  1. `ALTER TABLE ADD COLUMN` on a table with existing data.
  2. Verify old rows are readable (null/default) and new rows work.
  3. Verify live subscriptions receive schema change metadata or continue gracefully.
- **Agent Friendly**: Check `backend/tests/misc/schema/test_alter_table.rs`.

## 2. Data Integrity (DML)
**Goal**: Core CRUD operations and type safety.
- **Scenario A**: Multi-type inserts.
  1. Insert rows with BIGINT, TIMESTAMP, JSON, BOOLEAN, DOUBLE.
  2. Verify round-trip accuracy (no precision loss).
- **Scenario B**: Update & Delete with MVCC.
  1. Perform UPDATE on primary key.
  2. Perform soft DELETE.
  3. Verify `_deleted` flag vs default filtering.
- **Agent Friendly**: Check `backend/tests/misc/sql/test_dml_complex.rs`.

## 3. Query Execution & Optimization
**Goal**: SQL engine correctness via DataFusion.
- **Scenario A**: Join USER with SHARED.
  1. Join a user-specific table with a global reference table.
  2. Verify authorization allows reading the shared table.
- **Scenario B**: Index Usage.
  1. Run `EXPLAIN SELECT ...` on a primary key query.
  2. Verify the plan shows an index scan, not a full table scan.
- **Agent Friendly**: Check `backend/tests/misc/sql/test_explain_index_usage.rs`.

## 4. Namespace Management
**Goal**: Logical isolation of tables.
- **Steps**:
  1. Create multiple namespaces.
  2. Attempt cross-namespace access without qualification.
  3. Verify naming collisions (same table name in different namespaces) are handled correctly.
- **Agent Friendly**: Check `backend/tests/testserver/sql/test_namespace_validation_http.rs`.

## Coverage Analysis
- **Duplicates**: Basic CRUD is heavily duplicated in `test_user_sql_commands_http.rs` and `test_dml_complex.rs`.
- **Gaps**: `ALTER TABLE DROP COLUMN` has limited coverage. JSON path expressions in WHERE clauses need more exhaustive testing.
