//! User table service
//!
//! This service handles all user table-related operations including:
//! - Creating user tables with schema validation
//! - Auto-increment field injection (snowflake ID)
//! - System column injection (_updated, _deleted)
//! - Storage location resolution
//! - Schema storage in RocksDB via kalamdb-sql (system_table_schemas CF)
//! - Column family creation
//! - Deleted retention configuration
//!
//! **REFACTORED**: Now uses kalamdb-sql for schema persistence instead of filesystem JSON files

use crate::catalog::{NamespaceId, TableMetadata, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::flush::FlushPolicy;
use crate::schema::arrow_schema::ArrowSchemaWithOptions;
use crate::services::storage_location_service::StorageLocationService;
use crate::sql::ddl::create_user_table::{CreateUserTableStatement, StorageLocation};
use crate::storage::column_family_manager::ColumnFamilyManager;
use datafusion::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use kalamdb_sql::KalamSql;
use kalamdb_store::UserTableStore;
use std::sync::Arc;

/// User table service
///
/// Coordinates user table creation across schema storage (RocksDB),
/// column families, and metadata management.
pub struct UserTableService {
    kalam_sql: Arc<KalamSql>,
    user_table_store: Arc<UserTableStore>,
}

impl UserTableService {
    /// Create a new user table service
    ///
    /// # Arguments
    /// * `kalam_sql` - KalamSQL instance for schema storage
    /// * `user_table_store` - UserTableStore for column family creation
    pub fn new(kalam_sql: Arc<KalamSql>, user_table_store: Arc<UserTableStore>) -> Self {
        Self { 
            kalam_sql,
            user_table_store,
        }
    }

    /// Create a user table from a CREATE USER TABLE statement
    ///
    /// This method orchestrates:
    /// 1. Auto-increment field injection (if not present)
    /// 2. System column injection (_updated, _deleted)
    /// 3. Storage location resolution
    /// 4. Schema file creation (schema_v1.json, manifest.json, current.json)
    ///
    /// Note: Column family creation must be done separately on the DB instance
    /// because Arc<DB> doesn't allow mutable operations.
    ///
    /// # Arguments
    /// * `stmt` - Parsed CREATE USER TABLE statement
    /// * `storage_location_service` - Service to resolve location references
    ///
    /// # Returns
    /// Table metadata for the created table
    pub fn create_table(
        &self,
        stmt: CreateUserTableStatement,
        storage_location_service: Option<&StorageLocationService>,
    ) -> Result<TableMetadata, KalamDbError> {
        // Validate table name
        TableMetadata::validate_table_name(stmt.table_name.as_str())
            .map_err(KalamDbError::InvalidOperation)?;

        // Check if table already exists
        if self.table_exists(&stmt.namespace_id, &stmt.table_name)? {
            if stmt.if_not_exists {
                // Return error with special message that can be handled by caller
                return Err(KalamDbError::AlreadyExists(format!(
                    "Table {}.{} already exists",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                )));
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Table {}.{} already exists",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                )));
            }
        }

        // 1. Auto-increment field injection
        let schema = self.inject_auto_increment_field(stmt.schema.clone())?;

        // 2. System column injection (_updated, _deleted)
        let schema = self.inject_system_columns(schema, TableType::User)?;

        // 3. Storage location resolution
        let storage_location =
            self.resolve_storage_location(&stmt.storage_location, storage_location_service)?;

        // 4. Create schema file (schema_v1.json, manifest.json, current.json)
        self.create_schema_files(
            &stmt.namespace_id,
            &stmt.table_name,
            &schema,
            &stmt.flush_policy,
            stmt.deleted_retention_hours,
        )?;

        // 5. Create RocksDB column family for this table
        // This ensures the table is ready for data operations immediately after creation
        self.user_table_store
            .create_column_family(stmt.namespace_id.as_str(), stmt.table_name.as_str())
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to create column family for user table {}.{}: {}",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str(),
                    e
                ))
            })?;

        // 6. Create and return table metadata
        let metadata = TableMetadata {
            table_name: stmt.table_name.clone(),
            table_type: TableType::User,
            namespace: stmt.namespace_id.clone(),
            created_at: chrono::Utc::now(),
            storage_location,
            flush_policy: stmt.flush_policy.unwrap_or_default(),
            schema_version: 1,
            deleted_retention_hours: stmt.deleted_retention_hours,
        };

        Ok(metadata)
    }

    /// Inject auto-increment field if not present
    ///
    /// Adds a snowflake ID field named "id" as the first column if no field named "id" exists.
    /// Uses Int64 type for snowflake IDs.
    fn inject_auto_increment_field(
        &self,
        schema: Arc<Schema>,
    ) -> Result<Arc<Schema>, KalamDbError> {
        // Check if "id" field already exists
        if schema.field_with_name("id").is_ok() {
            // ID field already exists, no injection needed
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
    fn inject_system_columns(
        &self,
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

    /// Resolve storage location from statement
    ///
    /// Handles both direct path templates and location references.
    fn resolve_storage_location(
        &self,
        storage_location: &Option<StorageLocation>,
        storage_location_service: Option<&StorageLocationService>,
    ) -> Result<String, KalamDbError> {
        match storage_location {
            Some(StorageLocation::Path(path)) => {
                // Validate that path contains ${user_id} template variable
                if !path.contains("${user_id}") {
                    return Err(KalamDbError::InvalidOperation(
                        "User table storage location must contain ${user_id} template variable"
                            .to_string(),
                    ));
                }
                Ok(path.clone())
            }
            Some(StorageLocation::Reference(location_name)) => {
                // Resolve location reference via storage location service
                let _service = storage_location_service.ok_or_else(|| {
                    KalamDbError::InvalidOperation(
                        "Storage location service required to resolve location references"
                            .to_string(),
                    )
                })?;

                // TODO: Implement get_location method in StorageLocationService
                // For now, return error
                Err(KalamDbError::Other(format!(
                    "Location reference resolution not yet implemented: {}",
                    location_name
                )))
            }
            None => {
                // Use default location template
                Ok("/data/${user_id}/tables".to_string())
            }
        }
    }

    /// Create schema files for the table
    ///
    /// Creates:
    /// - schema_v1.json: Arrow schema in JSON format
    /// - manifest.json: Schema versioning metadata
    fn create_schema_files(
        &self,
        namespace: &NamespaceId,
        table_name: &TableName,
        schema: &Arc<Schema>,
        _flush_policy: &Option<FlushPolicy>,
        _deleted_retention_hours: Option<u32>,
    ) -> Result<(), KalamDbError> {
        // Create schema with options wrapper
        let schema_with_options = ArrowSchemaWithOptions::new(schema.clone());

        // Save schema to RocksDB via kalamdb-sql (system_table_schemas CF)
        let schema_json = schema_with_options.to_json()?;
        let schema_str = serde_json::to_string(&schema_json)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to serialize schema: {}", e)))?;

        // Create TableSchema record
        let table_id = format!("{}:{}", namespace.as_str(), table_name.as_str());
        let table_schema = kalamdb_sql::models::TableSchema {
            schema_id: format!("{}_v1", table_id),
            table_id: table_id.clone(),
            version: 1,
            arrow_schema: schema_str,
            created_at: chrono::Utc::now().timestamp_millis(),
            changes: "Initial schema".to_string(),
        };

        // Insert into system.table_schemas via KalamSQL
        self.kalam_sql
            .insert_table_schema(&table_schema)
            .map_err(|e| {
                KalamDbError::SchemaError(format!(
                    "Failed to insert schema for {}: {}",
                    table_id, e
                ))
            })?;

        log::info!(
            "Schema for table {} saved to system.table_schemas (version 1)",
            table_id
        );

        Ok(())
    }

    /// Get the column family name for a user table
    ///
    /// Returns the name that should be used when creating the column family.
    /// The caller must create the CF using the DB instance directly.
    pub fn get_column_family_name(namespace: &NamespaceId, table_name: &TableName) -> String {
        ColumnFamilyManager::column_family_name(TableType::User, Some(namespace), table_name)
    }

    /// Check if a table exists
    pub fn table_exists(
        &self,
        _namespace: &NamespaceId,
        _table_name: &TableName,
    ) -> Result<bool, KalamDbError> {
        // TODO: Query system_table_schemas to check if schema exists
        // For now, always return false - full implementation requires kalamdb-sql query
        log::warn!("table_exists() not fully implemented - requires system_table_schemas query");
        Ok(false)
    }

    /// Substitute ${user_id} in path template
    ///
    /// Replaces ${user_id} with actual user ID in storage paths.
    pub fn substitute_user_id(path_template: &str, user_id: &UserId) -> String {
        path_template.replace("${user_id}", user_id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::TestDb;

    fn setup_test_service() -> UserTableService {
        let test_db =
            TestDb::new(&["system_table_schemas", "system_namespaces", "system_tables"]).unwrap();

        let kalam_sql = Arc::new(KalamSql::new(test_db.db.clone()).unwrap());
        let user_table_store = Arc::new(UserTableStore::new(test_db.db.clone()).unwrap());
        UserTableService::new(kalam_sql, user_table_store)
    }

    #[test]
    fn test_inject_auto_increment_field() {
        let service = setup_test_service();

        // Schema without ID field
        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("age", DataType::Int32, false),
        ]));

        let result = service.inject_auto_increment_field(schema).unwrap();
        assert_eq!(result.fields().len(), 3);
        assert_eq!(result.field(0).name(), "id");
        assert_eq!(result.field(0).data_type(), &DataType::Int64);

        // Cleanup
    }

    #[test]
    fn test_inject_auto_increment_field_existing_id() {
        let service = setup_test_service();

        // Schema with existing ID field
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        let result = service.inject_auto_increment_field(schema.clone()).unwrap();
        assert_eq!(result.fields().len(), 2); // No change
        assert_eq!(result.field(0).name(), "id");

        // Cleanup
    }

    #[test]
    fn test_inject_system_columns() {
        let service = setup_test_service();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        let result = service
            .inject_system_columns(schema, TableType::User)
            .unwrap();
        assert_eq!(result.fields().len(), 4); // id, name, _updated, _deleted
        assert_eq!(result.field(2).name(), "_updated");
        assert_eq!(result.field(3).name(), "_deleted");

        // Cleanup
    }

    #[test]
    fn test_inject_system_columns_stream_table() {
        let service = setup_test_service();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("event", DataType::Utf8, false),
        ]));

        // Stream tables should NOT get system columns
        let result = service
            .inject_system_columns(schema.clone(), TableType::Stream)
            .unwrap();
        assert_eq!(result.fields().len(), 2); // No change
        assert_eq!(result.field(0).name(), "id");
        assert_eq!(result.field(1).name(), "event");

        // Cleanup
    }

    #[test]
    fn test_substitute_user_id() {
        let path = "/data/${user_id}/messages";
        let user_id = UserId::new("user123");
        let result = UserTableService::substitute_user_id(path, &user_id);
        assert_eq!(result, "/data/user123/messages");
    }

    #[test]
    fn test_resolve_storage_location_path() {
        let service = setup_test_service();

        let location = Some(StorageLocation::Path(
            "/data/${user_id}/messages".to_string(),
        ));
        let result = service.resolve_storage_location(&location, None).unwrap();
        assert_eq!(result, "/data/${user_id}/messages");
    }

    #[test]
    fn test_resolve_storage_location_invalid_path() {
        let service = setup_test_service();

        // Path without ${user_id} should fail
        let location = Some(StorageLocation::Path("/data/messages".to_string()));
        let result = service.resolve_storage_location(&location, None);
        assert!(result.is_err());

        // Cleanup
    }

    #[test]
    fn test_resolve_storage_location_default() {
        let service = setup_test_service();

        let result = service.resolve_storage_location(&None, None).unwrap();
        assert_eq!(result, "/data/${user_id}/tables");
    }
}
