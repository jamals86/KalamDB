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
use crate::stores::system_table::SharedTableStoreExt;
use crate::tables::SharedTableStore;
use datafusion::arrow::datatypes::Schema;
use kalamdb_commons::models::StorageId;
use kalamdb_sql::ddl::{CreateTableStatement, FlushPolicy as DdlFlushPolicy};
use kalamdb_sql::KalamSql;
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
    default_storage_path: String,
}

impl SharedTableService {
    /// Create a new shared table service
    ///
    /// # Arguments
    /// * `shared_table_store` - Storage backend for shared tables
    /// * `kalam_sql` - KalamSQL instance for metadata storage
    /// * `default_storage_path` - Default path for 'local' storage (from config.toml)
    pub fn new(
        shared_table_store: Arc<SharedTableStore>,
        kalam_sql: Arc<KalamSql>,
        default_storage_path: String,
    ) -> Self {
        Self {
            shared_table_store,
            kalam_sql,
            default_storage_path,
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
        stmt: CreateTableStatement,
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

        // Resolve storage location from storage_id (defaulting to 'local')
        // Uses config.default_storage_path (default: "./data/storage") when base_directory is empty
        let storage_id = stmt
            .storage_id
            .as_ref()
            .cloned()
            .unwrap_or_else(StorageId::local);

        let storage_location = match self.kalam_sql.get_storage(&storage_id) {
            Ok(Some(cfg)) => {
                // If base_directory is empty, use configured default_storage_path
                let base = if cfg.base_directory.trim().is_empty() {
                    self.default_storage_path.trim_end_matches('/').to_string()
                } else {
                    cfg.base_directory.trim_end_matches('/').to_string()
                };
                format!("{}/shared", base)
            }
            Ok(None) => {
                // No storage config found, fall back to default
                format!("{}/shared", self.default_storage_path.trim_end_matches('/'))
            }
            Err(e) => {
                // Fall back to default and surface a warning via error type
                log::warn!(
                    "Falling back to default shared storage due to get_storage error: {}",
                    e
                );
                format!("{}/shared", self.default_storage_path.trim_end_matches('/'))
            }
        };

        // Validate no ${user_id} templating in shared table storage location
        // TODO: We have a compiler for template strings; use that here for validation
        if storage_location.contains("${user_id}") {
            return Err(KalamDbError::InvalidOperation(
                "Shared table storage location cannot contain ${user_id} template variable"
                    .to_string(),
            ));
        }

        // Parse flush policy
        let flush_policy = self.parse_flush_policy(stmt.flush_policy.as_ref())?;

        // Save complete table definition to information_schema_tables (atomic write)
        self.save_table_definition(&stmt, &schema)?;

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
            deleted_retention_hours: stmt.deleted_retention_hours,
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

    /// Parse flush policy from statement
    fn parse_flush_policy(
        &self,
        flush_policy: Option<&DdlFlushPolicy>,
    ) -> Result<FlushPolicy, KalamDbError> {
        match flush_policy {
            Some(DdlFlushPolicy::RowLimit { row_limit }) => Ok(FlushPolicy::RowLimit {
                row_limit: *row_limit,
            }),
            Some(DdlFlushPolicy::TimeInterval { interval_seconds }) => {
                Ok(FlushPolicy::TimeInterval {
                    interval_seconds: *interval_seconds,
                })
            }
            Some(DdlFlushPolicy::Combined {
                row_limit,
                interval_seconds,
            }) => Ok(FlushPolicy::Combined {
                row_limit: *row_limit,
                interval_seconds: *interval_seconds,
            }),
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
    /// Create and save table definition to information_schema_tables.
    /// Replaces fragmented schema storage with single atomic write.
    ///
    /// # Arguments
    /// * `stmt` - CREATE TABLE statement with all metadata
    /// * `schema` - Final Arrow schema (after system column injection if applicable)
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

        // Build table options (SHARED tables use default options)
        let table_options = TableOptions::shared();

        // Create NEW TableDefinition directly
        let mut table_def = TableDefinition::new(
            stmt.namespace_id.clone(),
            stmt.table_name.clone(),
            kalamdb_commons::schemas::TableType::Shared,
            columns,
            table_options,
            None, // table_comment
        ).map_err(|e| KalamDbError::SchemaError(e))?;

        // Initialize schema history with version 1 entry (Initial schema)
        let schema_json = ArrowSchemaWithOptions::new(schema.clone())
            .to_json_string()
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to serialize Arrow schema: {}", e)))?;
        table_def.schema_history.push(SchemaVersion::initial(schema_json));

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
    use arrow::datatypes::Field;
    use datafusion::arrow::datatypes::DataType;
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::{RocksDBBackend, StorageBackend};

    fn create_test_service() -> (SharedTableService, TestDb) {
        let test_db = TestDb::new(&[
            "shared_table:app:config",
            "system_tables",
            "information_schema_tables",
        ])
        .unwrap();

        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));
        let shared_table_store =
            Arc::new(SharedTableStore::new(backend, "shared_table:app:config"));
        let kalam_sql =
            Arc::new(KalamSql::new(Arc::new(RocksDBBackend::new(test_db.db.clone()))).unwrap());

        let service = SharedTableService::new(
            shared_table_store,
            kalam_sql,
            "./data/storage".to_string(),
        );
        (service, test_db)
    }

    #[test]
    fn test_create_shared_table() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![
            Field::new("setting_key", DataType::Utf8, false),
            Field::new("setting_value", DataType::Utf8, false),
        ]));

        let stmt = CreateTableStatement {
            table_name: TableName::new("config"),
            namespace_id: NamespaceId::new("app"),
            table_type: kalamdb_commons::schemas::TableType::Shared,
            schema,
            column_defaults: std::collections::HashMap::new(),
            primary_key_column: None,
            storage_id: None, // Will default to 'local'
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            ttl_seconds: None,
            if_not_exists: false,
            access_level: None,
        };

        let result = service.create_table(stmt);
        assert!(result.is_ok());

        let (metadata, was_created) = result.unwrap();
        assert!(was_created);
        assert_eq!(metadata.table_name.as_str(), "config");
        assert_eq!(metadata.table_type, TableType::Shared);
        assert_eq!(metadata.namespace.as_str(), "app");
        assert_eq!(metadata.storage_location, "./data/storage/shared");
    }

    #[test]
    fn test_create_table_with_custom_location() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let stmt = CreateTableStatement {
            table_name: TableName::new("config"),
            namespace_id: NamespaceId::new("app"),
            table_type: kalamdb_commons::schemas::TableType::Shared,
            schema,
            column_defaults: std::collections::HashMap::new(),
            primary_key_column: None,
            storage_id: None, // Will default to 'local'
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            ttl_seconds: None,
            if_not_exists: false,
            access_level: None,
        };

        let result = service.create_table(stmt);
        assert!(result.is_ok());

        let (metadata, _was_created) = result.unwrap();
        assert_eq!(metadata.storage_location, "./data/storage/shared");
    }

    #[test]
    fn test_shared_table_rejects_user_id_templating() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let stmt = CreateTableStatement {
            table_name: TableName::new("config"),
            namespace_id: NamespaceId::new("app"),
            table_type: kalamdb_commons::schemas::TableType::Shared,
            schema,
            column_defaults: std::collections::HashMap::new(),
            primary_key_column: None,
            storage_id: None, // Will default to 'local'
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            ttl_seconds: None,
            if_not_exists: false,
            access_level: None,
        };

        // Currently storage locations are hardcoded to /data/shared
        // This test should pass since the default location doesn't contain ${user_id}
        let result = service.create_table(stmt);
        assert!(result.is_ok());

        // TODO: When storage locations become configurable, add a test that actually
        // tries to create a shared table with a location containing ${user_id}
    }

    #[test]
    fn test_parse_flush_policy() {
        let (service, _test_db) = create_test_service();

        // Row-based policy
        let policy = DdlFlushPolicy::RowLimit { row_limit: 500 };
        let result = service.parse_flush_policy(Some(&policy)).unwrap();
        assert!(matches!(result, FlushPolicy::RowLimit { row_limit: 500 }));

        // Time-based policy
        let policy = DdlFlushPolicy::TimeInterval {
            interval_seconds: 60,
        };
        let result = service.parse_flush_policy(Some(&policy)).unwrap();
        assert!(matches!(
            result,
            FlushPolicy::TimeInterval {
                interval_seconds: 60
            }
        ));

        // Combined policy
        let policy = DdlFlushPolicy::Combined {
            row_limit: 1000,
            interval_seconds: 300,
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

        let stmt1 = CreateTableStatement {
            table_name: TableName::new("config"),
            namespace_id: NamespaceId::new("app"),
            table_type: TableType::Shared,
            schema: schema.clone(),
            column_defaults: std::collections::HashMap::new(),
            primary_key_column: None,
            storage_id: None, // Will default to 'local'
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            ttl_seconds: None,
            if_not_exists: false,
            access_level: None,
        };

        // First creation should succeed
        assert!(service.create_table(stmt1).is_ok());

        // TODO: Once insert_table is implemented in kalamdb-sql, uncomment these tests
        // Second creation without IF NOT EXISTS should fail
        // let stmt2 = CreateSharedTableStatement {
        //     table_name: TableName::new("config"),
        //     namespace_id: NamespaceId::new("app"),
        //     schema: schema.clone(),
        //
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
        //
        //     flush_policy: None,
        //     deleted_retention: None,
        //     if_not_exists: true,
        // };
        // let result = service.create_table(stmt3);
        // assert!(result.is_err());
        // assert!(matches!(result.unwrap_err(), KalamDbError::AlreadyExists(_)));
    }
}
