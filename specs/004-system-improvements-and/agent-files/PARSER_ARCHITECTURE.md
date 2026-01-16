# Parser Architecture Decision

## Overview

KalamDB uses a hybrid parsing approach:
- **sqlparser-rs** for standard SQL (SELECT, INSERT, UPDATE, DELETE, CREATE TABLE, etc.)
- **Custom parsers** for KalamDB-specific extensions

## Custom Parsers (KalamDB-Specific Commands)

The following commands use custom parsers because they do not fit standard SQL syntax:

### 1. Storage Management (`storage_commands.rs`)

**Commands:**
- `CREATE STORAGE <id> TYPE <type> NAME '<name>' BASE_DIRECTORY '<path>' ...`
- `ALTER STORAGE <id> SET NAME '<name>' ...`
- `DROP STORAGE <id>`
- `SHOW STORAGES`

**Rationale:**
- STORAGE is not a standard SQL object type
- Custom KalamDB feature for managing Parquet storage backends
- Syntax includes KalamDB-specific keywords (TYPE, BASE_DIRECTORY, SHARED_TABLES_TEMPLATE, etc.)

### 2. Flush Commands (`flush_commands.rs`)

**Commands:**
- `STORAGE FLUSH TABLE <namespace>.<table>`
- `STORAGE FLUSH ALL IN <namespace>`

**Rationale:**
- Not part of SQL standard
- KalamDB-specific command for triggering asynchronous RocksDB → Parquet flush
- Returns job_id for background job tracking

### 3. Job Management (`job_commands.rs`)

**Commands:**
- `KILL JOB '<job_id>'`

**Rationale:**
- While `KILL` exists in MySQL (for connections), KalamDB uses it for background jobs
- Custom implementation for async job management (flush operations, etc.)
- Returns structured response with job status

### 4. System Table Queries (`parser/system.rs`)

**Commands:**
- `SELECT * FROM system.tables`
- `INSERT INTO system.namespaces ...`
- `UPDATE system.storages SET ...`
- `DELETE FROM system.jobs WHERE ...`

**Rationale:**
- Uses sqlparser-rs to parse the SQL syntax
- Custom layer to identify `system.*` table references
- Routes queries to metadata store instead of user data storage

## Standard SQL (Using sqlparser-rs)

The following commands use sqlparser-rs with multi-dialect support:

### DDL Commands
- `CREATE NAMESPACE <name>` - Uses custom parser (simple keyword extraction)
- `CREATE USER TABLE <name> (...)` - Uses sqlparser-rs (removes USER keyword, parses as standard CREATE TABLE)
- `DROP TABLE <name>` - Uses custom parser (handles USER/SHARED/STREAM table types)

### DML Commands
- `SELECT * FROM <table> WHERE ...`
- `INSERT INTO <table> VALUES (...)`
- `UPDATE <table> SET ... WHERE ...`
- `DELETE FROM <table> WHERE ...`

**Dialect Support:**
- PostgreSQL (primary)
- MySQL (fallback)
- Generic SQL (fallback)

## Shared Utilities (`parser/utils.rs`)

To avoid code duplication, common parsing utilities are provided:

```rust
normalize_sql(sql)                          // Remove extra whitespace, semicolons
extract_quoted_keyword_value(sql, keyword)  // Extract 'value' after KEYWORD
extract_keyword_value(sql, keyword)         // Extract quoted or unquoted value
extract_identifier(sql, skip_tokens)        // Extract Nth token
extract_qualified_table(table_ref)          // Parse namespace.table_name
```

**Usage:**
- `storage_commands.rs` - Uses for keyword extraction (NAME, TYPE, BASE_DIRECTORY, etc.)
- `flush_commands.rs` - Uses for normalization and qualified table parsing
- `job_commands.rs` - Uses for normalization

## Design Principles

1. **Prefer sqlparser-rs for standard SQL**
   - Battle-tested library with extensive SQL dialect support
   - Handles complex SQL syntax (subqueries, joins, CTEs, etc.)
   - Reduces maintenance burden

2. **Custom parsers only for KalamDB extensions**
   - Commands that don't fit SQL standards
   - Simple keyword-based parsing using shared utilities
   - Clear error messages for user-facing commands

3. **Multi-dialect compatibility**
   - PostgreSQL syntax preferred
   - MySQL fallback for common variants (AUTO_INCREMENT, etc.)
   - Generic fallback for basic SQL

4. **Type-safe AST**
   - All parsers return strongly-typed statement structs
   - Clear separation between parsing and execution
   - Validation at parse time where possible

## Future Considerations

### Potential for sqlparser-rs Usage

**CREATE NAMESPACE:**
- Currently uses simple keyword extraction
- Could potentially use sqlparser-rs with custom dialect
- Current implementation is simple and works well

**DROP TABLE:**
- Currently uses custom parser for USER/SHARED/STREAM types
- Could potentially use sqlparser-rs with preprocessing
- Current implementation handles KalamDB-specific table types clearly

### Keeping Custom

**Storage Management:**
- Too specific to KalamDB architecture
- Syntax doesn't match any SQL standard
- Will remain custom

**Flush Commands:**
- Not part of SQL standard
- Simple syntax makes custom parser appropriate
- Will remain custom

**Job Management:**
- Custom async job tracking system
- Will remain custom

## References

- sqlparser-rs documentation: https://docs.rs/sqlparser/
- PostgreSQL SQL syntax: https://www.postgresql.org/docs/current/sql.html
- MySQL SQL syntax: https://dev.mysql.com/doc/refman/8.0/en/sql-statements.html
