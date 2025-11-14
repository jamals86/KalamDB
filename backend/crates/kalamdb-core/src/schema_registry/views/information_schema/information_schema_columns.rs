//! Information Schema Columns Provider
//!
//! Provides column metadata by flattening the columns array from TableDefinition.
//! Reads from system.tables and exposes per-column records.
//!
//! **Updated**: Now uses unified VirtualView pattern from view_base.rs

use super::super::super::error::RegistryError;
use super::super::super::traits::TablesTableProvider;
use super::super::VirtualView;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use std::sync::Arc;

#[derive(Debug)]
pub struct InformationSchemaColumnsView {
    tables_provider: Arc<TablesTableProvider>,
    schema: SchemaRef,
}

impl InformationSchemaColumnsView {
    pub fn new(tables_provider: Arc<TablesTableProvider>) -> Self {
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
            tables_provider,
            schema 
        }
    }

    fn scan_all_columns(&self) -> Result<RecordBatch, RegistryError> {
        // TODO: Reimplement after extracting system tables traits
        return Err(RegistryError::ViewError {
            message: "information_schema.columns not yet implemented in kalamdb-registry".to_string(),
        });
        
        /* TODO: Reimplement with trait abstraction
        // Get all table definitions from system.tables
        let all_tables = self.tables_provider.list_tables()?;
        
        // Flatten columns from all tables
        let mut table_catalog_values = StringBuilder::new();
        let mut table_schema_values = StringBuilder::new();
        let mut table_name_values = StringBuilder::new();
        let mut column_name_values = StringBuilder::new();
        let mut ordinal_position_values = Vec::new();
        let mut data_type_values = StringBuilder::new();
        let mut is_nullable_values = Vec::new();
        let mut column_default_values = StringBuilder::new();
        let mut is_primary_key_values = Vec::new();

        for table_def in all_tables {
            // For each column in the table, add a row
            for column in &table_def.columns {
                // table_catalog: Use "def" as default catalog (SQL standard)
                table_catalog_values.append_value("def");
                
                // table_schema is the namespace
                table_schema_values.append_value(table_def.namespace_id.as_str());
                
                // table_name
                table_name_values.append_value(table_def.table_name.as_str());
                
                // column_name
                column_name_values.append_value(&column.column_name);
                
                // ordinal_position (1-indexed)
                ordinal_position_values.push(column.ordinal_position);
                
                // data_type: Convert KalamDataType to SQL standard name
                let data_type_str = format!("{:?}", column.data_type);
                data_type_values.append_value(&data_type_str);
                
                // is_nullable
                is_nullable_values.push(column.is_nullable);
                
                // column_default: Convert ColumnDefault to Option<String>
                match &column.default_value {
                    kalamdb_commons::schemas::ColumnDefault::None => {
                        column_default_values.append_null();
                    },
                    kalamdb_commons::schemas::ColumnDefault::Literal(val) => {
                        column_default_values.append_value(&val.to_string());
                    },
                    kalamdb_commons::schemas::ColumnDefault::FunctionCall { name, .. } => {
                        column_default_values.append_value(&format!("{}()", name));
                    },
                }
                
                // is_primary_key
                is_primary_key_values.push(column.is_primary_key);
            }
        }

        // Create RecordBatch
        let batch = RecordBatch::try_new(
            Arc::clone(&self.schema),
            vec![
                Arc::new(table_catalog_values.finish()) as ArrayRef,
                Arc::new(table_schema_values.finish()) as ArrayRef,
                Arc::new(table_name_values.finish()) as ArrayRef,
                Arc::new(column_name_values.finish()) as ArrayRef,
                Arc::new(UInt32Array::from(ordinal_position_values)) as ArrayRef,
                Arc::new(data_type_values.finish()) as ArrayRef,
                Arc::new(BooleanArray::from(is_nullable_values)) as ArrayRef,
                Arc::new(column_default_values.finish()) as ArrayRef,
                Arc::new(BooleanArray::from(is_primary_key_values)) as ArrayRef,
            ],
        )
        .map_err(|e| RegistryError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
        */
    }
}

impl VirtualView for InformationSchemaColumnsView {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn compute_batch(&self) -> Result<RecordBatch, RegistryError> {
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
    tables_provider: Arc<TablesTableProvider>,
) -> Arc<dyn datafusion::datasource::TableProvider> {
    use super::super::ViewTableProvider;
    let view = Arc::new(InformationSchemaColumnsView::new(tables_provider));
    Arc::new(ViewTableProvider::new(view))
}
