import type { DraftColumn, DraftTable } from "./types";

function quoteIdent(name: string): string {
  if (/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(name)) return name;
  return `"${name.replace(/"/g, '""')}"`;
}

function qualifiedName(namespace: string, name: string): string {
  return `${quoteIdent(namespace)}.${quoteIdent(name)}`;
}

function columnClause(col: DraftColumn): string {
  const parts: string[] = [quoteIdent(col.name), col.type];
  if (col.isPrimaryKey) parts.push("PRIMARY KEY");
  if (col.isNotNull) parts.push("NOT NULL");
  if (col.isUnique) parts.push("UNIQUE");
  if (col.defaultExpr.trim().length > 0) parts.push(`DEFAULT ${col.defaultExpr.trim()}`);
  return parts.join(" ");
}

export interface DraftValidation {
  table: string[];
  name: string | null;
  columns: Record<string, string>;
  hasAny: boolean;
}

export function validateDraft(draft: DraftTable): DraftValidation {
  const result: DraftValidation = {
    table: [],
    name: null,
    columns: {},
    hasAny: false,
  };

  if (!draft.name.trim()) {
    result.name = "Table name is required";
  }

  const liveCols = draft.columns.filter((c) => !c.isDeleted);
  if (liveCols.length === 0) {
    result.table.push("Table must have at least one column");
  }

  const seen = new Map<string, string[]>();
  for (const c of liveCols) {
    const trimmed = c.name.trim();
    if (!trimmed) {
      result.columns[c.id] = "Name is required";
      continue;
    }
    if (!c.type.trim()) {
      result.columns[c.id] = "Type is required";
      continue;
    }
    const lower = trimmed.toLowerCase();
    if (!seen.has(lower)) seen.set(lower, []);
    seen.get(lower)!.push(c.id);
  }
  for (const [, ids] of seen) {
    if (ids.length > 1) {
      for (const id of ids.slice(1)) {
        result.columns[id] = "Duplicate column name";
      }
    }
  }

  const pkCount = liveCols.filter((c) => c.isPrimaryKey).length;
  if (pkCount > 1) {
    result.table.push("Only one column can be marked as PRIMARY KEY");
  }

  result.hasAny =
    result.table.length > 0 || result.name !== null || Object.keys(result.columns).length > 0;
  return result;
}

export function generateCreateTableSql(draft: DraftTable): string {
  const cols = draft.columns
    .filter((c) => !c.isDeleted)
    .map((c) => columnClause(c))
    .join(", ");
  return `CREATE TABLE ${qualifiedName(draft.namespace, draft.name)} (${cols});`;
}

export function generateAlterTableSql(original: DraftTable, draft: DraftTable): string {
  const stmts: string[] = [];
  const fqn = qualifiedName(draft.namespace, draft.name);

  const originalById = new Map<string, DraftColumn>();
  for (const col of original.columns) originalById.set(col.id, col);

  for (const col of draft.columns) {
    if (col.isDeleted && !col.isNew) {
      stmts.push(`ALTER TABLE ${fqn} DROP COLUMN ${quoteIdent(col.name)};`);
    }
  }

  for (const col of draft.columns) {
    if (col.isNew && !col.isDeleted) {
      stmts.push(`ALTER TABLE ${fqn} ADD COLUMN ${columnClause(col)};`);
    }
  }

  for (const col of draft.columns) {
    if (col.isNew || col.isDeleted) continue;
    const orig = originalById.get(col.id);
    if (!orig) continue;

    if (orig.name !== col.name) {
      stmts.push(
        `ALTER TABLE ${fqn} RENAME COLUMN ${quoteIdent(orig.name)} TO ${quoteIdent(col.name)};`,
      );
    }
    if (orig.type !== col.type) {
      stmts.push(`ALTER TABLE ${fqn} MODIFY COLUMN ${quoteIdent(col.name)} ${col.type};`);
    }
    if (orig.isNotNull !== col.isNotNull) {
      stmts.push(
        `ALTER TABLE ${fqn} ALTER COLUMN ${quoteIdent(col.name)} ${col.isNotNull ? "SET NOT NULL" : "DROP NOT NULL"};`,
      );
    }
    if (orig.defaultExpr !== col.defaultExpr) {
      if (col.defaultExpr.trim().length === 0) {
        stmts.push(`ALTER TABLE ${fqn} ALTER COLUMN ${quoteIdent(col.name)} DROP DEFAULT;`);
      } else {
        stmts.push(
          `ALTER TABLE ${fqn} ALTER COLUMN ${quoteIdent(col.name)} SET DEFAULT ${col.defaultExpr.trim()};`,
        );
      }
    }
  }

  return stmts.join("\n");
}

export function generateDropTableSql(namespace: string, name: string): string {
  return `DROP TABLE ${qualifiedName(namespace, name)};`;
}
