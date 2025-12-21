// Utilities to normalize KalamDB SQL query responses (ESM).

/**
 * Get column names from schema or columns array (backwards compatible)
 * New format: schema = [{name, data_type, index}, ...]
 * Old format: columns = ['name1', 'name2', ...]
 * @param {object} resp - Query response
 * @returns {string[]} Column names
 */
function getColumnNames(resp) {
  // New format: schema array with name, data_type, index
  if (Array.isArray(resp.schema) && resp.schema.length > 0) {
    // Sort by index to ensure correct order
    return resp.schema
      .slice()
      .sort((a, b) => (a.index ?? 0) - (b.index ?? 0))
      .map(field => field.name);
  }
  // Old format: columns array of strings
  if (Array.isArray(resp.columns)) {
    return resp.columns;
  }
  return [];
}

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
 * Supports both new format (schema) and old format (columns).
 * @param {{schema?: {name: string, data_type: string, index: number}[], columns?: string[], rows: any[], [k:string]: any}} resp
 * @param {string[]} preferredOrder
 */
export function normalizeQueryResponse(resp, preferredOrder) {
  const currentColumns = getColumnNames(resp);
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

  // Build new schema with updated indices
  const newSchema = newColumns.map((name, index) => {
    // Find original field to preserve data_type
    const original = resp.schema?.find(f => f.name === name);
    return {
      name,
      data_type: original?.data_type ?? 'Text',
      index
    };
  });

  return { ...resp, schema: newSchema, rows: newRows };
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

