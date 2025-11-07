//! Information Schema Columns Provider
//!
//! Provides column metadata by flattening the columns array from TableDefinition.
//! Reads from information_schema_tables CF and exposes per-column records.
//!
//! **Updated**: Now uses unified VirtualView pattern from view_base.rs

use crate::error::KalamDbError;
use crate::schema_registry::views::VirtualView;
use crate::schema_registry::SchemaRegistry;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use std::sync::Arc;

#[derive(Debug)]
pub struct InformationSchemaColumnsView {
    _schema_registry: Arc<SchemaRegistry>, // TODO: Use this instead of kalam_sql
    schema: SchemaRef,
}

impl InformationSchemaColumnsView {
    pub fn new(schema_registry: Arc<SchemaRegistry>) -> Self {
        let schema = Arc::new(Schema::new(vec![
            Field::new("table_catalog", DataType::Utf8, true),
            Field::new("table_schema", DataType::Utf8, false),
            Field::new("table_name", DataType::Utf8, false),
            Field::new("column_name", DataType::Utf8, false),
            Field::new("ordinal_position", DataType::UInt32, false), // 1-indexed
            Field::new("data_type", DataType::Utf8, false),
            Field::new("is_nullable", DataType::Boolean, false),
            Field::new("column_default", DataType::Utf8, true),
            Field::new("is_primary_key", DataType::Boolean, false),
        ]));

        Self { 
            _schema_registry: schema_registry,
            schema 
        }
    }

    fn scan_all_columns(&self) -> Result<RecordBatch, KalamDbError> {
        // TODO: Implement using schema_registry once refactored
        // For now, return empty result
        return Err(KalamDbError::Other("information_schema.columns not yet implemented (needs SchemaRegistry refactoring)".to_string()));
        
        /* OLD CODE - needs refactoring
        // Flatten columns from all tables
        let mut table_catalog_values = Vec::new();
        let mut table_schema_values = Vec::new();
        let mut table_name_values = Vec::new();
        let mut column_name_values = Vec::new();
        let mut ordinal_position_values = Vec::new();
        let mut data_type_values: Vec<String> = Vec::new();
        let mut is_nullable_values = Vec::new();
        let mut column_default_values: Vec<Option<String>> = Vec::new();
        let mut is_primary_key_values = Vec::new();

        for _table_def in tables {
            for column in &table_def.columns {
                table_catalog_values.push(None::<String>); // NULL for now
                table_schema_values.push(table_def.namespace_id.to_string());
                table_name_values.push(table_def.table_name.to_string());
                column_name_values.push(column.column_name.clone());
                ordinal_position_values.push(column.ordinal_position);
                // Convert KalamDataType to string
                data_type_values.push(format!("{:?}", column.data_type));
                is_nullable_values.push(column.is_nullable);
                // Convert ColumnDefault to Option<String>
                let default_str = match &column.default_value {
                    kalamdb_commons::schemas::ColumnDefault::None => None,
                    kalamdb_commons::schemas::ColumnDefault::Literal(val) => Some(val.to_string()),
                    kalamdb_commons::schemas::ColumnDefault::FunctionCall { name, .. } => Some(format!("{}()", name)),
                };
                column_default_values.push(default_str);
                is_primary_key_values.push(column.is_primary_key);
            }
        }

        // Create RecordBatch
        let batch = RecordBatch::try_new(
            Arc::clone(&self.schema),
            vec![
                Arc::new(StringArray::from(table_catalog_values)),
                Arc::new(StringArray::from(table_schema_values)),
                Arc::new(StringArray::from(table_name_values)),
                Arc::new(StringArray::from(column_name_values)),
                Arc::new(UInt32Array::from(ordinal_position_values)),
                Arc::new(StringArray::from(data_type_values)),
                Arc::new(BooleanArray::from(is_nullable_values)),
                Arc::new(StringArray::from(column_default_values)),
                Arc::new(BooleanArray::from(is_primary_key_values)),
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
        */
    }
}

impl VirtualView for InformationSchemaColumnsView {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn compute_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_columns()
    }

    fn view_name(&self) -> &str {
        "information_schema.columns"
    }
}

/// Helper function to create information_schema.columns TableProvider
///
/// This wraps the InformationSchemaColumnsView in a ViewTableProvider for use in DataFusion.
pub fn create_information_schema_columns_provider(
    schema_registry: Arc<SchemaRegistry>,
) -> Arc<dyn datafusion::datasource::TableProvider> {
    use crate::schema_registry::views::ViewTableProvider;
    let view = Arc::new(InformationSchemaColumnsView::new(schema_registry));
    Arc::new(ViewTableProvider::new(view))
}

// Keep old name for backward compatibility (deprecated)
#[deprecated(note = "Use InformationSchemaColumnsView instead")]
pub type InformationSchemaColumnsProvider = InformationSchemaColumnsView;
