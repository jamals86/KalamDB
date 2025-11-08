//! DDL Helper Functions
//!
//! Common utilities for DDL operations including schema transformations,
//! table validation, and metadata storage.

use crate::error::KalamDbError;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use kalamdb_commons::schemas::{ColumnDefault, TableType};
use kalamdb_sql::ddl::CreateTableStatement;
use std::sync::Arc;

/// Validate table name
///
/// # Rules
/// - Must start with lowercase letter or underscore
/// - Cannot be a SQL keyword
///
/// # Arguments
/// * `name` - Table name to validate
///
/// # Returns
/// Ok(()) if valid, error otherwise
pub fn validate_table_name(name: &str) -> Result<(), String> {
    // Check first character
    let first_char = name.chars().next().ok_or_else(|| {
        "Table name cannot be empty".to_string()
    })?;

    if !first_char.is_lowercase() && first_char != '_' {
        return Err(format!(
            "Table name must start with lowercase letter or underscore: {}",
            name
        ));
    }

    // Check for SQL keywords
    let keywords = [
        "select", "insert", "update", "delete", "table", "from", "where",
    ];
    if keywords.contains(&name.to_lowercase().as_str()) {
        return Err(format!("Table name cannot be a SQL keyword: {}", name));
    }

    Ok(())
}

/// Inject auto-increment field if not present
///
/// Adds a snowflake ID field named "id" as the first column if no field named "id" exists.
/// Uses Int64 type for snowflake IDs.
pub fn inject_auto_increment_field(
    schema: Arc<Schema>,
) -> Result<Arc<Schema>, KalamDbError> {
    // Check if "id" field already exists
    if schema.field_with_name("id").is_ok() {
        return Ok(schema);
    }

    // Create snowflake ID field
    let id_field = Arc::new(Field::new("id", DataType::Int64, false)); // Not nullable

    // Create new schema with ID field as first column
    let mut fields = vec![id_field];
    fields.extend(schema.fields().iter().cloned());

    Ok(Arc::new(Schema::new(fields)))
}

/// Inject system columns for user tables
///
/// Adds _updated (TIMESTAMP) and _deleted (BOOLEAN) columns.
/// For stream tables, this should NOT be called (handled in stream table service).
pub fn inject_system_columns(
    schema: Arc<Schema>,
    table_type: TableType,
) -> Result<Arc<Schema>, KalamDbError> {
    // Stream tables do NOT have system columns
    if table_type == TableType::Stream {
        return Ok(schema);
    }

    // Check if system columns already exist
    let has_updated = schema.field_with_name("_updated").is_ok();
    let has_deleted = schema.field_with_name("_deleted").is_ok();

    if has_updated && has_deleted {
        return Ok(schema);
    }

    // Create system columns
    let mut fields: Vec<Arc<Field>> = schema.fields().iter().cloned().collect();

    if !has_updated {
        fields.push(Arc::new(Field::new(
            "_updated",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false, // Not nullable
        )));
    }

    if !has_deleted {
        fields.push(Arc::new(Field::new("_deleted", DataType::Boolean, false)));
    }

    Ok(Arc::new(Schema::new(fields)))
}

/// Save table definition to information_schema.tables
///
/// Replaces fragmented schema storage with single atomic write.
///
/// # Arguments
/// * `stmt` - CREATE TABLE statement with all metadata
/// * `schema` - Final Arrow schema (after auto-increment and system column injection)
///
/// # Returns
/// Ok(()) on success, error on failure
pub fn save_table_definition(
    stmt: &CreateTableStatement,
    schema: &Arc<Schema>,
) -> Result<(), KalamDbError> {
    use crate::app_context::AppContext;
    use crate::schema_registry::arrow_schema::ArrowSchemaWithOptions;
    use kalamdb_commons::datatypes::{FromArrowType, KalamDataType};
    use kalamdb_commons::models::TableId;
    use kalamdb_commons::schemas::{
        ColumnDefinition, SchemaVersion, TableDefinition, TableOptions,
    };

    // Extract columns directly from Arrow schema
    let columns: Vec<ColumnDefinition> = schema
        .fields()
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let kalam_type = KalamDataType::from_arrow_type(field.data_type())
                .unwrap_or(KalamDataType::Text);

            ColumnDefinition::new(
                field.name().clone(),
                (idx + 1) as u32, // ordinal_position is 1-indexed
                kalam_type,
                field.is_nullable(),
                false, // is_primary_key (determined separately)
                false, // is_partition_key (not used yet)
                stmt.column_defaults.get(field.name()).cloned().unwrap_or(ColumnDefault::None),
                None, // column_comment
            )
        })
        .collect();

    // Build table options based on table type
    let table_options = match stmt.table_type {
        TableType::User => TableOptions::user(),
        TableType::Shared => TableOptions::shared(),
        TableType::Stream => TableOptions::stream(stmt.ttl_seconds.unwrap_or(3600)),
        TableType::System => TableOptions::system(),
    };

    // Create NEW TableDefinition directly
    let mut table_def = TableDefinition::new(
        stmt.namespace_id.clone(),
        stmt.table_name.clone(),
        stmt.table_type,
        columns,
        table_options,
        None, // table_comment
    )
    .map_err(|e| KalamDbError::SchemaError(e))?;

    // Initialize schema history with version 1 entry (Initial schema)
    let schema_json = ArrowSchemaWithOptions::new(schema.clone())
        .to_json_string()
        .map_err(|e| {
            KalamDbError::SchemaError(format!("Failed to serialize Arrow schema: {}", e))
        })?;

    table_def
        .schema_history
        .push(SchemaVersion::initial(schema_json));

    // Single atomic write to information_schema_tables via SchemaRegistry
    let ctx = AppContext::get();
    let schema_registry = ctx.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
    schema_registry
        .put_table_definition(&table_id, &table_def)
        .map_err(|e| {
            KalamDbError::Other(format!(
                "Failed to save table definition to information_schema.tables: {}",
                e
            ))
        })?;

    // Prime unified schema cache with freshly saved definition so providers can resolve Arrow schema immediately
    {
        use crate::schema_registry::CachedTableData;
        let cached = Arc::new(CachedTableData::new(Arc::new(table_def.clone())));
        schema_registry.insert(table_id.clone(), cached);
    }

    log::info!(
        "Table definition for {}.{} saved to information_schema.tables (version 1)",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_table_name_valid() {
        assert!(validate_table_name("users").is_ok());
        assert!(validate_table_name("_internal").is_ok());
        assert!(validate_table_name("user_data_v2").is_ok());
    }

    #[test]
    fn test_validate_table_name_invalid() {
        assert!(validate_table_name("").is_err());
        assert!(validate_table_name("Users").is_err()); // Capital letter
        assert!(validate_table_name("select").is_err()); // SQL keyword
        assert!(validate_table_name("table").is_err()); // SQL keyword
    }

    #[test]
    fn test_inject_auto_increment_field() {
        let schema = Arc::new(Schema::new(vec![Arc::new(Field::new(
            "name",
            DataType::Utf8,
            false,
        ))]));

        let result = inject_auto_increment_field(schema.clone()).unwrap();
        assert_eq!(result.fields().len(), 2);
        assert_eq!(result.field(0).name(), "id");
        assert_eq!(result.field(0).data_type(), &DataType::Int64);
    }

    #[test]
    fn test_inject_auto_increment_field_already_exists() {
        let schema = Arc::new(Schema::new(vec![
            Arc::new(Field::new("id", DataType::Int64, false)),
            Arc::new(Field::new("name", DataType::Utf8, false)),
        ]));

        let result = inject_auto_increment_field(schema.clone()).unwrap();
        assert_eq!(result.fields().len(), 2); // No change
        assert_eq!(result.field(0).name(), "id");
    }

    #[test]
    fn test_inject_system_columns() {
        let schema = Arc::new(Schema::new(vec![Arc::new(Field::new(
            "data",
            DataType::Utf8,
            false,
        ))]));

        let result = inject_system_columns(schema, TableType::User).unwrap();
        assert_eq!(result.fields().len(), 3);
        assert_eq!(result.field(1).name(), "_updated");
        assert_eq!(result.field(2).name(), "_deleted");
    }

    #[test]
    fn test_inject_system_columns_stream_table() {
        let schema = Arc::new(Schema::new(vec![Arc::new(Field::new(
            "data",
            DataType::Utf8,
            false,
        ))]));

        let result = inject_system_columns(schema.clone(), TableType::Stream).unwrap();
        assert_eq!(result.fields().len(), 1); // No system columns for stream tables
    }
}
