export function isReadOnlyNamespace(name: string): boolean {
  return (
    name === "information_schema" ||
    name === "pg_catalog" ||
    name === "datafusion" ||
    name.startsWith("system") ||
    name.startsWith("dba")
  );
}

export const KALAMDB_TYPES = [
  "BOOLEAN",
  "SMALLINT",
  "INT",
  "BIGINT",
  "FLOAT",
  "DOUBLE",
  "DECIMAL",
  "TEXT",
  "TIMESTAMP",
  "DATE",
  "DATETIME",
  "TIME",
  "JSON",
  "BYTES",
  "UUID",
  "EMBEDDING",
  "FILE",
] as const;

export type KalamDbType = (typeof KALAMDB_TYPES)[number];

export const DEFAULT_NONE = "__NONE__";
export const DEFAULT_CUSTOM = "__CUSTOM__";

export const DEFAULT_PRESETS: Array<{ label: string; value: string; description?: string }> = [
  { label: "(none)", value: DEFAULT_NONE },
  { label: "SNOWFLAKE_ID()", value: "SNOWFLAKE_ID()", description: "Auto-generated bigint id" },
  { label: "NOW()", value: "NOW()", description: "Current timestamp" },
  { label: "UUID_GENERATE_V7()", value: "UUID_GENERATE_V7()", description: "Auto-generated UUID v7" },
  { label: "ULID()", value: "ULID()", description: "ULID string" },
  { label: "Custom...", value: DEFAULT_CUSTOM },
];

export interface DraftColumn {
  id: string;
  name: string;
  type: string;
  isPrimaryKey: boolean;
  isNotNull: boolean;
  isUnique: boolean;
  defaultExpr: string;
  isNew: boolean;
  isDeleted: boolean;
}

export type EditorMode = "idle" | "create" | "edit";

export interface DraftTable {
  namespace: string;
  name: string;
  columns: DraftColumn[];
}

function newId(): string {
  return typeof crypto !== "undefined" && crypto.randomUUID
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2);
}

export function newDraftColumn(): DraftColumn {
  return {
    id: newId(),
    name: "",
    type: "TEXT",
    isPrimaryKey: false,
    isNotNull: false,
    isUnique: false,
    defaultExpr: "",
    isNew: true,
    isDeleted: false,
  };
}

export function tableToDraft(table: {
  namespace: string;
  name: string;
  columns: Array<{ name: string; dataType: string; isNullable: boolean; isPrimaryKey: boolean }>;
}): DraftTable {
  const columns: DraftColumn[] = table.columns
    .filter((c) => !c.name.startsWith("_"))
    .map((c) => ({
      id: newId(),
      name: c.name,
      type: (c.dataType ?? "TEXT").toUpperCase(),
      isPrimaryKey: c.isPrimaryKey,
      isNotNull: !c.isNullable,
      isUnique: false,
      defaultExpr: "",
      isNew: false,
      isDeleted: false,
    }));
  return { namespace: table.namespace, name: table.name, columns };
}

export function emptyDraft(namespace = "default"): DraftTable {
  const idCol = newDraftColumn();
  idCol.name = "id";
  idCol.type = "BIGINT";
  idCol.isPrimaryKey = true;
  idCol.isNotNull = true;
  idCol.defaultExpr = "SNOWFLAKE_ID()";
  return { namespace, name: "", columns: [idCol] };
}
