# system.tables reference

system.tables exposes a flattened view of all registered tables (user, shared, stream, and system) using the unified TableDefinition model.

Columns:
- table_id (TEXT, PK): "{namespace_id}:{table_name}"
- table_name (TEXT): Name within the namespace
- namespace_id (TEXT): Owning namespace id
- table_type (TEXT): one of user|shared|stream|system
- created_at (TIMESTAMP): Creation time
- schema_version (INT): Current schema version
- table_comment (TEXT, nullable): Optional description
- updated_at (TIMESTAMP): Last modification time
- options (TEXT, nullable): Serialized TableOptions JSON for the table

Examples:
- List all tables in a namespace with their options
  SELECT table_name, table_type, options
  FROM system.tables
  WHERE namespace_id = 'analytics'
  ORDER BY table_name;

- Inspect a specific table’s options JSON
  SELECT table_id, options
  FROM system.tables
  WHERE namespace_id = 'analytics' AND table_name = 'events';

Notes:
- The options column is a JSON string representing the variant-specific TableOptions. You can parse it in clients as JSON.
- Column name is namespace (historical compatibility; was namespace_id internally). 
