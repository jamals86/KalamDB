//! CREATE SHARED TABLE SQL parser
//!
//! Parses CREATE SHARED TABLE statements with schema, location, flush policy,
//! and deleted retention configuration.

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::sql::sqlparser::ast::{ColumnDef, DataType as SQLDataType, Statement};
use datafusion::sql::sqlparser::dialect::GenericDialect;
use datafusion::sql::sqlparser::parser::Parser;
use std::sync::Arc;

/// Flush policy for shared tables
#[derive(Debug, Clone, PartialEq)]
pub enum FlushPolicy {
    /// Flush after N rows
    Rows(usize),
    /// Flush after N seconds
    Time(u64),
    /// Flush when either condition is met
    Combined { rows: usize, seconds: u64 },
}

/// CREATE SHARED TABLE statement
#[derive(Debug, Clone, PartialEq)]
pub struct CreateSharedTableStatement {
    /// Table name (without namespace prefix)
    pub table_name: TableName,
    /// Namespace ID
    pub namespace_id: NamespaceId,
    /// Arrow schema for the table (will include system columns: _updated, _deleted)
    pub schema: Arc<Schema>,
    /// Storage location (path or location reference)
    pub location: Option<String>,
    /// Flush policy for RocksDB → Parquet
    pub flush_policy: Option<FlushPolicy>,
    /// Retention period for soft-deleted rows (in seconds)
    pub deleted_retention: Option<u64>,
    /// If true, don't error if table already exists
    pub if_not_exists: bool,
}

impl CreateSharedTableStatement {
    /// Parse a CREATE SHARED TABLE statement from SQL
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

                // TODO: Parse WITH options for LOCATION, FLUSH POLICY, DELETED_RETENTION
                // For now, return basic statement with defaults

                Ok(CreateSharedTableStatement {
                    table_name: TableName::new(table_name),
                    namespace_id: current_namespace.clone(),
                    schema,
                    location: None,
                    flush_policy: None,
                    deleted_retention: None,
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
        if columns.is_empty() {
            return Err(KalamDbError::InvalidSql(
                "Table must have at least one column".to_string(),
            ));
        }

        let mut fields = Vec::new();

        for col in columns {
            let field_name = col.name.value.clone();
            let data_type = Self::map_sql_type_to_arrow(&col.data_type)?;
            let nullable = col.options.iter().any(|opt| {
                matches!(
                    opt.option,
                    datafusion::sql::sqlparser::ast::ColumnOption::Null
                )
            }) || !col.options.iter().any(|opt| {
                matches!(
                    opt.option,
                    datafusion::sql::sqlparser::ast::ColumnOption::NotNull
                )
            });

            fields.push(Field::new(field_name, data_type, nullable));
        }

        Ok(Arc::new(Schema::new(fields)))
    }

    /// Map SQL data type to Arrow data type
    fn map_sql_type_to_arrow(sql_type: &SQLDataType) -> Result<DataType, KalamDbError> {
        match sql_type {
            SQLDataType::BigInt(_) => Ok(DataType::Int64),
            SQLDataType::Int(_) | SQLDataType::Integer(_) => Ok(DataType::Int32),
            SQLDataType::SmallInt(_) => Ok(DataType::Int16),
            SQLDataType::TinyInt(_) => Ok(DataType::Int8),
            SQLDataType::UnsignedBigInt(_) => Ok(DataType::UInt64),
            SQLDataType::UnsignedInt(_) | SQLDataType::UnsignedInteger(_) => Ok(DataType::UInt32),
            SQLDataType::UnsignedSmallInt(_) => Ok(DataType::UInt16),
            SQLDataType::UnsignedTinyInt(_) => Ok(DataType::UInt8),
            SQLDataType::Float(_) => Ok(DataType::Float32),
            SQLDataType::Double | SQLDataType::DoublePrecision => Ok(DataType::Float64),
            SQLDataType::Boolean => Ok(DataType::Boolean),
            SQLDataType::Text | SQLDataType::String(_) => Ok(DataType::Utf8),
            SQLDataType::Varchar(_) | SQLDataType::Char(_) | SQLDataType::CharVarying(_) => {
                Ok(DataType::Utf8)
            }
            SQLDataType::Timestamp(_, _) => Ok(DataType::Timestamp(
                datafusion::arrow::datatypes::TimeUnit::Millisecond,
                None,
            )),
            SQLDataType::Date => Ok(DataType::Date32),
            SQLDataType::Binary(_) | SQLDataType::Varbinary(_) | SQLDataType::Blob(_) => {
                Ok(DataType::Binary)
            }
            SQLDataType::JSON => Ok(DataType::Utf8), // Store JSON as string
            _ => Err(KalamDbError::InvalidSql(format!(
                "Unsupported data type: {:?}",
                sql_type
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_namespace() -> NamespaceId {
        NamespaceId::new("test_namespace".to_string())
    }

    #[test]
    fn test_parse_basic_shared_table() {
        let sql = "CREATE TABLE config (id INT, value TEXT)";
        let stmt = CreateSharedTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.table_name.as_str(), "config");
        assert_eq!(stmt.namespace_id.as_str(), "test_namespace");
        assert_eq!(stmt.schema.fields().len(), 2);
        assert_eq!(stmt.schema.field(0).name(), "id");
        assert_eq!(stmt.schema.field(1).name(), "value");
        assert_eq!(stmt.location, None);
        assert_eq!(stmt.flush_policy, None);
        assert_eq!(stmt.deleted_retention, None);
        assert!(!stmt.if_not_exists);
    }

    #[test]
    fn test_parse_shared_table_if_not_exists() {
        let sql = "CREATE TABLE IF NOT EXISTS settings (id INT, value TEXT)";
        let stmt = CreateSharedTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.table_name.as_str(), "settings");
        assert!(stmt.if_not_exists);
    }

    #[test]
    fn test_parse_shared_table_various_types() {
        let sql = r#"
            CREATE TABLE metrics (
                id BIGINT,
                name TEXT,
                value DOUBLE,
                active BOOLEAN,
                created_at TIMESTAMP,
                data BLOB
            )
        "#;
        let stmt = CreateSharedTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.schema.fields().len(), 6);
        assert_eq!(stmt.schema.field(0).data_type(), &DataType::Int64);
        assert_eq!(stmt.schema.field(1).data_type(), &DataType::Utf8);
        assert_eq!(stmt.schema.field(2).data_type(), &DataType::Float64);
        assert_eq!(stmt.schema.field(3).data_type(), &DataType::Boolean);
        assert_eq!(
            stmt.schema.field(4).data_type(),
            &DataType::Timestamp(datafusion::arrow::datatypes::TimeUnit::Millisecond, None)
        );
        assert_eq!(stmt.schema.field(5).data_type(), &DataType::Binary);
    }

    #[test]
    fn test_parse_shared_table_not_null() {
        let sql = "CREATE TABLE users (id INT NOT NULL, name TEXT NOT NULL, email TEXT)";
        let stmt = CreateSharedTableStatement::parse(sql, &test_namespace()).unwrap();

        assert!(!stmt.schema.field(0).is_nullable()); // id NOT NULL
        assert!(!stmt.schema.field(1).is_nullable()); // name NOT NULL
        assert!(stmt.schema.field(2).is_nullable()); // email nullable
    }

    #[test]
    fn test_parse_empty_columns() {
        let sql = "CREATE TABLE empty ()";
        let result = CreateSharedTableStatement::parse(sql, &test_namespace());

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least one column"));
    }

    #[test]
    fn test_parse_qualified_table_name() {
        let sql = "CREATE TABLE my_namespace.global_config (setting TEXT)";
        let stmt = CreateSharedTableStatement::parse(sql, &test_namespace()).unwrap();

        // Should extract just the table name part
        assert_eq!(stmt.table_name.as_str(), "global_config");
    }

    #[test]
    fn test_parse_invalid_sql() {
        let sql = "CREATE TABLE";
        let result = CreateSharedTableStatement::parse(sql, &test_namespace());

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_various_integer_types() {
        let sql = r#"
            CREATE TABLE integers (
                tiny TINYINT,
                small SMALLINT,
                normal INT,
                big BIGINT,
                utiny TINYINT UNSIGNED,
                usmall SMALLINT UNSIGNED,
                unormal INT UNSIGNED,
                ubig BIGINT UNSIGNED
            )
        "#;
        let stmt = CreateSharedTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.schema.field(0).data_type(), &DataType::Int8);
        assert_eq!(stmt.schema.field(1).data_type(), &DataType::Int16);
        assert_eq!(stmt.schema.field(2).data_type(), &DataType::Int32);
        assert_eq!(stmt.schema.field(3).data_type(), &DataType::Int64);
        assert_eq!(stmt.schema.field(4).data_type(), &DataType::UInt8);
        assert_eq!(stmt.schema.field(5).data_type(), &DataType::UInt16);
        assert_eq!(stmt.schema.field(6).data_type(), &DataType::UInt32);
        assert_eq!(stmt.schema.field(7).data_type(), &DataType::UInt64);
    }
}
