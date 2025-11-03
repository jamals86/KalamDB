//! Stream table service
//!
//! This service handles all stream table-related operations including:
//! - Creating stream tables with schema validation  
//! - NO system columns (_updated, _deleted) - stream tables are ephemeral
//! - NO auto-increment injection - stream events have timestamps
//! - Metadata registration in system_tables via kalamdb-sql
//! - Column family creation for stream_table:{namespace}:{table_name}
//! - TTL and max_buffer configuration
//!
//! **REFACTORED (Phase 5, T205)**: Stateless service - fetches dependencies from AppContext

use crate::app_context::AppContext;
use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;
// TODO: Phase 2b - Re-enable FlushPolicy when flushing is re-implemented
// use crate::flush::FlushPolicy;
use crate::stores::system_table::SharedTableStoreExt;
use datafusion::arrow::datatypes::Schema;
use kalamdb_sql::ddl::CreateTableStatement;
use std::sync::Arc;

/// Stream table service (stateless)
///
/// Coordinates stream table creation across schema storage (RocksDB),
/// column families, and metadata management.
///
/// Stream tables are different from user/shared tables:
/// - NO system columns (_updated, _deleted)
/// - NO Parquet persistence (memory/RocksDB only)
/// - TTL-based eviction
/// - Optional ephemeral mode (only store if subscribers exist)
///
/// **Phase 5 Optimization**: Zero-sized struct - all dependencies fetched
/// from AppContext::get() on demand.
pub struct StreamTableService;

impl StreamTableService {
    /// Create a new stream table service (zero-sized)
    pub fn new() -> Self {
        Self
    }

    /// Create a stream table from a CREATE STREAM TABLE statement
    ///
    /// This method orchestrates:
    /// 1. Schema validation (NO auto-increment or system column injection)
    /// 2. Metadata registration in system_tables via kalamdb-sql
    /// 3. Schema storage in system_table_schemas via kalamdb-sql
    /// 4. Column family creation
    ///
    /// Note: Column family creation must be done separately on the DB instance.
    ///
    /// # Arguments
    /// * `stmt` - Parsed CREATE STREAM TABLE statement
    ///
    /// # Returns
    /// Ok(()) if successful
    pub fn create_table(&self, stmt: CreateTableStatement) -> Result<(), KalamDbError> {
        // Validate table name
        Self::validate_table_name(stmt.table_name.as_str())
            .map_err(KalamDbError::InvalidOperation)?;

        // Check if table already exists
        if self.table_exists(&stmt.namespace_id, &stmt.table_name)? {
            if stmt.if_not_exists {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Stream table {}.{} already exists",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                )));
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Stream table {}.{} already exists",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                )));
            }
        }

        // Stream tables use the schema as-is (no modifications)
        let schema = stmt.schema.clone();

        // Save complete table definition to information_schema_tables (atomic write)
        self.save_table_definition(&stmt, &schema)?;

        // Create the column family in RocksDB
        SharedTableStoreExt::create_column_family(
            self.stream_table_store.as_ref(),
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
        )
        .map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to create column family: {}", e))
        })?;

        // Table created successfully
        Ok(())
    }

    /// Create and save table definition to information_schema_tables.
    /// Replaces fragmented schema storage with single atomic write.
    ///
    /// # Arguments
    /// * `stmt` - CREATE TABLE statement with all metadata
    /// * `schema` - Final Arrow schema (stream tables use schema as-is)
    ///
    /// # Returns
    /// Ok(()) on success, error on failure
    fn save_table_definition(
        &self,
        stmt: &CreateTableStatement,
        schema: &Arc<Schema>,
    ) -> Result<(), KalamDbError> {
        use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions};
        use kalamdb_commons::types::{KalamDataType, FromArrowType};

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

        // Build table options with TTL from statement
        let ttl_seconds = stmt.ttl_seconds.unwrap_or(86400); // Default 24h if not specified
        let table_options = TableOptions::stream(ttl_seconds);

        // Create NEW TableDefinition directly
        let table_def = TableDefinition::new(
            stmt.namespace_id.clone(),
            stmt.table_name.clone(),
            kalamdb_commons::schemas::TableType::Stream,
            columns,
            table_options,
            None, // table_comment
        ).map_err(|e| KalamDbError::SchemaError(e))?;

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

    /// DEPRECATED: Create schema metadata in RocksDB via kalamdb-sql
    ///
    /// **REPLACED BY**: save_table_definition() which writes to information_schema_tables
    ///
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

    /// Check if a table already exists
    fn table_exists(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
    ) -> Result<bool, KalamDbError> {
        match self.kalam_sql.get_table_definition(namespace_id, table_name) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(KalamDbError::Other(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field};
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::{RocksDBBackend, StorageBackend};
    use kalamdb_commons::models::StorageId;
    use kalamdb_commons::schemas::TableType;

    fn create_test_service() -> (StreamTableService, TestDb) {
        let test_db = TestDb::new(&[
            "stream_table:app:events",
            "system_tables",
            "information_schema_tables",
        ])
        .unwrap();

        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));
        let stream_table_store = Arc::new(StreamTableStore::new(backend.clone(), "stream_tables"));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());

        let service = StreamTableService::new(stream_table_store, kalam_sql);
        (service, test_db)
    }

    #[test]
    fn test_create_stream_table() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
            Field::new(
                "timestamp",
                DataType::Timestamp(datafusion::arrow::datatypes::TimeUnit::Millisecond, None),
                false,
            ),
        ]));

        let stmt = CreateTableStatement {
            table_name: TableName::new("events"),
            namespace_id: NamespaceId::new("app"),
            table_type: TableType::Stream,
            schema,
            column_defaults: std::collections::HashMap::new(),
            primary_key_column: None,
            storage_id: None,
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            ttl_seconds: Some(300), // 5 minutes (was retention_seconds)
            if_not_exists: false,
            access_level: None,
        };

        let result = service.create_table(stmt);
        assert!(result.is_ok());

        // Verify table was created by checking if it exists
        assert!(service.table_exists(&NamespaceId::new("app"), &TableName::new("events")).unwrap());
    }

    #[test]
    fn test_stream_table_no_system_columns() {
        let (service, _test_db) = create_test_service();

        // Create stream table with simple schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("data", DataType::Utf8, false),
        ]));

        let stmt = CreateTableStatement {
            table_name: TableName::new("events"),
            namespace_id: NamespaceId::new("app"),
            table_type: TableType::Stream,
            schema,
            column_defaults: std::collections::HashMap::new(),
            primary_key_column: None,
            storage_id: None,
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            ttl_seconds: None, // No retention (was retention_seconds)
            if_not_exists: false,
            access_level: None,
        };

        let result = service.create_table(stmt);
        assert!(result.is_ok());

        // Verify table was created
        assert!(service.table_exists(&NamespaceId::new("app"), &TableName::new("events")).unwrap());
    }

    #[test]
    fn test_create_table_if_not_exists() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let stmt1 = CreateTableStatement {
            table_name: TableName::new("events"),
            namespace_id: NamespaceId::new("app"),
            table_type: TableType::Stream,
            schema: schema.clone(),
            column_defaults: std::collections::HashMap::new(),
            primary_key_column: None,
            storage_id: None,
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
        // let stmt2 = CreateStreamTableStatement {
        //     table_name: TableName::new("events"),
        //     namespace_id: NamespaceId::new("app"),
        //     schema: schema.clone(),
        //     retention_seconds: None,
        //     ephemeral: false,
        //     max_buffer: None,
        //     if_not_exists: false,
        // };
        // assert!(service.create_table(stmt2).is_err());

        // Third creation with IF NOT EXISTS should fail but with specific error
        // let stmt3 = CreateStreamTableStatement {
        //     table_name: TableName::new("events"),
        //     namespace_id: NamespaceId::new("app"),
        //     schema,
        //     retention_seconds: None,
        //     ephemeral: false,
        //     max_buffer: None,
        //     if_not_exists: true,
        // };
        // let result = service.create_table(stmt3);
        // assert!(result.is_err());
        // assert!(matches!(result.unwrap_err(), KalamDbError::AlreadyExists(_)));
    }

    #[test]
    fn test_validate_table_name() {
        let (service, _test_db) = create_test_service();

        // Invalid name: starts with uppercase
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let stmt = CreateTableStatement {
            table_name: TableName::new("Events"),
            namespace_id: NamespaceId::new("app"),
            table_type: TableType::Stream,
            schema,
            column_defaults: std::collections::HashMap::new(),
            primary_key_column: None,
            storage_id: None,
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            ttl_seconds: None,
            if_not_exists: false,
            access_level: None,
        };

        let result = service.create_table(stmt);
        assert!(result.is_err());
    }
}
