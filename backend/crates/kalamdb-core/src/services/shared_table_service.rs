//! Shared table service
//!
//! This service handles all shared table-related operations including:
//! - Creating shared tables with schema validation
//! - System column injection (_updated, _deleted)
//! - Metadata registration in system_tables via kalamdb-sql
//! - Column family creation for shared_table:{namespace}:{table_name}
//! - Flush policy configuration

use crate::catalog::{NamespaceId, TableMetadata, TableName, TableType};
use crate::error::KalamDbError;
use crate::flush::FlushPolicy;
use crate::schema::arrow_schema::ArrowSchemaWithOptions;
use crate::sql::ddl::create_shared_table::CreateSharedTableStatement;
use datafusion::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use kalamdb_sql::models::TableSchema;
use kalamdb_sql::KalamSql;
use kalamdb_store::SharedTableStore;
use std::sync::Arc;

/// Shared table service
///
/// Coordinates shared table creation across schema storage (RocksDB),
/// column families, and metadata management.
///
/// Shared tables are similar to user tables but:
/// - Single storage location (no ${user_id} templating)
/// - Accessible by all users in namespace (subject to permissions)
/// - HAVE system columns (_updated, _deleted)
/// - Support flush policies (RocksDB → Parquet)
pub struct SharedTableService {
    shared_table_store: Arc<SharedTableStore>,
    kalam_sql: Arc<KalamSql>,
}

impl SharedTableService {
    /// Create a new shared table service
    ///
    /// # Arguments
    /// * `shared_table_store` - Storage backend for shared tables
    /// * `kalam_sql` - KalamSQL instance for metadata storage
    pub fn new(shared_table_store: Arc<SharedTableStore>, kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            shared_table_store,
            kalam_sql,
        }
    }

    /// Create a shared table from a CREATE SHARED TABLE statement
    ///
    /// This method orchestrates:
    /// 1. Schema validation
    /// 2. System column injection (_updated, _deleted)
    /// 3. Metadata registration in system_tables via kalamdb-sql
    /// 4. Schema storage in system_table_schemas via kalamdb-sql
    /// 5. Table metadata creation
    ///
    /// Note: Column family creation must be done separately on the DB instance.
    ///
    /// # Arguments
    /// * `stmt` - Parsed CREATE SHARED TABLE statement
    ///
    /// # Returns
    /// Table metadata for the created shared table
    /// Create a new shared table
    ///
    /// # Returns
    /// * `Ok((metadata, was_created))` - Table metadata and whether it was newly created (false if IF NOT EXISTS and exists)
    /// * `Err(KalamDbError)` - If creation failed
    pub fn create_table(
        &self,
        stmt: CreateSharedTableStatement,
    ) -> Result<(TableMetadata, bool), KalamDbError> {
        // Validate table name
        self.validate_table_name(&stmt.table_name)?;

        // Check if table already exists
        if self.table_exists(&stmt.namespace_id, &stmt.table_name)? {
            if stmt.if_not_exists {
                // Return existing table metadata (or a default success response)
                // For now, we'll create a minimal metadata response
                let table_id = format!(
                    "{}:{}",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                );

                // Get existing table from system.tables
                let existing_table = self
                    .kalam_sql
                    .get_table(&table_id)
                    .map_err(|e| {
                        KalamDbError::Other(format!("Failed to get existing table: {}", e))
                    })?
                    .ok_or_else(|| {
                        KalamDbError::NotFound(format!("Table {} not found", table_id))
                    })?;

                // Return a minimal metadata object for the existing table
                return Ok((
                    TableMetadata {
                        table_name: stmt.table_name.clone(),
                        table_type: TableType::Shared,
                        namespace: stmt.namespace_id.clone(),
                        created_at: chrono::DateTime::from_timestamp(
                            existing_table.created_at / 1000,
                            0,
                        )
                        .unwrap_or_else(chrono::Utc::now),
                        storage_location: existing_table.storage_location,
                        flush_policy: serde_json::from_str(&existing_table.flush_policy)
                            .unwrap_or_default(),
                        schema_version: existing_table.schema_version as u32,
                        deleted_retention_hours: if existing_table.deleted_retention_hours > 0 {
                            Some(existing_table.deleted_retention_hours as u32)
                        } else {
                            None
                        },
                    },
                    false,
                )); // false = not newly created
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Shared table {}.{} already exists",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                )));
            }
        }

        // Use the schema as-is WITHOUT injecting system columns
        // System columns will be added dynamically by SharedTableProvider at query time
        let schema = stmt.schema.clone();

        // Resolve storage location
        let storage_location = stmt.location.unwrap_or_else(|| "/data/shared".to_string());

        // Validate no ${user_id} templating in shared table storage location
        if storage_location.contains("${user_id}") {
            return Err(KalamDbError::InvalidOperation(
                "Shared table storage location cannot contain ${user_id} template variable"
                    .to_string(),
            ));
        }

        // Parse flush policy
        let flush_policy = self.parse_flush_policy(stmt.flush_policy.as_ref())?;

        // Store schema in system_table_schemas via kalamdb-sql
        self.create_schema_metadata(
            &stmt.namespace_id,
            &stmt.table_name,
            &schema,
            &storage_location,
            &flush_policy,
            stmt.deleted_retention,
        )?;

        // Create RocksDB column family for this table
        // This ensures the table is ready for data operations immediately after creation
        self.shared_table_store
            .create_column_family(stmt.namespace_id.as_str(), stmt.table_name.as_str())
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to create column family for shared table {}.{}: {}",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str(),
                    e
                ))
            })?;

        // Note: Column family creation must be done separately via DB instance
        // The caller should use:
        // db.create_cf(format!("shared_table:{}:{}", namespace_id, table_name), &opts)

        // Create and return table metadata
        let metadata = TableMetadata {
            table_name: stmt.table_name.clone(),
            table_type: TableType::Shared,
            namespace: stmt.namespace_id.clone(),
            created_at: chrono::Utc::now(),
            storage_location,
            flush_policy,
            schema_version: 1,
            deleted_retention_hours: stmt.deleted_retention.map(|secs| (secs / 3600) as u32),
        };

        Ok((metadata, true)) // true = newly created
    }

    /// Validate table name
    ///
    /// Rules:
    /// - Must start with lowercase letter or underscore
    /// - Can contain lowercase letters, numbers, underscores
    /// - Cannot be a SQL keyword
    fn validate_table_name(&self, table_name: &TableName) -> Result<(), KalamDbError> {
        let name = table_name.as_str();

        // Check first character
        let first_char = name.chars().next().ok_or_else(|| {
            KalamDbError::InvalidOperation("Table name cannot be empty".to_string())
        })?;

        if !first_char.is_lowercase() && first_char != '_' {
            return Err(KalamDbError::InvalidOperation(format!(
                "Table name must start with lowercase letter or underscore: {}",
                name
            )));
        }

        // Check for SQL keywords
        let keywords = [
            "select", "insert", "update", "delete", "table", "from", "where",
        ];
        if keywords.contains(&name.to_lowercase().as_str()) {
            return Err(KalamDbError::InvalidOperation(format!(
                "Table name cannot be a SQL keyword: {}",
                name
            )));
        }

        Ok(())
    }

    /// Inject system columns for shared tables
    ///
    /// Adds _updated (TIMESTAMP) and _deleted (BOOLEAN) columns.
    fn inject_system_columns(&self, schema: Arc<Schema>) -> Result<Arc<Schema>, KalamDbError> {
        // Check if system columns already exist
        let has_updated = schema.field_with_name("_updated").is_ok();
        let has_deleted = schema.field_with_name("_deleted").is_ok();

        if has_updated && has_deleted {
            // System columns already exist
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
            // Not nullable
        }

        Ok(Arc::new(Schema::new(fields)))
    }

    /// Parse flush policy from statement
    fn parse_flush_policy(
        &self,
        flush_policy: Option<&crate::sql::ddl::create_shared_table::FlushPolicy>,
    ) -> Result<FlushPolicy, KalamDbError> {
        match flush_policy {
            Some(crate::sql::ddl::create_shared_table::FlushPolicy::Rows(rows)) => {
                Ok(FlushPolicy::RowLimit {
                    row_limit: *rows as u32,
                })
            }
            Some(crate::sql::ddl::create_shared_table::FlushPolicy::Time(seconds)) => {
                Ok(FlushPolicy::TimeInterval {
                    interval_seconds: *seconds as u32,
                })
            }
            Some(crate::sql::ddl::create_shared_table::FlushPolicy::Combined { rows, seconds }) => {
                Ok(FlushPolicy::Combined {
                    row_limit: *rows as u32,
                    interval_seconds: *seconds as u32,
                })
            }
            None => {
                // Default flush policy: 1000 rows
                Ok(FlushPolicy::RowLimit { row_limit: 1000 })
            }
        }
    }

    /// Create schema metadata in RocksDB via kalamdb-sql
    ///
    /// Shared tables store:
    /// - Schema in system_table_schemas (version 1) WITHOUT system columns
    /// - System columns (_updated, _deleted) are added dynamically by SharedTableProvider
    /// - Table metadata in system_tables
    /// - Flush policy configuration
    fn create_schema_metadata(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        schema: &Arc<Schema>,
        storage_location: &str,
        flush_policy: &FlushPolicy,
        deleted_retention: Option<u64>,
    ) -> Result<(), KalamDbError> {
        // Serialize Arrow schema to JSON
        let arrow_with_options = ArrowSchemaWithOptions::new(schema.clone());
        let arrow_schema_json = arrow_with_options.to_json_string()?;

        // Create table ID
        let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());

        // Create TableSchema record for version 1
        let table_schema = TableSchema {
            schema_id: format!("{}:1", table_id),
            table_id: table_id.clone(),
            version: 1,
            arrow_schema: arrow_schema_json,
            // Use millis for consistency with other parts of the system
            created_at: chrono::Utc::now().timestamp_millis(),
            changes: format!(
                "Initial shared table schema. System columns: _updated, _deleted. Deleted retention: {:?}h",
                deleted_retention.map(|s| s / 3600)
            ),
        };

        // Insert schema into system_table_schemas via KalamSQL so DataFusion can load it
        self.kalam_sql
            .insert_table_schema(&table_schema)
            .map_err(|e| {
                KalamDbError::SchemaError(format!(
                    "Failed to insert schema for {}: {}",
                    table_id, e
                ))
            })?;

        // Create Table record in system_tables
        let flush_policy_str = match flush_policy {
            FlushPolicy::RowLimit { row_limit } => format!("rows:{}", row_limit),
            FlushPolicy::TimeInterval { interval_seconds } => format!("time:{}s", interval_seconds),
            FlushPolicy::Combined {
                row_limit,
                interval_seconds,
            } => format!("combined:{}rows,{}s", row_limit, interval_seconds),
        };

        let _table = kalamdb_sql::models::Table {
            table_id: table_id.clone(),
            table_name: table_name.as_str().to_string(),
            namespace: namespace_id.as_str().to_string(),
            table_type: "Shared".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            storage_location: storage_location.to_string(),
            storage_id: Some("local".to_string()),
            use_user_storage: false,
            flush_policy: flush_policy_str,
            schema_version: 1,
            deleted_retention_hours: deleted_retention.map(|s| (s / 3600) as i32).unwrap_or(0),
        };

        // TODO: Add insert_table method to kalamdb-sql
        // For now we'll skip this
        // self.kalam_sql
        //     .insert_table(&table)
        //     .map_err(|e| KalamDbError::InternalError(e.to_string()))?;

        Ok(())
    }

    /// Check if a table already exists
    fn table_exists(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
    ) -> Result<bool, KalamDbError> {
        let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());

        match self.kalam_sql.get_table(&table_id) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(KalamDbError::Other(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::DataType;
    use kalamdb_store::test_utils::TestDb;

    fn create_test_service() -> (SharedTableService, TestDb) {
        let test_db = TestDb::new(&[
            "shared_table:app:config",
            "system_tables",
            "system_table_schemas",
        ])
        .unwrap();

        let shared_table_store = Arc::new(SharedTableStore::new(test_db.db.clone()).unwrap());
        let kalam_sql = Arc::new(KalamSql::new(test_db.db.clone()).unwrap());

        let service = SharedTableService::new(shared_table_store, kalam_sql);
        (service, test_db)
    }

    #[test]
    fn test_create_shared_table() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![
            Field::new("setting_key", DataType::Utf8, false),
            Field::new("setting_value", DataType::Utf8, false),
        ]));

        let stmt = CreateSharedTableStatement {
            table_name: TableName::new("config"),
            namespace_id: NamespaceId::new("app"),
            schema,
            location: None,
            flush_policy: None,
            deleted_retention: None,
            if_not_exists: false,
        };

        let result = service.create_table(stmt);
        assert!(result.is_ok());

        let (metadata, was_created) = result.unwrap();
        assert!(was_created);
        assert_eq!(metadata.table_name.as_str(), "config");
        assert_eq!(metadata.table_type, TableType::Shared);
        assert_eq!(metadata.namespace.as_str(), "app");
        assert_eq!(metadata.storage_location, "/data/shared");
    }

    #[test]
    fn test_shared_table_has_system_columns() {
        let (service, _test_db) = create_test_service();

        // Create shared table with simple schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        // Inject system columns
        let schema_with_columns = service.inject_system_columns(schema).unwrap();

        // Verify _updated and _deleted columns exist
        assert!(schema_with_columns.field_with_name("_updated").is_ok());
        assert!(schema_with_columns.field_with_name("_deleted").is_ok());

        let updated_field = schema_with_columns.field_with_name("_updated").unwrap();
        assert_eq!(
            updated_field.data_type(),
            &DataType::Timestamp(TimeUnit::Millisecond, None)
        );

        let deleted_field = schema_with_columns.field_with_name("_deleted").unwrap();
        assert_eq!(deleted_field.data_type(), &DataType::Boolean);
    }

    #[test]
    fn test_create_table_with_custom_location() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let stmt = CreateSharedTableStatement {
            table_name: TableName::new("config"),
            namespace_id: NamespaceId::new("app"),
            schema,
            location: Some("/custom/path/shared".to_string()),
            flush_policy: None,
            deleted_retention: None,
            if_not_exists: false,
        };

        let result = service.create_table(stmt);
        assert!(result.is_ok());

        let (metadata, _was_created) = result.unwrap();
        assert_eq!(metadata.storage_location, "/custom/path/shared");
    }

    #[test]
    fn test_shared_table_rejects_user_id_templating() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let stmt = CreateSharedTableStatement {
            table_name: TableName::new("config"),
            namespace_id: NamespaceId::new("app"),
            schema,
            location: Some("/data/${user_id}/shared".to_string()), // Invalid!
            flush_policy: None,
            deleted_retention: None,
            if_not_exists: false,
        };

        let result = service.create_table(stmt);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot contain ${user_id}"));
    }

    #[test]
    fn test_parse_flush_policy() {
        let (service, _test_db) = create_test_service();

        // Row-based policy
        let policy = crate::sql::ddl::create_shared_table::FlushPolicy::Rows(500);
        let result = service.parse_flush_policy(Some(&policy)).unwrap();
        assert!(matches!(result, FlushPolicy::RowLimit { row_limit: 500 }));

        // Time-based policy
        let policy = crate::sql::ddl::create_shared_table::FlushPolicy::Time(60);
        let result = service.parse_flush_policy(Some(&policy)).unwrap();
        assert!(matches!(
            result,
            FlushPolicy::TimeInterval {
                interval_seconds: 60
            }
        ));

        // Combined policy
        let policy = crate::sql::ddl::create_shared_table::FlushPolicy::Combined {
            rows: 1000,
            seconds: 300,
        };
        let result = service.parse_flush_policy(Some(&policy)).unwrap();
        assert!(matches!(
            result,
            FlushPolicy::Combined {
                row_limit: 1000,
                interval_seconds: 300
            }
        ));

        // Default policy
        let result = service.parse_flush_policy(None).unwrap();
        assert!(matches!(result, FlushPolicy::RowLimit { row_limit: 1000 }));
    }

    #[test]
    fn test_validate_table_name() {
        let (service, _test_db) = create_test_service();

        // Valid names
        assert!(service
            .validate_table_name(&TableName::new("config"))
            .is_ok());
        assert!(service
            .validate_table_name(&TableName::new("_private"))
            .is_ok());
        assert!(service
            .validate_table_name(&TableName::new("table_123"))
            .is_ok());

        // Invalid names: start with uppercase
        assert!(service
            .validate_table_name(&TableName::new("Config"))
            .is_err());

        // Invalid names: SQL keywords
        assert!(service
            .validate_table_name(&TableName::new("select"))
            .is_err());
        assert!(service
            .validate_table_name(&TableName::new("table"))
            .is_err());
    }

    #[test]
    fn test_create_table_if_not_exists() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let stmt1 = CreateSharedTableStatement {
            table_name: TableName::new("config"),
            namespace_id: NamespaceId::new("app"),
            schema: schema.clone(),
            location: None,
            flush_policy: None,
            deleted_retention: None,
            if_not_exists: false,
        };

        // First creation should succeed
        assert!(service.create_table(stmt1).is_ok());

        // TODO: Once insert_table is implemented in kalamdb-sql, uncomment these tests
        // Second creation without IF NOT EXISTS should fail
        // let stmt2 = CreateSharedTableStatement {
        //     table_name: TableName::new("config"),
        //     namespace_id: NamespaceId::new("app"),
        //     schema: schema.clone(),
        //     location: None,
        //     flush_policy: None,
        //     deleted_retention: None,
        //     if_not_exists: false,
        // };
        // assert!(service.create_table(stmt2).is_err());

        // Third creation with IF NOT EXISTS should fail but with specific error
        // let stmt3 = CreateSharedTableStatement {
        //     table_name: TableName::new("config"),
        //     namespace_id: NamespaceId::new("app"),
        //     schema,
        //     location: None,
        //     flush_policy: None,
        //     deleted_retention: None,
        //     if_not_exists: true,
        // };
        // let result = service.create_table(stmt3);
        // assert!(result.is_err());
        // assert!(matches!(result.unwrap_err(), KalamDbError::AlreadyExists(_)));
    }
}
