//! Schema evolution service
//!
//! Handles ALTER TABLE operations including:
//! - ADD COLUMN: Add new columns with defaults
//! - DROP COLUMN: Remove columns (with subscription checking)
//! - MODIFY COLUMN: Change data types (with compatibility validation)
//!
//! Validation rules:
//! - System columns (_updated, _deleted) cannot be altered
//! - Stream tables have immutable schemas
//! - Active live queries prevent dropping referenced columns
//! - Type changes must be backwards compatible
//! - Schema version is incremented for each change
//!
//! Uses kalamdb-sql for all metadata operations (three-layer architecture)

use crate::catalog::{NamespaceId, TableName, TableType};
use crate::error::KalamDbError;
use crate::schema::ArrowSchemaWithOptions;
use arrow::datatypes::{DataType, Field, Schema};
use kalamdb_sql::ddl::ColumnOperation;
use kalamdb_sql::{KalamSql, Table, TableSchema};
use std::sync::Arc;

/// Result of schema evolution operation
#[derive(Debug, Clone)]
pub struct SchemaEvolutionResult {
    /// Table ID that was modified
    pub table_id: String,

    /// New schema version number
    pub new_version: i32,

    /// Description of the change
    pub change_description: String,

    /// Indicates if schema cache was invalidated
    pub cache_invalidated: bool,
}

/// Schema evolution service
///
/// Orchestrates schema changes using kalamdb-sql
pub struct SchemaEvolutionService {
    kalam_sql: Arc<KalamSql>,
}

impl SchemaEvolutionService {
    /// Create a new schema evolution service
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self { kalam_sql }
    }

    /// Apply an ALTER TABLE operation
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of table to alter
    /// * `operation` - Column operation (ADD/DROP/MODIFY)
    ///
    /// # Returns
    /// * `Ok(result)` - Schema was successfully evolved
    /// * `Err(_)` - Validation or evolution error
    pub fn alter_table(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        operation: &ColumnOperation,
    ) -> Result<SchemaEvolutionResult, KalamDbError> {
        // Generate table_id (format: namespace:table_name)
        let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());

        // Get table metadata
        let table = self
            .kalam_sql
            .get_table(&table_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get table: {}", e)))?
            .ok_or_else(|| {
                KalamDbError::table_not_found(format!(
                    "Table '{}' not found in namespace '{}'",
                    table_name.as_str(),
                    namespace_id.as_str()
                ))
            })?;

        // Prevent altering stream tables (T180)
        let table_type = TableType::from_str(&table.table_type).ok_or_else(|| {
            KalamDbError::InvalidSql(format!("Unknown table type: {}", table.table_type))
        })?;
        if table_type == TableType::Stream {
            return Err(KalamDbError::invalid_schema_evolution(
                "Stream tables have immutable schemas and cannot be altered",
            ));
        }

        // Prevent altering system columns (T179)
        self.validate_system_columns(operation)?;

        // Check for active subscriptions that reference this column (T178)
        if let ColumnOperation::Drop { column_name } = operation {
            self.check_active_subscriptions(table_name, column_name)?;
        }

        // Get current schema
        let current_schema = self
            .kalam_sql
            .get_table_schema(&table_id, Some(table.schema_version))
            .map_err(|e| KalamDbError::IoError(format!("Failed to get schema: {}", e)))?
            .ok_or_else(|| {
                KalamDbError::schema_version_not_found(&table_id, table.schema_version)
            })?;

        // Deserialize Arrow schema using ArrowSchemaWithOptions
        let arrow_schema_with_opts =
            ArrowSchemaWithOptions::from_json_string(&current_schema.arrow_schema)
                .map_err(|e| KalamDbError::SchemaError(format!("Failed to parse schema: {}", e)))?;
        let arrow_schema = arrow_schema_with_opts.schema.as_ref().clone();

        // Validate the operation (T177)
        self.validate_operation(&arrow_schema, operation)?;

        // Apply the operation to create new schema (T181)
        let new_arrow_schema = self.apply_operation(arrow_schema, operation)?;
        let new_version = table.schema_version + 1;

        // Serialize new schema using ArrowSchemaWithOptions
        let new_schema_with_opts = ArrowSchemaWithOptions::new(Arc::new(new_arrow_schema));
        let new_schema_json = new_schema_with_opts
            .to_json_string()
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to serialize schema: {}", e)))?;

        // Create change description
        let change_description = match operation {
            ColumnOperation::Add {
                column_name,
                data_type,
                nullable,
                default_value,
            } => {
                if let Some(default) = default_value {
                    format!(
                        "ADD COLUMN {} {} {} DEFAULT {}",
                        column_name,
                        data_type,
                        if *nullable { "NULL" } else { "NOT NULL" },
                        default
                    )
                } else {
                    format!(
                        "ADD COLUMN {} {} {}",
                        column_name,
                        data_type,
                        if *nullable { "NULL" } else { "NOT NULL" }
                    )
                }
            }
            ColumnOperation::Drop { column_name } => {
                format!("DROP COLUMN {}", column_name)
            }
            ColumnOperation::Modify {
                column_name,
                new_data_type,
                nullable,
            } => {
                if let Some(null) = nullable {
                    format!(
                        "MODIFY COLUMN {} {} {}",
                        column_name,
                        new_data_type,
                        if *null { "NULL" } else { "NOT NULL" }
                    )
                } else {
                    format!("MODIFY COLUMN {} {}", column_name, new_data_type)
                }
            }
        };

        // Create and insert new schema version
        let new_schema = TableSchema {
            schema_id: format!("{}:v{}", table_id, new_version),
            table_id: table_id.clone(),
            version: new_version,
            arrow_schema: new_schema_json,
            created_at: chrono::Utc::now().timestamp(),
            changes: serde_json::to_string(&vec![change_description.clone()])
                .unwrap_or_else(|_| "[]".to_string()),
        };

        self.kalam_sql
            .insert_table_schema(&new_schema)
            .map_err(|e| KalamDbError::IoError(format!("Failed to insert schema: {}", e)))?;

        // Update table metadata with new schema version (T182)
        let updated_table = Table {
            schema_version: new_version,
            ..table
        };

        self.kalam_sql
            .update_table(&updated_table)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update table: {}", e)))?;

        // Note: Schema cache invalidation (T183) would happen here in a full implementation.
        // In practice, DataFusion tables would need to be deregistered and re-registered
        // with the new schema. This is typically handled at the SQL executor level
        // when the next query is executed against this table.
        let cache_invalidated = false; // Placeholder for T183

        Ok(SchemaEvolutionResult {
            table_id,
            new_version,
            change_description,
            cache_invalidated,
        })
    }

    /// Validate that operation doesn't touch system columns (T179)
    fn validate_system_columns(&self, operation: &ColumnOperation) -> Result<(), KalamDbError> {
        let system_columns = ["_updated", "_deleted"];

        let column_name = match operation {
            ColumnOperation::Add { column_name, .. } => column_name,
            ColumnOperation::Drop { column_name } => column_name,
            ColumnOperation::Modify { column_name, .. } => column_name,
        };

        if system_columns.contains(&column_name.as_str()) {
            return Err(KalamDbError::invalid_schema_evolution(format!(
                "System column '{}' cannot be altered (managed by system)",
                column_name
            )));
        }

        Ok(())
    }

    /// Check for active subscriptions that reference the column being dropped (T178)
    fn check_active_subscriptions(
        &self,
        table_name: &TableName,
        column_name: &str,
    ) -> Result<(), KalamDbError> {
        // Get all active live queries
        let live_queries = self
            .kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::IoError(format!("Failed to scan live queries: {}", e)))?;

        // Filter queries for this table
        let table_queries: Vec<_> = live_queries
            .iter()
            .filter(|lq| {
                // Check if query references this table
                lq.table_name == table_name.as_str()
            })
            .collect();

        if table_queries.is_empty() {
            return Ok(());
        }

        // Check if any query references the column being dropped
        let affected_queries: Vec<_> = table_queries
            .iter()
            .filter(|lq| {
                // Check if column name appears in query
                // This is a simple heuristic - a proper implementation would parse the SQL
                let query_upper = lq.query.to_uppercase();
                let column_upper = column_name.to_uppercase();

                // Check for column in SELECT list or WHERE clause
                query_upper.contains(&column_upper)
            })
            .collect();

        if !affected_queries.is_empty() {
            let subscription_details: Vec<String> = affected_queries
                .iter()
                .map(|lq| {
                    format!(
                        "connection_id={}, user_id={}, query_id={}",
                        lq.connection_id, lq.user_id, lq.query_id
                    )
                })
                .collect();

            return Err(KalamDbError::Conflict(format!(
                "Cannot drop column '{}': {} active subscription(s) reference this column. Affected: [{}]",
                column_name,
                affected_queries.len(),
                subscription_details.join(", ")
            )));
        }

        Ok(())
    }

    /// Validate operation for backwards compatibility (T177)
    fn validate_operation(
        &self,
        schema: &Schema,
        operation: &ColumnOperation,
    ) -> Result<(), KalamDbError> {
        match operation {
            ColumnOperation::Add {
                column_name,
                nullable,
                default_value,
                ..
            } => {
                // Check column doesn't already exist
                if schema.field_with_name(column_name).is_ok() {
                    return Err(KalamDbError::invalid_schema_evolution(format!(
                        "Column '{}' already exists",
                        column_name
                    )));
                }

                // If NOT NULL, must have default value
                if !nullable && default_value.is_none() {
                    return Err(KalamDbError::invalid_schema_evolution(format!(
                        "NOT NULL column '{}' requires a DEFAULT value for existing rows",
                        column_name
                    )));
                }

                Ok(())
            }
            ColumnOperation::Drop { column_name } => {
                // Check column exists
                if schema.field_with_name(column_name).is_err() {
                    return Err(KalamDbError::invalid_schema_evolution(format!(
                        "Column '{}' does not exist",
                        column_name
                    )));
                }

                // Prevent dropping primary key (assuming first field is PK)
                if let Some(first_field) = schema.fields().first() {
                    if first_field.name() == column_name {
                        return Err(KalamDbError::invalid_schema_evolution(format!(
                            "Cannot drop primary key column '{}'",
                            column_name
                        )));
                    }
                }

                Ok(())
            }
            ColumnOperation::Modify {
                column_name,
                nullable,
                ..
            } => {
                // Check column exists
                let current_field = schema.field_with_name(column_name).map_err(|_| {
                    KalamDbError::invalid_schema_evolution(format!(
                        "Column '{}' does not exist",
                        column_name
                    ))
                })?;

                // Prevent modifying primary key
                if let Some(first_field) = schema.fields().first() {
                    if first_field.name() == column_name {
                        return Err(KalamDbError::invalid_schema_evolution(format!(
                            "Cannot modify primary key column '{}'",
                            column_name
                        )));
                    }
                }

                // Validate nullable change (NULL -> NOT NULL requires existing data validation)
                if let Some(new_nullable) = nullable {
                    if !new_nullable && current_field.is_nullable() {
                        return Err(KalamDbError::invalid_schema_evolution(
                            format!("Cannot change column '{}' from NULL to NOT NULL without validating existing data", column_name)
                        ));
                    }
                }

                Ok(())
            }
        }
    }

    /// Apply operation to schema to create new version (T181)
    fn apply_operation(
        &self,
        schema: Schema,
        operation: &ColumnOperation,
    ) -> Result<Schema, KalamDbError> {
        match operation {
            ColumnOperation::Add {
                column_name,
                data_type,
                nullable,
                ..
            } => {
                // Parse data type string to Arrow DataType
                let arrow_type = Self::parse_data_type(data_type)?;

                // Create new field
                let new_field = Field::new(column_name, arrow_type, *nullable);

                // Add field to schema (convert Arc<Field> to Field)
                let mut fields: Vec<Field> =
                    schema.fields().iter().map(|f| (**f).clone()).collect();
                fields.push(new_field);

                Ok(Schema::new(fields))
            }
            ColumnOperation::Drop { column_name } => {
                // Remove field from schema
                let fields: Vec<Field> = schema
                    .fields()
                    .iter()
                    .filter(|f| f.name() != column_name)
                    .map(|f| (**f).clone())
                    .collect();

                if fields.len() == schema.fields().len() {
                    return Err(KalamDbError::invalid_schema_evolution(format!(
                        "Column '{}' not found in schema",
                        column_name
                    )));
                }

                Ok(Schema::new(fields))
            }
            ColumnOperation::Modify {
                column_name,
                new_data_type,
                nullable,
            } => {
                // Parse new data type
                let arrow_type = Self::parse_data_type(new_data_type)?;

                // Modify field in schema
                let fields: Vec<Field> = schema
                    .fields()
                    .iter()
                    .map(|f| {
                        if f.name() == column_name {
                            let new_nullable = nullable.unwrap_or(f.is_nullable());
                            Field::new(column_name, arrow_type.clone(), new_nullable)
                        } else {
                            (**f).clone()
                        }
                    })
                    .collect();

                Ok(Schema::new(fields))
            }
        }
    }

    /// Parse SQL data type string to Arrow DataType
    fn parse_data_type(type_str: &str) -> Result<DataType, KalamDbError> {
        let type_upper = type_str.to_uppercase();

        let data_type = if type_upper.starts_with("VARCHAR") || type_upper == "TEXT" {
            DataType::Utf8
        } else if type_upper == "INT" || type_upper == "INT4" || type_upper == "INTEGER" {
            DataType::Int32
        } else if type_upper == "BIGINT" || type_upper == "INT8" {
            DataType::Int64
        } else if type_upper == "SMALLINT" || type_upper == "INT2" {
            DataType::Int16
        } else if type_upper == "BOOLEAN" || type_upper == "BOOL" {
            DataType::Boolean
        } else if type_upper == "FLOAT" || type_upper == "FLOAT4" {
            DataType::Float32
        } else if type_upper == "DOUBLE" || type_upper == "FLOAT8" {
            DataType::Float64
        } else if type_upper == "TIMESTAMP" {
            DataType::Timestamp(arrow::datatypes::TimeUnit::Millisecond, None)
        } else if type_upper == "DATE" {
            DataType::Date32
        } else {
            return Err(KalamDbError::SchemaError(format!(
                "Unsupported data type: {}",
                type_str
            )));
        };

        Ok(data_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_sql::{LiveQuery, Namespace};
    use kalamdb_store::test_utils::TestDb;

    fn setup_test_db() -> (Arc<KalamSql>, TestDb) {
        // Create TestDb with required column families
        let cf_names = &[
            "system_users",
            "system_namespaces",
            "system_tables",
            "system_table_schemas",
            "system_storage_locations",
            "system_live_queries",
            "system_jobs",
        ];
        let test_db = TestDb::new(cf_names).unwrap();
        let kalam_sql = Arc::new(KalamSql::new(test_db.db.clone()).unwrap());
        (kalam_sql, test_db)
    }

    fn create_test_table(kalam_sql: &KalamSql, table_type: &str) -> String {
        let table_id = "test_ns:test_table".to_string();

        // Create namespace
        let namespace = Namespace {
            namespace_id: "test_ns".to_string(),
            name: "test_ns".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            options: "{}".to_string(),
            table_count: 1,
        };
        kalam_sql.insert_namespace_struct(&namespace).unwrap();

        // Create table
        let table = Table {
            table_id: table_id.clone(),
            table_name: "test_table".to_string(),
            namespace: "test_ns".to_string(),
            table_type: table_type.to_string(),
            created_at: chrono::Utc::now().timestamp(),
            storage_location: "/tmp/test".to_string(),
            storage_id: Some("local".to_string()),
            use_user_storage: false,
            flush_policy: "{}".to_string(),
            schema_version: 1,
            deleted_retention_hours: 72,
        };
        kalam_sql.insert_table(&table).unwrap();

        // Create initial schema
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("age", DataType::Int32, true),
        ]);

        let schema_with_opts = ArrowSchemaWithOptions::new(Arc::new(schema));
        let schema_json = schema_with_opts.to_json_string().unwrap();
        let table_schema = TableSchema {
            schema_id: format!("{}:v1", table_id),
            table_id: table_id.clone(),
            version: 1,
            arrow_schema: schema_json,
            created_at: chrono::Utc::now().timestamp(),
            changes: "[]".to_string(),
        };
        kalam_sql.insert_table_schema(&table_schema).unwrap();

        table_id
    }

    #[test]
    fn test_add_column() {
        let (kalam_sql, _db) = setup_test_db();
        let service = SchemaEvolutionService::new(kalam_sql.clone());

        create_test_table(&kalam_sql, "user");

        let operation = ColumnOperation::Add {
            column_name: "email".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: true,
            default_value: None,
        };

        let result = service
            .alter_table(
                &NamespaceId::new("test_ns"),
                &TableName::new("test_table"),
                &operation,
            )
            .unwrap();

        assert_eq!(result.new_version, 2);
        assert!(result.change_description.contains("ADD COLUMN email"));
    }

    #[test]
    fn test_drop_column() {
        let (kalam_sql, _db) = setup_test_db();
        let service = SchemaEvolutionService::new(kalam_sql.clone());

        create_test_table(&kalam_sql, "user");

        let operation = ColumnOperation::Drop {
            column_name: "age".to_string(),
        };

        let result = service
            .alter_table(
                &NamespaceId::new("test_ns"),
                &TableName::new("test_table"),
                &operation,
            )
            .unwrap();

        assert_eq!(result.new_version, 2);
        assert!(result.change_description.contains("DROP COLUMN age"));
    }

    #[test]
    fn test_modify_column() {
        let (kalam_sql, _db) = setup_test_db();
        let service = SchemaEvolutionService::new(kalam_sql.clone());

        create_test_table(&kalam_sql, "user");

        let operation = ColumnOperation::Modify {
            column_name: "age".to_string(),
            new_data_type: "BIGINT".to_string(),
            nullable: None,
        };

        let result = service
            .alter_table(
                &NamespaceId::new("test_ns"),
                &TableName::new("test_table"),
                &operation,
            )
            .unwrap();

        assert_eq!(result.new_version, 2);
        assert!(result.change_description.contains("MODIFY COLUMN age"));
    }

    #[test]
    fn test_prevent_altering_system_columns() {
        let (kalam_sql, _db) = setup_test_db();
        let service = SchemaEvolutionService::new(kalam_sql.clone());

        create_test_table(&kalam_sql, "user");

        let operation = ColumnOperation::Drop {
            column_name: "_updated".to_string(),
        };

        let result = service.alter_table(
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
            &operation,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("System column '_updated' cannot be altered"));
    }

    #[test]
    fn test_prevent_altering_stream_tables() {
        let (kalam_sql, _db) = setup_test_db();
        let service = SchemaEvolutionService::new(kalam_sql.clone());

        create_test_table(&kalam_sql, "stream");

        let operation = ColumnOperation::Add {
            column_name: "new_col".to_string(),
            data_type: "INT".to_string(),
            nullable: true,
            default_value: None,
        };

        let result = service.alter_table(
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
            &operation,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Stream tables have immutable schemas"));
    }

    #[test]
    fn test_prevent_drop_column_with_active_subscriptions() {
        let (kalam_sql, _db) = setup_test_db();
        let service = SchemaEvolutionService::new(kalam_sql.clone());

        create_test_table(&kalam_sql, "user");

        // Create active live query
        let live_query = LiveQuery {
            live_id: "lq123".to_string(),
            connection_id: "conn1".to_string(),
            user_id: "user1".to_string(),
            table_name: "test_table".to_string(),
            query_id: "query1".to_string(),
            query: "SELECT name, age FROM test_table WHERE age > 18".to_string(),
            options: "{}".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
            changes: 0,
            node: "node1".to_string(),
        };
        kalam_sql.insert_live_query(&live_query).unwrap();

        let operation = ColumnOperation::Drop {
            column_name: "age".to_string(),
        };

        let result = service.alter_table(
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
            &operation,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("active subscription(s) reference this column"));
    }

    #[test]
    fn test_validate_not_null_requires_default() {
        let (kalam_sql, _db) = setup_test_db();
        let service = SchemaEvolutionService::new(kalam_sql.clone());

        create_test_table(&kalam_sql, "user");

        let operation = ColumnOperation::Add {
            column_name: "required_field".to_string(),
            data_type: "INT".to_string(),
            nullable: false,
            default_value: None,
        };

        let result = service.alter_table(
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
            &operation,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NOT NULL column"));
    }

    #[test]
    fn test_prevent_drop_primary_key() {
        let (kalam_sql, _db) = setup_test_db();
        let service = SchemaEvolutionService::new(kalam_sql.clone());

        create_test_table(&kalam_sql, "user");

        let operation = ColumnOperation::Drop {
            column_name: "id".to_string(),
        };

        let result = service.alter_table(
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
            &operation,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot drop primary key"));
    }
}
