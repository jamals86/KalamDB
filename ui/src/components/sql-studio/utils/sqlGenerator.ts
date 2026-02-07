/**
 * sqlGenerator - Generates SQL UPDATE / DELETE statements from tracked table changes.
 *
 * Used by the SQL Preview Dialog to show users exactly what SQL will be executed
 * before they commit. Works with the change-set produced by useTableChanges.
 *
 * Supports:
 *   - UPDATE statements with WHERE clause built from primary key values
 *   - DELETE statements with WHERE clause built from primary key values
 *   - Proper escaping of string values
 *   - NULL handling
 *
 * Usage:
 *   import { generateSqlStatements } from './sqlGenerator';
 *   const sql = generateSqlStatements(namespace, tableName, edits, deletions);
 */

import type { RowEdit, RowDeletion } from '../../../hooks/useTableChanges';

// ─── Value formatting helpers ────────────────────────────────────────────────

/**
 * Escape a SQL string value (single quotes → doubled single quotes).
 */
function escapeSqlString(value: string): string {
  return value.replace(/'/g, "''");
}

/**
 * Convert a JS value to its SQL literal representation.
 */
function toSqlLiteral(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'string') return `'${escapeSqlString(value)}'`;
  // For objects / arrays – serialize as JSON string
  if (typeof value === 'object') return `'${escapeSqlString(JSON.stringify(value))}'`;
  return `'${escapeSqlString(String(value))}'`;
}

/**
 * Build a WHERE clause from primary key values.
 * Example: WHERE id = 1 AND tenant = 'acme'
 */
function buildWhereClause(primaryKeyValues: Record<string, unknown>): string {
  const conditions = Object.entries(primaryKeyValues).map(([col, val]) => {
    if (val === null || val === undefined) {
      return `${col} IS NULL`;
    }
    return `${col} = ${toSqlLiteral(val)}`;
  });
  return conditions.join(' AND ');
}

// ─── Statement generators ────────────────────────────────────────────────────

/**
 * Generate a single UPDATE statement for one row's edits.
 */
function generateUpdate(
  qualifiedTable: string,
  rowEdit: RowEdit,
): string {
  const setClauses = Object.values(rowEdit.cellEdits)
    .map((edit) => `${edit.columnName} = ${toSqlLiteral(edit.newValue)}`)
    .join(', ');

  const where = buildWhereClause(rowEdit.primaryKeyValues);
  return `UPDATE ${qualifiedTable} SET ${setClauses} WHERE ${where};`;
}

/**
 * Generate a single DELETE statement for one row.
 */
function generateDelete(
  qualifiedTable: string,
  deletion: RowDeletion,
): string {
  const where = buildWhereClause(deletion.primaryKeyValues);
  return `DELETE FROM ${qualifiedTable} WHERE ${where};`;
}

// ─── Public API ──────────────────────────────────────────────────────────────

export interface GeneratedSql {
  /** All individual SQL statements. */
  statements: string[];
  /** Combined SQL text (statements joined by newlines). */
  fullSql: string;
  /** Count of UPDATE statements. */
  updateCount: number;
  /** Count of DELETE statements. */
  deleteCount: number;
}

/**
 * Generate all SQL statements needed to apply the current change-set.
 *
 * @param namespace  - The namespace (schema) the table belongs to.
 * @param tableName  - The table name.
 * @param edits      - Map of row edits from useTableChanges.
 * @param deletions  - Map of row deletions from useTableChanges.
 * @returns An object containing individual statements and the combined SQL.
 */
export function generateSqlStatements(
  namespace: string,
  tableName: string,
  edits: Map<number, RowEdit>,
  deletions: Map<number, RowDeletion>,
): GeneratedSql {
  const qualifiedTable = `${namespace}.${tableName}`;
  const statements: string[] = [];

  // Generate UPDATE statements first (order: edits before deletes)
  const sortedEdits = Array.from(edits.values()).sort((a, b) => a.rowIndex - b.rowIndex);
  for (const rowEdit of sortedEdits) {
    // Skip rows that are also marked for deletion (delete wins)
    if (deletions.has(rowEdit.rowIndex)) continue;
    statements.push(generateUpdate(qualifiedTable, rowEdit));
  }

  // Generate DELETE statements
  const sortedDeletions = Array.from(deletions.values()).sort((a, b) => a.rowIndex - b.rowIndex);
  for (const deletion of sortedDeletions) {
    statements.push(generateDelete(qualifiedTable, deletion));
  }

  return {
    statements,
    fullSql: statements.join('\n'),
    updateCount: sortedEdits.filter((e) => !deletions.has(e.rowIndex)).length,
    deleteCount: sortedDeletions.length,
  };
}

/**
 * Generate an ALTER TABLE SQL statement for common operations.
 * Used by the TableProperties panel to preview DDL changes.
 */
export function generateAlterTableSql(
  namespace: string,
  tableName: string,
  operation: 'add_column' | 'drop_column' | 'rename_column' | 'modify_column',
  params: {
    columnName?: string;
    newColumnName?: string;
    dataType?: string;
    nullable?: boolean;
  },
): string {
  const qualifiedTable = `${namespace}.${tableName}`;

  switch (operation) {
    case 'add_column':
      return `ALTER TABLE ${qualifiedTable} ADD COLUMN ${params.columnName ?? 'new_column'} ${params.dataType ?? 'STRING'}${params.nullable === false ? ' NOT NULL' : ''};`;
    case 'drop_column':
      return `ALTER TABLE ${qualifiedTable} DROP COLUMN ${params.columnName};`;
    case 'rename_column':
      return `ALTER TABLE ${qualifiedTable} RENAME COLUMN ${params.columnName} TO ${params.newColumnName};`;
    case 'modify_column':
      return `ALTER TABLE ${qualifiedTable} MODIFY COLUMN ${params.columnName} ${params.dataType ?? 'STRING'};`;
    default:
      return `-- Unknown operation: ${operation}`;
  }
}

/**
 * Generate a DROP TABLE statement.
 */
export function generateDropTableSql(namespace: string, tableName: string): string {
  return `DROP TABLE ${namespace}.${tableName};`;
}

/**
 * Generate a CREATE TABLE DDL statement.
 */
export function generateCreateTableDdl(
  namespace: string,
  tableName: string,
  columns: { name: string; dataType: string; nullable?: boolean }[],
): string {
  const cols = columns
    .map((c) => `  ${c.name} ${c.dataType}${c.nullable === false ? ' NOT NULL' : ''}`)
    .join(',\n');
  return `CREATE TABLE ${namespace}.${tableName} (\n${cols}\n);`;
}
