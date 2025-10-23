//! Stream table service
//!
//! This service handles all stream table-related operations including:
//! - Creating stream tables with schema validation  
//! - NO system columns (_updated, _deleted) - stream tables are ephemeral
//! - NO auto-increment injection - stream events have timestamps
//! - Metadata registration in system_tables via kalamdb-sql
//! - Column family creation for stream_table:{namespace}:{table_name}
//! - TTL and max_buffer configuration

use crate::catalog::{NamespaceId, TableMetadata, TableName, TableType};
use crate::error::KalamDbError;
use crate::schema::arrow_schema::ArrowSchemaWithOptions;
use datafusion::arrow::datatypes::Schema;
use kalamdb_sql::ddl::CreateStreamTableStatement;
use kalamdb_sql::models::TableSchema;
use kalamdb_sql::KalamSql;
use kalamdb_store::StreamTableStore;
use std::sync::Arc;

/// Stream table service
///
/// Coordinates stream table creation across schema storage (RocksDB),
/// column families, and metadata management.
///
/// Stream tables are different from user/shared tables:
/// - NO system columns (_updated, _deleted)
/// - NO Parquet persistence (memory/RocksDB only)
/// - TTL-based eviction
/// - Optional ephemeral mode (only store if subscribers exist)
pub struct StreamTableService {
    stream_table_store: Arc<StreamTableStore>,
    kalam_sql: Arc<KalamSql>,
}

impl StreamTableService {
    /// Create a new stream table service
    ///
    /// # Arguments
    /// * `stream_table_store` - Storage backend for stream tables
    /// * `kalam_sql` - KalamSQL instance for metadata storage
    pub fn new(stream_table_store: Arc<StreamTableStore>, kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            stream_table_store,
            kalam_sql,
        }
    }

    /// Create a stream table from a CREATE STREAM TABLE statement
    ///
    /// This method orchestrates:
    /// 1. Schema validation (NO auto-increment or system column injection)
    /// 2. Metadata registration in system_tables via kalamdb-sql
    /// 3. Schema storage in system_table_schemas via kalamdb-sql
    /// 4. Table metadata creation
    ///
    /// Note: Column family creation must be done separately on the DB instance.
    ///
    /// # Arguments
    /// * `stmt` - Parsed CREATE STREAM TABLE statement
    ///
    /// # Returns
    /// Table metadata for the created stream table
    pub fn create_table(
        &self,
        stmt: CreateStreamTableStatement,
    ) -> Result<TableMetadata, KalamDbError> {
        // Validate table name
        stmt.validate_table_name()?;

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

        // Create column family for this stream table
        self.stream_table_store
            .create_column_family(stmt.namespace_id.as_str(), stmt.table_name.as_str())
            .map_err(|e| KalamDbError::Other(format!("Failed to create column family: {}", e)))?;

        // Store schema in system_table_schemas via kalamdb-sql
        self.create_schema_metadata(
            &stmt.namespace_id,
            &stmt.table_name,
            &schema,
            stmt.retention_seconds,
            stmt.ephemeral,
            stmt.max_buffer,
        )?;

        // Create and return table metadata
        let metadata = TableMetadata {
            table_name: stmt.table_name.clone(),
            table_type: TableType::Stream,
            namespace: stmt.namespace_id.clone(),
            created_at: chrono::Utc::now(),
            storage_location: String::new(), // Stream tables don't use Parquet storage
            flush_policy: crate::flush::FlushPolicy::RowLimit { row_limit: 0 }, // No flush for stream tables
            schema_version: 1,
            deleted_retention_hours: None, // Stream tables don't have soft deletes
        };

        Ok(metadata)
    }

    /// Create schema metadata in RocksDB via kalamdb-sql
    ///
    /// Stream tables store:
    /// - Schema in system_table_schemas (version 1)
    /// - Table metadata in system_tables
    /// - Retention configuration in table options
    fn create_schema_metadata(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        schema: &Arc<Schema>,
        retention_seconds: Option<u32>,
        ephemeral: bool,
        max_buffer: Option<usize>,
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
            created_at: chrono::Utc::now().timestamp_millis(), // Use millis for consistency
            changes: format!(
                "Initial stream table schema. Retention: {:?}s, Ephemeral: {}, Max Buffer: {:?}",
                retention_seconds, ephemeral, max_buffer
            ),
        };

        // Insert schema into system_table_schemas
        self.kalam_sql
            .insert_table_schema(&table_schema)
            .map_err(|e| {
                KalamDbError::SchemaError(format!("Failed to insert table schema: {}", e))
            })?;

        // Create Table record in system_tables
        let table = kalamdb_sql::models::Table {
            table_id: table_id.clone(),
            table_name: table_name.as_str().to_string(),
            namespace: namespace_id.as_str().to_string(),
            table_type: "Stream".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            storage_location: String::new(), // Stream tables don't use Parquet
            storage_id: Some("local".to_string()),
            use_user_storage: false,
            flush_policy: String::new(), // Stream tables don't flush to Parquet
            schema_version: 1,
            deleted_retention_hours: 0, // Stream tables don't have soft deletes
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
    use datafusion::arrow::datatypes::{DataType, Field};
    use kalamdb_store::test_utils::TestDb;

    fn create_test_service() -> (StreamTableService, TestDb) {
        let test_db = TestDb::new(&[
            "stream_table:app:events",
            "system_tables",
            "system_table_schemas",
        ])
        .unwrap();

        let stream_table_store = Arc::new(StreamTableStore::new(test_db.db.clone()).unwrap());
        let kalam_sql = Arc::new(KalamSql::new(test_db.db.clone()).unwrap());

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

        let stmt = CreateStreamTableStatement {
            table_name: TableName::new("events"),
            namespace_id: NamespaceId::new("app"),
            schema,
            retention_seconds: Some(300), // 5 minutes
            ephemeral: false,
            max_buffer: Some(10000),
            if_not_exists: false,
        };

        let result = service.create_table(stmt);
        assert!(result.is_ok());

        let metadata = result.unwrap();
        assert_eq!(metadata.table_name.as_str(), "events");
        assert_eq!(metadata.table_type, TableType::Stream);
        assert_eq!(metadata.namespace.as_str(), "app");
        assert_eq!(metadata.storage_location, ""); // Stream tables don't use Parquet
        assert!(metadata.deleted_retention_hours.is_none()); // Stream tables don't have soft deletes
    }

    #[test]
    fn test_stream_table_no_system_columns() {
        let (service, _test_db) = create_test_service();

        // Create stream table with simple schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("data", DataType::Utf8, false),
        ]));

        let stmt = CreateStreamTableStatement {
            table_name: TableName::new("events"),
            namespace_id: NamespaceId::new("app"),
            schema,
            retention_seconds: None,
            ephemeral: false,
            max_buffer: None,
            if_not_exists: false,
        };

        let result = service.create_table(stmt);
        assert!(result.is_ok());

        // Verify metadata doesn't include system column configuration
        let metadata = result.unwrap();
        assert!(metadata.deleted_retention_hours.is_none());
    }

    #[test]
    fn test_create_table_if_not_exists() {
        let (service, _test_db) = create_test_service();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let stmt1 = CreateStreamTableStatement {
            table_name: TableName::new("events"),
            namespace_id: NamespaceId::new("app"),
            schema: schema.clone(),
            retention_seconds: None,
            ephemeral: false,
            max_buffer: None,
            if_not_exists: false,
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
        let stmt = CreateStreamTableStatement {
            table_name: TableName::new("Events"),
            namespace_id: NamespaceId::new("app"),
            schema,
            retention_seconds: None,
            ephemeral: false,
            max_buffer: None,
            if_not_exists: false,
        };

        let result = service.create_table(stmt);
        assert!(result.is_err());
    }
}
