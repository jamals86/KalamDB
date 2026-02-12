export interface CreateStorageInput {
  storage_id: string;
  storage_type: "filesystem" | "s3" | "gcs" | "azure";
  storage_name: string;
  description?: string;
  base_directory: string;
  credentials?: string;
  config_json?: string;
  shared_tables_template?: string;
  user_tables_template?: string;
}

export interface UpdateStorageInput {
  storage_name?: string;
  description?: string;
  config_json?: string;
  shared_tables_template?: string;
  user_tables_template?: string;
}

export const SYSTEM_STORAGES_QUERY = `
  SELECT storage_id, storage_name, description, storage_type, base_directory,
         credentials, config_json, shared_tables_template, user_tables_template, created_at, updated_at
  FROM system.storages
  ORDER BY storage_name
`;

function escapeSqlLiteral(value: string): string {
  return value.replace(/'/g, "''");
}

export function buildCreateStorageSql(input: CreateStorageInput): string {
  let sql = `CREATE STORAGE ${input.storage_id} TYPE ${input.storage_type}`;
  sql += ` NAME '${escapeSqlLiteral(input.storage_name)}'`;

  if (input.description) {
    sql += ` DESCRIPTION '${escapeSqlLiteral(input.description)}'`;
  }

  if (input.storage_type === "filesystem") {
    sql += ` PATH '${escapeSqlLiteral(input.base_directory)}'`;
  } else {
    sql += ` BUCKET '${escapeSqlLiteral(input.base_directory)}'`;
  }

  if (input.credentials?.trim()) {
    sql += ` CREDENTIALS '${escapeSqlLiteral(input.credentials.trim())}'`;
  }

  if (input.config_json?.trim()) {
    sql += ` CONFIG '${escapeSqlLiteral(input.config_json.trim())}'`;
  }

  if (input.shared_tables_template) {
    sql += ` SHARED_TABLES_TEMPLATE '${escapeSqlLiteral(input.shared_tables_template)}'`;
  }

  if (input.user_tables_template) {
    sql += ` USER_TABLES_TEMPLATE '${escapeSqlLiteral(input.user_tables_template)}'`;
  }

  return sql;
}

export function buildUpdateStorageSql(storageId: string, input: UpdateStorageInput): string | null {
  const setClauses: string[] = [];

  if (input.storage_name) {
    setClauses.push(`SET NAME '${escapeSqlLiteral(input.storage_name)}'`);
  }
  if (input.description !== undefined) {
    setClauses.push(`SET DESCRIPTION '${escapeSqlLiteral(input.description || "")}'`);
  }
  if (input.config_json !== undefined) {
    setClauses.push(`SET CONFIG '${escapeSqlLiteral(input.config_json || "")}'`);
  }
  if (input.shared_tables_template !== undefined) {
    setClauses.push(`SET SHARED_TABLES_TEMPLATE '${escapeSqlLiteral(input.shared_tables_template || "")}'`);
  }
  if (input.user_tables_template !== undefined) {
    setClauses.push(`SET USER_TABLES_TEMPLATE '${escapeSqlLiteral(input.user_tables_template || "")}'`);
  }

  if (setClauses.length === 0) {
    return null;
  }

  return `ALTER STORAGE ${storageId} ${setClauses.join(" ")}`;
}

export function buildStorageHealthCheckSql(storageId: string, extended = true): string {
  return extended
    ? `STORAGE CHECK ${storageId} EXTENDED`
    : `STORAGE CHECK ${storageId}`;
}
