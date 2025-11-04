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
//! **REFACTORED (Phase 5, T205)**: Stateless service - fetches dependencies from AppContext

use crate::app_context::AppContext;
use crate::catalog::{NamespaceId, TableName, TableType, UserId};
use crate::error::KalamDbError;
// TODO: Phase 2b - FlushPolicy import will be needed again
// use crate::flush::FlushPolicy;
// TODO: Phase 2b - StorageLocationService deprecated (replaced by system_storages)
// use crate::services::storage_location_service::StorageLocationService;
use crate::storage::column_family_manager::ColumnFamilyManager;
use crate::stores::system_table::UserTableStoreExt;
use datafusion::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use kalamdb_sql::ddl::CreateTableStatement;
use std::sync::Arc;

/// User table service (stateless)
///
/// Coordinates user table creation across schema storage (RocksDB),
/// column families, and metadata management.
///
/// **Phase 5 Optimization**: Zero-sized struct - all dependencies fetched
/// from AppContext::get() on demand. No stored Arc<_> fields.
pub struct UserTableService;

impl UserTableService {
    /// Create a new user table service (zero-sized)
    pub fn new() -> Self {
        Self
    }
    
    /// Create a new user table service (deprecated - use new())
    /// 
    /// **Deprecated**: This signature exists for backward compatibility during migration.
    /// Use `UserTableService::new()` instead. The parameters are ignored.
    #[deprecated(since = "0.1.0", note = "Use UserTableService::new() instead - service is now stateless")]
    pub fn new_with_deps(_kalam_sql: std::sync::Arc<kalamdb_sql::KalamSql>, _user_table_store: std::sync::Arc<crate::tables::UserTableStore>) -> Self {
        Self
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
    /// Ok(()) on success
    pub fn create_table(&self, stmt: CreateTableStatement) -> Result<(), KalamDbError> {
        // Validate table name
        Self::validate_table_name(stmt.table_name.as_str())
            .map_err(KalamDbError::InvalidOperation)?;

        let namespace_id_core = stmt.namespace_id.clone();
        let table_name_core = stmt.table_name.clone();

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

        // 3. Inject DEFAULT SNOWFLAKE_ID() for auto-injected id column
        let mut modified_stmt = stmt.clone();
        if !modified_stmt.column_defaults.contains_key("id") {
            modified_stmt.column_defaults.insert(
                "id".to_string(),
                kalamdb_commons::schemas::ColumnDefault::function("SNOWFLAKE_ID", vec![]),
            );
        }

        // 4. Storage location resolution - handled dynamically via TableCache in flush jobs
        // storage_id is stored in table metadata for later path resolution

        // 5. Save complete table definition to information_schema_tables (atomic write)
        self.save_table_definition(&modified_stmt, &schema)?;

        // 6. Create RocksDB column family for this table
        // This ensures the table is ready for data operations immediately after creation
        let ctx = AppContext::get();
        let user_table_store = ctx.user_table_store();
        user_table_store
            .create_column_family(namespace_id_core.as_str(), table_name_core.as_str())
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to create column family for user table {}.{}: {}",
                    namespace_id_core.as_str(),
                    table_name_core.as_str(),
                    e
                ))
            })?;

        // Return success - table metadata is now in cache via save_table_definition
        Ok(())
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
        use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, SchemaVersion};
        use kalamdb_commons::types::{KalamDataType, FromArrowType};
        use crate::schema::arrow_schema::ArrowSchemaWithOptions;

        // Extract columns directly from Arrow schema
        let columns: Vec<ColumnDefinition> = schema
            .fields()
            .iter()
            .enumerate()
            .map(|(idx, field)| {
                let column_name = field.name().clone();
                let is_primary_key = stmt.primary_key_column
                    .as_ref()
                    .map(|pk| pk.as_str() == column_name.as_str())
                    .unwrap_or(false);
                
                // Convert Arrow DataType to KalamDataType
                let data_type = KalamDataType::from_arrow_type(field.data_type())
                    .unwrap_or(KalamDataType::Text);
                
                // Get default value from statement
                let default_value = stmt.column_defaults
                    .get(column_name.as_str())
                    .cloned()
                    .unwrap_or(kalamdb_commons::schemas::ColumnDefault::None);
                
                ColumnDefinition::new(
                    column_name,
                    (idx + 1) as u32,
                    data_type,
                    field.is_nullable(),
                    is_primary_key,
                    false, // is_partition_key
                    default_value,
                    None, // column_comment
                )
            })
            .collect();

        // Build table options (USER tables use default options)
        let table_options = TableOptions::user();

        // Create NEW TableDefinition directly
        let mut table_def = TableDefinition::new(
            stmt.namespace_id.clone(),
            stmt.table_name.clone(),
            kalamdb_commons::schemas::TableType::User,
            columns,
            table_options,
            None, // table_comment
        ).map_err(|e| KalamDbError::SchemaError(e))?;

        // Initialize schema history with version 1 entry (Initial schema)
        // Serialize Arrow schema (including any options if needed)
        let schema_json = ArrowSchemaWithOptions::new(schema.clone())
            .to_json_string()
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to serialize Arrow schema: {}", e)))?;

        // Push initial schema version (v1)
        table_def.schema_history.push(SchemaVersion::initial(schema_json));

        // Single atomic write to information_schema_tables
        let ctx = AppContext::get();
        let kalam_sql = ctx.kalam_sql();
        kalam_sql
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
        let ctx = AppContext::get();
        let kalam_sql = ctx.kalam_sql();
        match kalam_sql.get_table_definition(namespace, table_name) {
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
    pub fn substitute_user_id(path: &str, user_id: &UserId) -> String {
        path.replace("${user_id}", user_id.as_str())
    }

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
    fn validate_table_name(name: &str) -> Result<(), String> {
        // Check first character
        let first_char = name.chars().next().ok_or_else(|| {
            "Table name cannot be empty".to_string()
        })?;

        if !first_char.is_lowercase() && first_char != '_' {
            return Err(format!(
                "Table name must start with lowercase letter or underscore: {}",
                name
            ));
        }

        // Check for SQL keywords
        let keywords = [
            "select", "insert", "update", "delete", "table", "from", "where",
        ];
        if keywords.contains(&name.to_lowercase().as_str()) {
            return Err(format!("Table name cannot be a SQL keyword: {}", name));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::{RocksDBBackend, StorageBackend};

    fn setup_test_service() -> UserTableService {
        let _test_db =
            TestDb::new(&["system_table_schemas", "system_namespaces", "system_tables"]).unwrap();

        // Note: UserTableService is now stateless and doesn't need dependencies
        // AppContext would be initialized in a real test, but for unit tests we just create the service
        UserTableService::new()
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
    fn test_validate_table_name() {
        assert!(UserTableService::validate_table_name("users").is_ok());
        assert!(UserTableService::validate_table_name("_private").is_ok());
        assert!(UserTableService::validate_table_name("table_123").is_ok());
        
        // Should fail: starts with uppercase
        assert!(UserTableService::validate_table_name("Users").is_err());
        
        // Should fail: SQL keyword
        assert!(UserTableService::validate_table_name("select").is_err());
    }
}