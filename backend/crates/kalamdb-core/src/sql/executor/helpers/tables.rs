//! DDL Helper Functions
//!
//! Common utilities for DDL operations including schema transformations,
//! table validation, and metadata storage.

use crate::error::KalamDbError;
use arrow::datatypes::{DataType, Field, Schema};
use kalamdb_commons::schemas::{ColumnDefault, TableType};
use kalamdb_commons::StorageId;
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
    kalamdb_commons::validation::validate_table_name(name).map_err(|e| e.to_string())
}

/// DEPRECATED: We no longer inject auto-increment "id" fields.
/// Tables rely on _seq system column for uniqueness and user-defined primary keys.
#[deprecated(
    since = "0.2.0",
    note = "Use _seq system column and user-defined primary keys instead"
)]
pub fn inject_auto_increment_field(schema: Arc<Schema>) -> Result<Arc<Schema>, KalamDbError> {
    // No longer inject "id" field - rely on _seq and user-defined primary keys
    Ok(schema)
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
    original_arrow_schema: &Arc<Schema>,
) -> Result<(), KalamDbError> {
    use crate::app_context::AppContext;
    use crate::schema_registry::arrow_schema::ArrowSchemaWithOptions;
    use kalamdb_commons::datatypes::{FromArrowType, KalamDataType};
    use kalamdb_commons::models::TableId;
    use kalamdb_commons::schemas::{
        ColumnDefinition, SchemaVersion, TableDefinition, TableOptions,
    };

    // Extract columns directly from Arrow schema (user-provided columns only)
    let columns: Vec<ColumnDefinition> = original_arrow_schema
        .fields()
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            // Validate column name
            kalamdb_commons::validation::validate_column_name(field.name()).map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Invalid column name '{}': {}",
                    field.name(),
                    e
                ))
            })?;

            let kalam_type =
                KalamDataType::from_arrow_type(field.data_type()).unwrap_or(KalamDataType::Text);

            // T060: Mark the PRIMARY KEY column with is_primary_key = true
            let is_pk = stmt
                .primary_key_column
                .as_ref()
                .map(|pk| pk == field.name())
                .unwrap_or(false);

            Ok(ColumnDefinition::new(
                field.name().clone(),
                (idx + 1) as u32, // ordinal_position is 1-indexed
                kalam_type,
                field.is_nullable(),
                is_pk, // is_primary_key (T060: determined from stmt.primary_key_column)
                false, // is_partition_key (not used yet)
                stmt.column_defaults
                    .get(field.name())
                    .cloned()
                    .unwrap_or(ColumnDefault::None),
                None, // column_comment
            ))
        })
        .collect::<Result<Vec<_>, KalamDbError>>()?;

    // Build table options based on table type
    let table_options = match stmt.table_type {
        TableType::User => TableOptions::user(),
        TableType::Shared => TableOptions::shared(),
        TableType::Stream => TableOptions::stream(stmt.ttl_seconds.unwrap_or(3600)),
        TableType::System => TableOptions::system(),
    };

    // Create NEW TableDefinition directly (WITHOUT system columns yet)
    let mut table_def = TableDefinition::new(
        stmt.namespace_id.clone(),
        stmt.table_name.clone(),
        stmt.table_type,
        columns,
        table_options,
        None, // table_comment
    )
    .map_err(|e| KalamDbError::SchemaError(e))?;

    // Persist table-level options from DDL (storage, flush policy, ACL, TTL overrides)
    match (&mut table_def.table_options, stmt.table_type) {
        (TableOptions::User(opts), TableType::User) => {
            let storage = stmt.storage_id.clone().unwrap_or_else(StorageId::local);
            opts.storage_id = storage;
            opts.use_user_storage = stmt.use_user_storage;
            opts.flush_policy = stmt.flush_policy.clone();
        }
        (TableOptions::Shared(opts), TableType::Shared) => {
            let storage = stmt.storage_id.clone().unwrap_or_else(StorageId::local);
            opts.storage_id = storage;
            opts.access_level = stmt.access_level.clone();
            opts.flush_policy = stmt.flush_policy.clone();
        }
        (TableOptions::Stream(opts), TableType::Stream) => {
            if let Some(ttl) = stmt.ttl_seconds {
                opts.ttl_seconds = ttl;
            }
        }
        _ => {}
    }

    // Inject system columns via SystemColumnsService (Phase 12, US5, T022)
    // This adds _seq, _deleted to the TableDefinition (authoritative types)
    let app_ctx = AppContext::get();
    let sys_cols = app_ctx.system_columns_service();
    sys_cols.add_system_columns(&mut table_def)?;

    // Build Arrow schema FROM the mutated TableDefinition (includes system columns)
    let full_arrow_schema = table_def.to_arrow_schema().map_err(|e| {
        KalamDbError::SchemaError(format!(
            "Failed to build Arrow schema after system columns injection: {}",
            e
        ))
    })?;

    // Initialize schema history with version 1 entry (Initial full schema WITH system columns)
    let schema_json = ArrowSchemaWithOptions::new(full_arrow_schema.clone())
        .to_json_string()
        .map_err(|e| {
            KalamDbError::SchemaError(format!("Failed to serialize Arrow schema: {}", e))
        })?;

    table_def
        .schema_history
        .push(SchemaVersion::initial(schema_json));

    // Persist to system.tables AND cache in SchemaRegistry
    let ctx = AppContext::get();
    let schema_registry = ctx.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());

    // Write to system.tables for persistence
    let tables_provider = ctx.system_tables().tables();
    tables_provider
        .create_table(&table_id, &table_def)
        .map_err(|e| {
            KalamDbError::Other(format!(
                "Failed to save table definition to system.tables: {}",
                e
            ))
        })?;

    // Call stub method for API consistency (actual persistence handled above)
    schema_registry
        .put_table_definition(&table_id, &table_def)
        .map_err(|e| KalamDbError::Other(format!("Failed to update schema registry: {}", e)))?;

    // Prime unified schema cache with freshly saved definition (includes system columns)
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
        assert!(validate_table_name("Users").is_ok()); // Capitals are allowed
        assert!(validate_table_name("user_data_v2").is_ok());
    }

    #[test]
    fn test_validate_table_name_invalid() {
        assert!(validate_table_name("").is_err()); // Empty
        assert!(validate_table_name("_internal").is_err()); // Starts with underscore
        assert!(validate_table_name("select").is_err()); // SQL keyword
        assert!(validate_table_name("table").is_err()); // SQL keyword
        assert!(validate_table_name("123table").is_err()); // Starts with number
    }

    #[test]
    fn test_inject_auto_increment_field() {
        // DEPRECATED: This function no longer adds an "id" column
        // We rely solely on _seq system column for uniqueness
        let schema = Arc::new(Schema::new(vec![Arc::new(Field::new(
            "name",
            DataType::Utf8,
            false,
        ))]));

        let result = inject_auto_increment_field(schema.clone()).unwrap();
        // Should return schema unchanged
        assert_eq!(result.fields().len(), 1);
        assert_eq!(result.field(0).name(), "name");
        assert_eq!(result.field(0).data_type(), &DataType::Utf8);
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
}
