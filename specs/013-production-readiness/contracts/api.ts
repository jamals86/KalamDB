// API Contracts for Production Readiness

// 1. ALTER TABLE
// POST /api/v1/query
// Request
interface AlterTableRequest {
  sql: string; // "ALTER TABLE users ADD COLUMN age INT"
}

// Response
interface AlterTableResponse {
  success: boolean;
  message: string;
  affected_rows: number; // 0
}

// 2. System Tables Query
// POST /api/v1/query
// Request
interface SystemQueryRequest {
  sql: string; // "SELECT * FROM system.tables"
}

// Response
interface SystemQueryResponse {
  schema: SchemaDefinition;
  data: Row[];
}

// 3. Manifest Inspection (Internal/Debug)
// GET /api/v1/debug/manifest/:table_id
interface ManifestDebugResponse {
  table_id: string;
  version: number;
  segments: {
    id: string;
    path: string;
    row_count: number;
  }[];
}
