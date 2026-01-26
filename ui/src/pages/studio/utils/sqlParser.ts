/**
 * Utility to extract table context from SQL query.
 * Used to build download URLs for FILE datatype columns.
 */

export interface TableContext {
  namespace: string;
  tableName: string;
}

/**
 * Extract namespace and table name from a SQL query.
 * Handles common patterns like:
 * - SELECT * FROM namespace.table
 * - SELECT * FROM table (assumes default namespace)
 * - INSERT INTO namespace.table
 */
export function extractTableContext(sql: string): TableContext | null {
  if (!sql) return null;

  // Remove comments and normalize whitespace
  const normalized = sql
    .replace(/--[^\n]*/g, '') // Remove line comments
    .replace(/\/\*[\s\S]*?\*\//g, '') // Remove block comments
    .replace(/\s+/g, ' ') // Normalize whitespace
    .trim()
    .toUpperCase();

  // Pattern 1: FROM/INTO namespace.table
  const qualifiedMatch = normalized.match(/(?:FROM|INTO|UPDATE|TABLE)\s+([a-z_][a-z0-9_]*\.[a-z_][a-z0-9_]*)/i);
  if (qualifiedMatch) {
    const [namespace, tableName] = qualifiedMatch[1].split('.');
    return { namespace: namespace.toLowerCase(), tableName: tableName.toLowerCase() };
  }

  // Pattern 2: FROM/INTO table (no namespace - assume 'default')
  const simpleMatch = normalized.match(/(?:FROM|INTO|UPDATE|TABLE)\s+([a-z_][a-z0-9_]*)/i);
  if (simpleMatch) {
    return { namespace: 'default', tableName: simpleMatch[1].toLowerCase() };
  }

  return null;
}
