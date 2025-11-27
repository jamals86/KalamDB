import assert from 'node:assert/strict';
import { normalizeQueryResponse, sortColumns, SYSTEM_TABLES_ORDER } from '../src/query_normalize.js';

// Basic behavior: reorder columns and rows (array form)
{
  const resp = {
    columns: ['schema_version', 'table_name', 'table_id'],
    rows: [
      ['v3', 'users', 'tbl_1'],
      ['v4', 'jobs', 'tbl_2'],
    ],
  };

  const out = normalizeQueryResponse(resp, SYSTEM_TABLES_ORDER);
  assert.deepEqual(out.columns.slice(0, 3), ['table_id', 'table_name', 'schema_version']);
  assert.deepEqual(out.rows[0].slice(0, 3), ['tbl_1', 'users', 'v3']);
}

// Object rows: convert to arrays in preferred order
{
  const resp = {
    columns: ['table_name', 'access_level', 'table_id'],
    rows: [
      { table_id: 't1', table_name: 'tables', access_level: 'public' },
    ],
  };

  const out = normalizeQueryResponse(resp, SYSTEM_TABLES_ORDER);
  // ensure first columns match preferred where present
  assert.equal(out.columns[0], 'table_id');
  // row aligned
  assert.equal(out.rows[0][0], 't1');
}

// sortColumns leaves unknown columns after preferred ones
{
  const cols = ['x', 'table_name', 'y', 'table_id'];
  const sorted = sortColumns(cols, SYSTEM_TABLES_ORDER);
  const idxTableId = sorted.indexOf('table_id');
  const idxTableName = sorted.indexOf('table_name');
  assert.ok(idxTableId < idxTableName, 'table_id should come before table_name');
  // unknowns at the end (relative order preserved: x before y)
  const tail = sorted.slice(-2);
  assert.deepEqual(tail, ['x', 'y']);
}

console.log('normalize.test.mjs passed');
