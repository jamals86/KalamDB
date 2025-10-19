//! CREATE STREAM TABLE SQL parser
//!
//! Parses CREATE STREAM TABLE statements with schema, retention period,
//! ephemeral mode, and max buffer size.

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::sql::sqlparser::ast::{ColumnDef, DataType as SQLDataType, Statement};
use datafusion::sql::sqlparser::dialect::GenericDialect;
use datafusion::sql::sqlparser::parser::Parser;
use std::sync::Arc;

/// CREATE STREAM TABLE statement
#[derive(Debug, Clone, PartialEq)]
pub struct CreateStreamTableStatement {
    /// Table name (without namespace prefix)
    pub table_name: TableName,
    /// Namespace ID
    pub namespace_id: NamespaceId,
    /// Arrow schema for the table
    pub schema: Arc<Schema>,
    /// Retention period in seconds (TTL)
    pub retention_seconds: Option<u32>,
    /// Ephemeral mode - only store if subscribers exist
    pub ephemeral: bool,
    /// Maximum buffer size (number of events to keep)
    pub max_buffer: Option<usize>,
    /// If true, don't error if table already exists
    pub if_not_exists: bool,
}

impl CreateStreamTableStatement {
    /// Parse a CREATE STREAM TABLE statement from SQL
    pub fn parse(sql: &str, current_namespace: &NamespaceId) -> Result<Self, KalamDbError> {
        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, sql)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        if statements.is_empty() {
            return Err(KalamDbError::InvalidSql(
                "No SQL statement found".to_string(),
            ));
        }

        let stmt = &statements[0];
        Self::parse_statement(stmt, current_namespace)
    }

    /// Parse the CREATE TABLE statement
    fn parse_statement(
        stmt: &Statement,
        current_namespace: &NamespaceId,
    ) -> Result<Self, KalamDbError> {
        match stmt {
            Statement::CreateTable {
                name,
                columns,
                if_not_exists,
                ..
            } => {
                // Extract table name
                let table_name = Self::extract_table_name(name)?;

                // Parse schema from columns
                let schema = Self::parse_schema(columns)?;

                // TODO: Parse WITH options for RETENTION, EPHEMERAL, MAX_BUFFER
                // For now, return basic statement with defaults

                Ok(CreateStreamTableStatement {
                    table_name: TableName::new(table_name),
                    namespace_id: current_namespace.clone(),
                    schema,
                    retention_seconds: None,
                    ephemeral: false,
                    max_buffer: None,
                    if_not_exists: *if_not_exists,
                })
            }
            _ => Err(KalamDbError::InvalidSql(
                "Expected CREATE TABLE statement".to_string(),
            )),
        }
    }

    /// Extract table name from object name
    fn extract_table_name(
        name: &datafusion::sql::sqlparser::ast::ObjectName,
    ) -> Result<String, KalamDbError> {
        let parts = &name.0;
        if parts.is_empty() {
            return Err(KalamDbError::InvalidSql("Empty table name".to_string()));
        }

        // Take the last part as table name (handles both "table" and "namespace.table")
        Ok(parts.last().unwrap().value.clone())
    }

    /// Parse schema from column definitions
    fn parse_schema(columns: &[ColumnDef]) -> Result<Arc<Schema>, KalamDbError> {
        let fields: Result<Vec<Field>, KalamDbError> = columns
            .iter()
            .map(|col| {
                let data_type = Self::convert_sql_type(&col.data_type)?;
                Ok(Field::new(
                    col.name.value.clone(),
                    data_type,
                    col.options.iter().any(|opt| {
                        matches!(
                            opt.option,
                            datafusion::sql::sqlparser::ast::ColumnOption::Null
                        )
                    }),
                ))
            })
            .collect();

        Ok(Arc::new(Schema::new(fields?)))
    }

    /// Convert SQL data type to Arrow data type
    fn convert_sql_type(sql_type: &SQLDataType) -> Result<DataType, KalamDbError> {
        match sql_type {
            SQLDataType::BigInt(_) | SQLDataType::Int8(_) => Ok(DataType::Int64),
            SQLDataType::Int(_) | SQLDataType::Integer(_) | SQLDataType::Int4(_) => {
                Ok(DataType::Int32)
            }
            SQLDataType::SmallInt(_) | SQLDataType::Int2(_) => Ok(DataType::Int16),
            SQLDataType::Boolean => Ok(DataType::Boolean),
            SQLDataType::Text | SQLDataType::String(_) | SQLDataType::Varchar(_) => {
                Ok(DataType::Utf8)
            }
            SQLDataType::Float(_) | SQLDataType::Real => Ok(DataType::Float32),
            SQLDataType::Double | SQLDataType::DoublePrecision => Ok(DataType::Float64),
            SQLDataType::Timestamp(_, _) => Ok(DataType::Timestamp(
                datafusion::arrow::datatypes::TimeUnit::Millisecond,
                None,
            )),
            SQLDataType::Date => Ok(DataType::Date32),
            SQLDataType::Binary(_) | SQLDataType::Bytea => Ok(DataType::Binary),
            _ => Err(KalamDbError::InvalidSql(format!(
                "Unsupported data type: {:?}",
                sql_type
            ))),
        }
    }

    /// Validate the table name follows naming conventions
    pub fn validate_table_name(&self) -> Result<(), KalamDbError> {
        let name = self.table_name.as_str();

        // Must start with lowercase letter
        if !name
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(false)
        {
            return Err(KalamDbError::InvalidOperation(
                "Table name must start with a lowercase letter".to_string(),
            ));
        }

        // Can only contain lowercase letters, digits, and underscores
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(KalamDbError::InvalidOperation(
                "Table name can only contain lowercase letters, digits, and underscores"
                    .to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_create_stream_table() {
        let sql = "CREATE TABLE events (id BIGINT, event_type TEXT, timestamp TIMESTAMP)";
        let namespace = NamespaceId::new("app");

        let result = CreateStreamTableStatement::parse(sql, &namespace);
        assert!(result.is_ok());

        let stmt = result.unwrap();
        assert_eq!(stmt.table_name.as_str(), "events");
        assert_eq!(stmt.namespace_id, namespace);
        assert_eq!(stmt.schema.fields().len(), 3);
        assert_eq!(stmt.ephemeral, false); // Default
        assert!(stmt.retention_seconds.is_none()); // Default
        assert!(stmt.max_buffer.is_none()); // Default
    }

    #[test]
    fn test_parse_with_if_not_exists() {
        let sql = "CREATE TABLE IF NOT EXISTS events (id BIGINT, name TEXT)";
        let namespace = NamespaceId::new("app");

        let result = CreateStreamTableStatement::parse(sql, &namespace);
        assert!(result.is_ok());

        let stmt = result.unwrap();
        assert!(stmt.if_not_exists);
    }

    #[test]
    fn test_validate_table_name() {
        let namespace = NamespaceId::new("app");

        // Valid names
        let valid = CreateStreamTableStatement {
            table_name: TableName::new("events"),
            namespace_id: namespace.clone(),
            schema: Arc::new(Schema::empty()),
            retention_seconds: None,
            ephemeral: false,
            max_buffer: None,
            if_not_exists: false,
        };
        assert!(valid.validate_table_name().is_ok());

        // Invalid: starts with uppercase
        let invalid = CreateStreamTableStatement {
            table_name: TableName::new("Events"),
            namespace_id: namespace.clone(),
            schema: Arc::new(Schema::empty()),
            retention_seconds: None,
            ephemeral: false,
            max_buffer: None,
            if_not_exists: false,
        };
        assert!(invalid.validate_table_name().is_err());

        // Invalid: contains special characters
        let invalid2 = CreateStreamTableStatement {
            table_name: TableName::new("my-events"),
            namespace_id: namespace.clone(),
            schema: Arc::new(Schema::empty()),
            retention_seconds: None,
            ephemeral: false,
            max_buffer: None,
            if_not_exists: false,
        };
        assert!(invalid2.validate_table_name().is_err());
    }

    #[test]
    fn test_convert_sql_types() {
        assert_eq!(
            CreateStreamTableStatement::convert_sql_type(&SQLDataType::BigInt(None)).unwrap(),
            DataType::Int64
        );
        assert_eq!(
            CreateStreamTableStatement::convert_sql_type(&SQLDataType::Text).unwrap(),
            DataType::Utf8
        );
        assert_eq!(
            CreateStreamTableStatement::convert_sql_type(&SQLDataType::Boolean).unwrap(),
            DataType::Boolean
        );
    }

    #[test]
    fn test_parse_schema() {
        let columns = vec![
            ColumnDef {
                name: datafusion::sql::sqlparser::ast::Ident::new("id"),
                data_type: SQLDataType::BigInt(None),
                collation: None,
                options: vec![],
            },
            ColumnDef {
                name: datafusion::sql::sqlparser::ast::Ident::new("event_type"),
                data_type: SQLDataType::Text,
                collation: None,
                options: vec![],
            },
        ];

        let schema = CreateStreamTableStatement::parse_schema(&columns).unwrap();
        assert_eq!(schema.fields().len(), 2);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(0).data_type(), &DataType::Int64);
        assert_eq!(schema.field(1).name(), "event_type");
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
    }

    #[test]
    fn test_stream_table_no_system_columns() {
        // Stream tables should not have _updated or _deleted columns
        let sql = "CREATE TABLE events (id BIGINT, data TEXT)";
        let namespace = NamespaceId::new("app");

        let result = CreateStreamTableStatement::parse(sql, &namespace);
        assert!(result.is_ok());

        let stmt = result.unwrap();
        // Verify schema only has user-defined columns
        assert_eq!(stmt.schema.fields().len(), 2);
        assert_eq!(stmt.schema.field(0).name(), "id");
        assert_eq!(stmt.schema.field(1).name(), "data");
        // No _updated or _deleted fields
        assert!(stmt.schema.field_with_name("_updated").is_err());
        assert!(stmt.schema.field_with_name("_deleted").is_err());
    }
}
