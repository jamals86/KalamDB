//! system.table_schemas table schema definition

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;

/// Schema definition for system.table_schemas
pub struct TableSchemasTable;

impl TableSchemasTable {
    /// Arrow schema for system.table_schemas
    pub fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("schema_id", DataType::Utf8, false),
            Field::new("table_id", DataType::Utf8, false),
            Field::new("namespace_id", DataType::Utf8, false),
            Field::new("table_name", DataType::Utf8, false),
            Field::new("version", DataType::Int32, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("changes", DataType::Utf8, true),
            Field::new("arrow_schema", DataType::Utf8, false),
        ]))
    }

    /// Table name for registration
    pub fn table_name() -> &'static str {
        "table_schemas"
    }

    /// Backing column family name
    pub fn column_family_name() -> &'static str {
        "system_table_schemas"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_schemas_schema() {
        let schema = TableSchemasTable::schema();
        assert_eq!(schema.fields().len(), 8);
        assert_eq!(schema.field(0).name(), "schema_id");
        assert_eq!(schema.field(1).name(), "table_id");
        assert_eq!(schema.field(2).name(), "namespace_id");
        assert_eq!(schema.field(3).name(), "table_name");
        assert_eq!(schema.field(4).name(), "version");
        assert_eq!(schema.field(5).name(), "created_at");
        assert_eq!(schema.field(6).name(), "changes");
        assert_eq!(schema.field(7).name(), "arrow_schema");
    }
}
