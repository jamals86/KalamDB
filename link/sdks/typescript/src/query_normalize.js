// Utilities to normalize KalamDB SQL query responses (ESM).

/**
 * Compute a stable sorted columns array given a preferred order.
 * Columns in preferredOrder come first by that exact order;
 * others are appended keeping original relative order.
 * @param {string[]} columns
 * @param {string[]} preferredOrder
 */
export function sortColumns(columns, preferredOrder) {
  const orderIndex = new Map();
  preferredOrder.forEach((name, i) => orderIndex.set(name, i));

  const listed = [];
  const unlisted = [];
  for (const c of columns) {
    (orderIndex.has(c) ? listed : unlisted).push(c);
  }
  listed.sort((a, b) => orderIndex.get(a) - orderIndex.get(b));
  return [...listed, ...unlisted];
}

function remapArrayRow(row, currentColumns, newColumns) {
  const idxMap = new Map();
  currentColumns.forEach((c, i) => idxMap.set(c, i));
  return newColumns.map(c => (idxMap.has(c) ? row[idxMap.get(c)] : null));
}

function objectRowToArray(row, newColumns) {
  return newColumns.map(c => (c in row ? row[c] : null));
}

/**
 * Normalize query response to the preferred column order.
 * @param {{columns: string[], rows: any[], [k:string]: any}} resp
 * @param {string[]} preferredOrder
 */
export function normalizeQueryResponse(resp, preferredOrder) {
  const currentColumns = Array.isArray(resp.columns) ? resp.columns : [];
  const newColumns = sortColumns(currentColumns, preferredOrder);

  let newRows = [];
  const rows = Array.isArray(resp.rows) ? resp.rows : [];
  if (rows.length > 0) {
    const first = rows[0];
    if (Array.isArray(first)) {
      newRows = rows.map(r => remapArrayRow(r, currentColumns, newColumns));
    } else if (first && typeof first === 'object') {
      newRows = rows.map(r => objectRowToArray(r, newColumns));
    } else {
      newRows = rows;
    }
  }

  return { ...resp, columns: newColumns, rows: newRows };
}

// Canonical order for system.tables
export const SYSTEM_TABLES_ORDER = [
  'table_id',
  'table_name',
  'namespace',
  'table_type',
  'created_at',
  'storage_location',
  'storage_id',
  'use_user_storage',
  'flush_policy',
  'schema_version',
  'deleted_retention_hours',
  'access_level',
];

