//! User table service
//!
//! This service handles all user table-related operations including:
//! - Creating user tables with schema validation
//! - Auto-increment field injection (snowflake ID)
//! - System column injection (_updated, _deleted)
//! - Storage location resolution
//! - Schema storage in information_schema.tables (Phase 2b - currently TODO)
//! - Column family creation
//! - Deleted retention configuration
//!
//! **REFACTORED**: Migrating from system_table_schemas to information_schema.tables

use crate::catalog::{NamespaceId, TableMetadata, TableName, TableType, UserId};
use crate::error::KalamDbError;
// TODO: Phase 2b - FlushPolicy import will be needed again
// use crate::flush::FlushPolicy;
use crate::schema::arrow_schema::ArrowSchemaWithOptions;
// TODO: Phase 2b - StorageLocationService deprecated (replaced by system_storages)
// use crate::services::storage_location_service::StorageLocationService;
use crate::storage::column_family_manager::ColumnFamilyManager;
use datafusion::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use kalamdb_commons::models::StorageId;
use kalamdb_sql::ddl::{CreateTableStatement, FlushPolicy as DdlFlushPolicy};
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
    /// * `stmt` - Parsed CREATE TABLE statement (USER table)
    ///
    /// # Returns
    /// Table metadata for the created table
    pub fn create_table(&self, stmt: CreateTableStatement) -> Result<TableMetadata, KalamDbError> {
        // Validate table name
        TableMetadata::validate_table_name(stmt.table_name.as_str())
            .map_err(KalamDbError::InvalidOperation)?;

        let namespace_id_core = NamespaceId::from(stmt.namespace_id.clone());
        let table_name_core = TableName::from(stmt.table_name.clone());

        // Check if table already exists
        if self.table_exists(&namespace_id_core, &table_name_core)? {
            if stmt.if_not_exists {
                // Return error with special message that can be handled by caller
                return Err(KalamDbError::AlreadyExists(format!(
                    "Table {}.{} already exists",
                    namespace_id_core.as_str(),
                    table_name_core.as_str()
                )));
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Table {}.{} already exists",
                    namespace_id_core.as_str(),
                    table_name_core.as_str()
                )));
            }
        }

        // 1. Auto-increment field injection
        let schema = self.inject_auto_increment_field(stmt.schema.clone())?;

        // 2. System column injection (_updated, _deleted)
        let schema = self.inject_system_columns(schema, TableType::User)?;

        // 3. Storage location resolution - use storage_id to get the path template
        let default_storage = StorageId::local();
        let storage_id = stmt.storage_id.as_ref().unwrap_or(&default_storage);
        let storage_location = self.resolve_storage_from_id(storage_id)?;

        // 4. Save complete table definition to information_schema_tables (atomic write)
        self.save_table_definition(&stmt, &schema)?;

        // 5. Create RocksDB column family for this table
        // This ensures the table is ready for data operations immediately after creation
        self.user_table_store
            .create_column_family(namespace_id_core.as_str(), table_name_core.as_str())
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to create column family for user table {}.{}: {}",
                    namespace_id_core.as_str(),
                    table_name_core.as_str(),
                    e
                ))
            })?;

        // 6. Create and return table metadata
        let metadata = TableMetadata {
            table_name: table_name_core.clone(),
            table_type: TableType::User,
            namespace: namespace_id_core.clone(),
            created_at: chrono::Utc::now(),
            storage_location,
            flush_policy: stmt
                .flush_policy
                .clone()
                .map(crate::flush::FlushPolicy::from)
                .unwrap_or_default(),
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

    /// Resolve storage location from storage_id
    ///
    /// Gets the user table template path from the storage in system.storages
    fn resolve_storage_from_id(&self, storage_id: &StorageId) -> Result<String, KalamDbError> {
        // Get the storage location from system.storages via KalamSQL
        let storage = self
            .kalam_sql
            .get_storage(storage_id)
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to get storage '{}': {}", storage_id, e))
            })?
            .ok_or_else(|| KalamDbError::NotFound(format!("Storage '{}' not found", storage_id)))?;

        // For user tables, we use the user_tables_template from the storage
        // The template should be in the format: "{namespace}/users/{tableName}/{shard}/{userId}/"
        Ok(storage.user_tables_template)
    }

    /// Create and save table definition to information_schema_tables.
    /// Replaces fragmented schema storage with single atomic write.
    ///
    /// # Arguments
    /// * `stmt` - CREATE TABLE statement with all metadata
    /// * `schema` - Final Arrow schema (after auto-increment and system column injection)
    ///
    /// # Returns
    /// Ok(()) on success, error on failure
    fn save_table_definition(
        &self,
        stmt: &CreateTableStatement,
        schema: &Arc<Schema>,
    ) -> Result<(), KalamDbError> {
        use kalamdb_commons::models::{
            ColumnDefinition, FlushPolicyDef, SchemaVersion, TableDefinition,
        };
        use std::collections::HashMap;

        // Extract columns from schema with ordinal positions
        let columns = TableDefinition::extract_columns_from_schema(
            schema.as_ref(),
            &stmt.column_defaults,
            stmt.primary_key_column.as_deref(),
        );

        // Serialize Arrow schema for history
        let arrow_schema_json =
            TableDefinition::serialize_arrow_schema(schema.as_ref()).map_err(|e| {
                KalamDbError::SchemaError(format!("Failed to serialize Arrow schema: {}", e))
            })?;

        // Build flush policy definition
        let flush_policy_def = stmt.flush_policy.as_ref().map(|policy| match policy {
            DdlFlushPolicy::RowLimit { row_limit } => FlushPolicyDef {
                row_threshold: Some(*row_limit as u64),
                interval_seconds: None,
            },
            DdlFlushPolicy::TimeInterval { interval_seconds } => FlushPolicyDef {
                row_threshold: None,
                interval_seconds: Some(*interval_seconds as u64),
            },
            DdlFlushPolicy::Combined {
                row_limit,
                interval_seconds,
            } => FlushPolicyDef {
                row_threshold: Some(*row_limit as u64),
                interval_seconds: Some(*interval_seconds as u64),
            },
        });

        // Build complete table definition
        let now_millis = chrono::Utc::now().timestamp_millis();
        let table_def = TableDefinition {
            table_id: format!(
                "{}:{}",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ),
            table_name: stmt.table_name.clone(),
            namespace_id: stmt.namespace_id.clone(),
            table_type: TableType::User,
            created_at: now_millis,
            updated_at: now_millis,
            schema_version: 1,
            storage_id: stmt
                .storage_id
                .clone()
                .unwrap_or_else(|| StorageId::from("local")),
            use_user_storage: stmt.use_user_storage,
            flush_policy: flush_policy_def,
            deleted_retention_hours: stmt.deleted_retention_hours,
            ttl_seconds: stmt.ttl_seconds,
            columns,
            schema_history: vec![SchemaVersion {
                version: 1,
                created_at: now_millis,
                changes: "Initial schema".to_string(),
                arrow_schema_json,
            }],
        };

        // Single atomic write to information_schema_tables
        self.kalam_sql
            .upsert_table_definition(&table_def)
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to save table definition to information_schema.tables: {}",
                    e
                ))
            })?;

        log::info!(
            "Table definition for {}.{} saved to information_schema.tables (version 1)",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str()
        );

        Ok(())
    }

    /// DEPRECATED: Create schema files for the table
    ///
    /// **REPLACED BY**: save_table_definition() which writes to information_schema_tables
    ///
    /// Creates:
    /// - schema_v1.json: Arrow schema in JSON format
    /// - manifest.json: Schema versioning metadata
    fn create_schema_files(
        &self,
        namespace: &NamespaceId,
        table_name: &TableName,
        schema: &Arc<Schema>,
        _flush_policy: &Option<DdlFlushPolicy>,
        _deleted_retention_hours: Option<u32>,
    ) -> Result<(), KalamDbError> {
        // Create schema with options wrapper
        let schema_with_options = ArrowSchemaWithOptions::new(schema.clone());

        // TODO: Phase 2b - Save schema to information_schema.tables (TableDefinition.schema_history)
        let schema_json = schema_with_options.to_json()?;
        let schema_str = serde_json::to_string(&schema_json)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to serialize schema: {}", e)))?;

        // Create TableSchema record
        let table_id = format!("{}:{}", namespace.as_str(), table_name.as_str());
        let table_schema = kalamdb_sql::TableSchema {
            schema_id: format!("{}_v1", table_id),
            table_id: table_id.clone(),
            version: 1,
            arrow_schema: schema_str,
            created_at: chrono::Utc::now().timestamp_millis(),
            changes: "Initial schema".to_string(),
        };

        // TODO: Replace with information_schema_tables storage (Phase 2b)
        // Schema will be stored in TableDefinition.schema_history array
        // self.kalam_sql.insert_table_schema(&table_schema)?;

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
        namespace: &NamespaceId,
        table_name: &TableName,
    ) -> Result<bool, KalamDbError> {
        // Query system.tables using KalamSQL
        let table_id = format!("{}:{}", namespace.as_str(), table_name.as_str());

        match self.kalam_sql.get_table(&table_id) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(KalamDbError::Other(format!(
                "Failed to check if table exists: {}",
                e
            ))),
        }
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
}
