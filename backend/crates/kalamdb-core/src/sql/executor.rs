//! SQL executor for DDL and DML statements
//!
//! This module provides execution logic for SQL statements,
//! coordinating between parsers, services, and DataFusion.

use crate::catalog::Namespace;
use crate::catalog::{NamespaceId, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::schema::arrow_schema::ArrowSchemaWithOptions;
use crate::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use crate::sql::ddl::{
    AlterNamespaceStatement, CreateNamespaceStatement, CreateSharedTableStatement,
    CreateStreamTableStatement, CreateUserTableStatement, DescribeTableStatement,
    DropNamespaceStatement, DropTableStatement, ShowNamespacesStatement, ShowTableStatsStatement,
    ShowTablesStatement,
};
use crate::tables::{SharedTableProvider, StreamTableProvider, UserTableProvider};
use datafusion::arrow::array::{ArrayRef, StringArray, UInt32Array};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::prelude::SessionContext;
use kalamdb_sql::KalamSql;
use kalamdb_store::{SharedTableStore, StreamTableStore, UserTableStore};
use std::sync::Arc;

/// SQL execution result
#[derive(Debug)]
pub enum ExecutionResult {
    /// DDL operation completed successfully with a message
    Success(String),

    /// Query result as Arrow RecordBatch
    RecordBatch(RecordBatch),

    /// Multiple record batches (for streaming results)
    RecordBatches(Vec<RecordBatch>),
}

/// SQL executor
///
/// Executes SQL statements by dispatching to appropriate handlers
pub struct SqlExecutor {
    namespace_service: Arc<NamespaceService>,
    session_context: Arc<SessionContext>,
    user_table_service: Arc<UserTableService>,
    shared_table_service: Arc<SharedTableService>,
    stream_table_service: Arc<StreamTableService>,
    table_deletion_service: Option<Arc<TableDeletionService>>,
    // Stores for table provider creation
    user_table_store: Option<Arc<UserTableStore>>,
    shared_table_store: Option<Arc<SharedTableStore>>,
    stream_table_store: Option<Arc<StreamTableStore>>,
    kalam_sql: Option<Arc<KalamSql>>,
}

impl SqlExecutor {
    /// Create a new SQL executor
    pub fn new(
        namespace_service: Arc<NamespaceService>,
        session_context: Arc<SessionContext>,
        user_table_service: Arc<UserTableService>,
        shared_table_service: Arc<SharedTableService>,
        stream_table_service: Arc<StreamTableService>,
    ) -> Self {
        Self {
            namespace_service,
            session_context,
            user_table_service,
            shared_table_service,
            stream_table_service,
            table_deletion_service: None,
            user_table_store: None,
            shared_table_store: None,
            stream_table_store: None,
            kalam_sql: None,
        }
    }

    /// Set the table deletion service (optional, for DROP TABLE support)
    pub fn with_table_deletion_service(mut self, service: Arc<TableDeletionService>) -> Self {
        self.table_deletion_service = Some(service);
        self
    }

    /// Set stores and KalamSQL for table registration (optional, for SELECT/INSERT/UPDATE/DELETE support)
    pub fn with_stores(
        mut self,
        user_table_store: Arc<UserTableStore>,
        shared_table_store: Arc<SharedTableStore>,
        stream_table_store: Arc<StreamTableStore>,
        kalam_sql: Arc<KalamSql>,
    ) -> Self {
        self.user_table_store = Some(user_table_store);
        self.shared_table_store = Some(shared_table_store);
        self.stream_table_store = Some(stream_table_store);
        self.kalam_sql = Some(kalam_sql);
        self
    }

    /// Load and register existing tables from system_tables on initialization
    pub async fn load_existing_tables(&self, default_user_id: UserId) -> Result<(), KalamDbError> {
        let kalam_sql = self.kalam_sql.as_ref().ok_or_else(|| {
            KalamDbError::InvalidOperation(
                "Cannot load tables - KalamSQL not configured".to_string(),
            )
        })?;

        // Get all tables from system_tables
        let tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {:?}", e)))?;

        for table in tables {
            // Get schema for the table
            let schemas = kalam_sql
                .get_table_schemas_for_table(&table.table_id)
                .map_err(|e| KalamDbError::Other(format!("Failed to get schemas: {:?}", e)))?;

            if schemas.is_empty() {
                continue; // Skip tables without schemas
            }

            // Use the latest schema version
            let latest_schema = &schemas[0];
            let schema_with_opts =
                ArrowSchemaWithOptions::from_json_string(&latest_schema.arrow_schema)
                    .map_err(|e| KalamDbError::Other(format!("Failed to parse schema: {:?}", e)))?;
            let schema = schema_with_opts.schema;

            // Parse namespace and table_name
            let namespace_id = NamespaceId::from(table.namespace.as_str());
            let table_name = TableName::from(table.table_name.as_str());
            let table_type = TableType::from_str(&table.table_type).ok_or_else(|| {
                KalamDbError::Other(format!("Invalid table type: {}", table.table_type))
            })?;

            // Register based on table type
            self.register_table_with_datafusion(
                &namespace_id,
                &table_name,
                table_type,
                schema,
                default_user_id.clone(),
            )
            .await?;
        }

        Ok(())
    }

    /// Register a table with DataFusion SessionContext
    async fn register_table_with_datafusion(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        table_type: TableType,
        schema: SchemaRef,
        default_user_id: UserId,
    ) -> Result<(), KalamDbError> {
        use datafusion::catalog::schema::MemorySchemaProvider;

        // Use the "kalam" catalog (configured in DataFusionSessionFactory)
        let catalog_name = "kalam";

        let catalog = self
            .session_context
            .catalog(catalog_name)
            .ok_or_else(|| KalamDbError::Other(format!("Catalog '{}' not found", catalog_name)))?;

        // Ensure schema exists for the namespace
        let namespace_str = namespace_id.as_str();
        if catalog.schema(namespace_str).is_none() {
            // Create new schema for this namespace
            let new_schema = Arc::new(MemorySchemaProvider::new());
            catalog
                .register_schema(namespace_str, new_schema)
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to register schema '{}': {}",
                        namespace_str, e
                    ))
                })?;
        }

        // Get the schema
        let df_schema = catalog.schema(namespace_str).ok_or_else(|| {
            KalamDbError::Other(format!(
                "Schema '{}' not found after registration",
                namespace_str
            ))
        })?;

        // Create minimal table metadata for registration
        let table_metadata = crate::catalog::TableMetadata::new(
            table_name.clone(),
            table_type,
            namespace_id.clone(),
            String::new(), // Storage location will be loaded from metadata
        );

        match table_type {
            TableType::User => {
                let store = self.user_table_store.as_ref().ok_or_else(|| {
                    KalamDbError::InvalidOperation("UserTableStore not configured".to_string())
                })?;

                // TODO: Get current_user_id from execution context
                let current_user_id = UserId::from("system");

                let provider = Arc::new(UserTableProvider::new(
                    table_metadata,
                    schema,
                    store.clone(),
                    current_user_id,
                    vec![], // parquet_paths - empty for now
                ));

                df_schema
                    .register_table(table_name.as_str().to_string(), provider)
                    .map_err(|e| {
                        KalamDbError::Other(format!("Failed to register user table: {}", e))
                    })?;
            }
            TableType::Shared => {
                let store = self.shared_table_store.as_ref().ok_or_else(|| {
                    KalamDbError::InvalidOperation("SharedTableStore not configured".to_string())
                })?;

                let provider = Arc::new(SharedTableProvider::new(
                    table_metadata,
                    schema,
                    store.clone(),
                ));

                df_schema
                    .register_table(table_name.as_str().to_string(), provider)
                    .map_err(|e| {
                        KalamDbError::Other(format!("Failed to register shared table: {}", e))
                    })?;
            }
            TableType::Stream => {
                let store = self.stream_table_store.as_ref().ok_or_else(|| {
                    KalamDbError::InvalidOperation("StreamTableStore not configured".to_string())
                })?;

                let provider = Arc::new(StreamTableProvider::new(
                    table_metadata,
                    schema,
                    store.clone(),
                    None,  // retention_seconds - TODO: get from table metadata
                    false, // ephemeral - TODO: get from table metadata
                    None,  // max_buffer - TODO: get from table metadata
                ));

                df_schema
                    .register_table(table_name.as_str().to_string(), provider)
                    .map_err(|e| {
                        KalamDbError::Other(format!("Failed to register stream table: {}", e))
                    })?;
            }
            TableType::System => {
                // System tables are already registered at startup
                return Ok(());
            }
        }

        Ok(())
    }

    /// Execute a SQL statement
    pub async fn execute(
        &self,
        sql: &str,
        user_id: Option<&UserId>,
    ) -> Result<ExecutionResult, KalamDbError> {
        let sql_upper = sql.trim().to_uppercase();

        // Try to parse as DDL first
        if sql_upper.starts_with("CREATE NAMESPACE") {
            return self.execute_create_namespace(sql).await;
        } else if sql_upper.starts_with("SHOW NAMESPACES") {
            return self.execute_show_namespaces(sql).await;
        } else if sql_upper.starts_with("SHOW TABLES") {
            return self.execute_show_tables(sql).await;
        } else if sql_upper.starts_with("SHOW STATS") {
            return self.execute_show_table_stats(sql).await;
        } else if sql_upper.starts_with("DESCRIBE TABLE") || sql_upper.starts_with("DESC TABLE") {
            return self.execute_describe_table(sql).await;
        } else if sql_upper.starts_with("ALTER NAMESPACE") {
            return self.execute_alter_namespace(sql).await;
        } else if sql_upper.starts_with("DROP NAMESPACE") {
            return self.execute_drop_namespace(sql).await;
        } else if sql_upper.starts_with("CREATE TABLE")
            || sql_upper.starts_with("CREATE USER TABLE")
            || sql_upper.starts_with("CREATE SHARED TABLE")
            || sql_upper.starts_with("CREATE STREAM TABLE")
        {
            return self.execute_create_table(sql, user_id).await;
        } else if sql_upper.starts_with("DROP TABLE") {
            return self.execute_drop_table(sql).await;
        } else if sql_upper.starts_with("SELECT")
            || sql_upper.starts_with("INSERT")
            || sql_upper.starts_with("UPDATE")
            || sql_upper.starts_with("DELETE")
        {
            // Delegate DML queries to DataFusion
            return self.execute_datafusion_query(sql).await;
        }

        // Otherwise, unsupported
        Err(KalamDbError::InvalidSql(format!(
            "Unsupported SQL statement: {}",
            sql.lines().next().unwrap_or("")
        )))
    }
    /// Execute query via DataFusion
    async fn execute_datafusion_query(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        // Execute SQL via DataFusion
        let df = self
            .session_context
            .sql(sql)
            .await
            .map_err(|e| KalamDbError::Other(format!("Error planning query: {}", e)))?;

        // Collect results
        let batches = df
            .collect()
            .await
            .map_err(|e| KalamDbError::Other(format!("Error executing query: {}", e)))?;

        // Return batches
        if batches.is_empty() {
            // Return empty result with schema
            Ok(ExecutionResult::RecordBatches(vec![]))
        } else if batches.len() == 1 {
            Ok(ExecutionResult::RecordBatch(batches[0].clone()))
        } else {
            Ok(ExecutionResult::RecordBatches(batches))
        }
    }

    /// Execute CREATE NAMESPACE
    async fn execute_create_namespace(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = CreateNamespaceStatement::parse(sql)?;
        let created = self
            .namespace_service
            .create(stmt.name.clone(), stmt.if_not_exists)?;

        let message = if created {
            format!("Namespace '{}' created successfully", stmt.name.as_str())
        } else {
            format!("Namespace '{}' already exists", stmt.name.as_str())
        };

        Ok(ExecutionResult::Success(message))
    }

    /// Execute SHOW NAMESPACES
    async fn execute_show_namespaces(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let _stmt = ShowNamespacesStatement::parse(sql)?;
        let namespaces = self.namespace_service.list()?;

        // Convert to RecordBatch
        let batch = Self::namespaces_to_record_batch(namespaces)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute SHOW TABLES
    async fn execute_show_tables(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = ShowTablesStatement::parse(sql)?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Scan all tables from kalam_sql
        let tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        // Filter by namespace if specified
        let filtered_tables: Vec<_> = if let Some(namespace_id) = &stmt.namespace_id {
            tables
                .into_iter()
                .filter(|t| t.namespace == namespace_id.as_str())
                .collect()
        } else {
            tables
        };

        // Convert to RecordBatch
        let batch = Self::tables_to_record_batch(filtered_tables)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute DESCRIBE TABLE
    async fn execute_describe_table(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = DescribeTableStatement::parse(sql)?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Get all tables to find the requested one
        let tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        // Find the table (match by name and optionally namespace)
        let table = tables
            .into_iter()
            .find(|t| {
                let name_matches = t.table_name == stmt.table_name.as_str();
                let namespace_matches = stmt
                    .namespace_id
                    .as_ref()
                    .map(|ns| t.namespace == ns.as_str())
                    .unwrap_or(true);
                name_matches && namespace_matches
            })
            .ok_or_else(|| {
                KalamDbError::NotFound(format!("Table '{}' not found", stmt.table_name.as_str()))
            })?;

        // Convert to detailed RecordBatch with table metadata
        let batch = Self::table_details_to_record_batch(&table)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute SHOW STATS FOR TABLE
    async fn execute_show_table_stats(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = ShowTableStatsStatement::parse(sql)?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Get all tables to find the requested one
        let tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        // Find the table (match by name and optionally namespace)
        let table = tables
            .into_iter()
            .find(|t| {
                let name_matches = t.table_name == stmt.table_name.as_str();
                let namespace_matches = stmt
                    .namespace_id
                    .as_ref()
                    .map(|ns| t.namespace == ns.as_str())
                    .unwrap_or(true);
                name_matches && namespace_matches
            })
            .ok_or_else(|| {
                KalamDbError::NotFound(format!("Table '{}' not found", stmt.table_name.as_str()))
            })?;

        // For now, return basic statistics
        // In Phase 16, this would query actual row counts from hot/cold storage
        let batch = Self::table_stats_to_record_batch(&table)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute ALTER NAMESPACE
    async fn execute_alter_namespace(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = AlterNamespaceStatement::parse(sql)?;
        self.namespace_service
            .update_options(stmt.name.clone(), stmt.options)?;

        let message = format!("Namespace '{}' updated successfully", stmt.name.as_str());

        Ok(ExecutionResult::Success(message))
    }

    /// Execute DROP NAMESPACE
    async fn execute_drop_namespace(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = DropNamespaceStatement::parse(sql)?;
        let deleted = self
            .namespace_service
            .delete(stmt.name.clone(), stmt.if_exists)?;

        let message = if deleted {
            format!("Namespace '{}' dropped successfully", stmt.name.as_str())
        } else {
            format!("Namespace '{}' does not exist", stmt.name.as_str())
        };

        Ok(ExecutionResult::Success(message))
    }

    /// Execute CREATE TABLE - determines table type from LOCATION and routes to appropriate service
    async fn execute_create_table(
        &self,
        sql: &str,
        user_id: Option<&UserId>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Determine table type based on SQL keywords or LOCATION pattern
        let sql_upper = sql.to_uppercase();
        let namespace_id = crate::catalog::NamespaceId::new("default"); // TODO: Get from context
        let default_user_id = user_id
            .cloned()
            .unwrap_or_else(|| crate::catalog::UserId::from("system"));

        if sql_upper.contains("USER TABLE") || sql_upper.contains("${USER_ID}") {
            // User table - requires user_id
            let actual_user_id = user_id.ok_or_else(|| {
                KalamDbError::InvalidOperation(
                    "CREATE USER TABLE requires X-USER-ID header to be set".to_string(),
                )
            })?;

            let stmt = CreateUserTableStatement::parse(sql, &namespace_id)?;
            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone(); // Use the namespace from the statement
            let schema = stmt.schema.clone();
            let flush_policy = stmt.flush_policy.clone();
            let deleted_retention_hours = stmt.deleted_retention_hours;

            let metadata = self.user_table_service.create_table(stmt, None)?;

            // Insert into system.tables via KalamSQL
            if let Some(kalam_sql) = &self.kalam_sql {
                let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());
                let table = kalamdb_sql::models::Table {
                    table_id,
                    table_name: table_name.as_str().to_string(),
                    namespace: namespace_id.as_str().to_string(),
                    table_type: "user".to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    storage_location: metadata.storage_location.clone(),
                    flush_policy: serde_json::to_string(&flush_policy.unwrap_or_default())
                        .unwrap_or_else(|_| "{}".to_string()),
                    schema_version: 1,
                    deleted_retention_hours: deleted_retention_hours.unwrap_or(0) as i32,
                };
                kalam_sql.insert_table(&table).map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to insert table into system catalog: {}",
                        e
                    ))
                })?;
            }

            // Register with DataFusion if stores are configured
            if self.user_table_store.is_some() {
                self.register_table_with_datafusion(
                    &namespace_id,
                    &table_name,
                    crate::catalog::TableType::User,
                    schema,
                    default_user_id,
                )
                .await?;
            }

            Ok(ExecutionResult::Success(
                "User table created successfully".to_string(),
            ))
        } else if sql_upper.contains("STREAM TABLE")
            || sql_upper.contains("TTL")
            || sql_upper.contains("BUFFER_SIZE")
        {
            // Stream table
            let stmt = CreateStreamTableStatement::parse(sql, &namespace_id)?;
            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone(); // Use the namespace from the statement
            let schema = stmt.schema.clone();
            let retention_seconds = stmt.retention_seconds;

            let metadata = self.stream_table_service.create_table(stmt)?;

            // Insert into system.tables via KalamSQL
            if let Some(kalam_sql) = &self.kalam_sql {
                let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());
                let table = kalamdb_sql::models::Table {
                    table_id,
                    table_name: table_name.as_str().to_string(),
                    namespace: namespace_id.as_str().to_string(),
                    table_type: "stream".to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    storage_location: metadata.storage_location.clone(),
                    flush_policy: serde_json::to_string(&metadata.flush_policy)
                        .unwrap_or_else(|_| "{}".to_string()),
                    schema_version: 1,
                    deleted_retention_hours: retention_seconds
                        .map(|s| (s / 3600) as i32)
                        .unwrap_or(0),
                };
                kalam_sql.insert_table(&table).map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to insert table into system catalog: {}",
                        e
                    ))
                })?;
            }

            // Register with DataFusion if stores are configured
            if self.stream_table_store.is_some() {
                self.register_table_with_datafusion(
                    &namespace_id,
                    &table_name,
                    crate::catalog::TableType::Stream,
                    schema,
                    default_user_id,
                )
                .await?;
            }

            Ok(ExecutionResult::Success(
                "Stream table created successfully".to_string(),
            ))
        } else {
            // Default to shared table (most common case)
            let stmt = CreateSharedTableStatement::parse(sql, &namespace_id)?;
            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone(); // Use the namespace from the statement
            let schema = stmt.schema.clone();
            let flush_policy = stmt.flush_policy.clone();
            let deleted_retention = stmt.deleted_retention;

            let metadata = self.shared_table_service.create_table(stmt)?;

            // Insert into system.tables via KalamSQL
            if let Some(kalam_sql) = &self.kalam_sql {
                let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());

                // Convert local FlushPolicy to the global one for serialization
                let flush_policy_json = if let Some(fp) = flush_policy {
                    match fp {
                        crate::sql::ddl::create_shared_table::FlushPolicy::Rows(n) => {
                            serde_json::json!({"type": "row_limit", "row_limit": n}).to_string()
                        }
                        crate::sql::ddl::create_shared_table::FlushPolicy::Time(s) => {
                            serde_json::json!({"type": "time_interval", "interval_seconds": s}).to_string()
                        }
                        crate::sql::ddl::create_shared_table::FlushPolicy::Combined { rows, seconds } => {
                            serde_json::json!({"type": "combined", "row_limit": rows, "interval_seconds": seconds}).to_string()
                        }
                    }
                } else {
                    "{}".to_string()
                };

                let table = kalamdb_sql::models::Table {
                    table_id,
                    table_name: table_name.as_str().to_string(),
                    namespace: namespace_id.as_str().to_string(),
                    table_type: "shared".to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    storage_location: metadata.storage_location.clone(),
                    flush_policy: flush_policy_json,
                    schema_version: 1,
                    deleted_retention_hours: deleted_retention
                        .map(|s| (s / 3600) as i32)
                        .unwrap_or(0),
                };
                kalam_sql.insert_table(&table).map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to insert table into system catalog: {}",
                        e
                    ))
                })?;
            }

            // Register with DataFusion if stores are configured
            if self.shared_table_store.is_some() {
                self.register_table_with_datafusion(
                    &namespace_id,
                    &table_name,
                    crate::catalog::TableType::Shared,
                    schema,
                    default_user_id,
                )
                .await?;
            }

            Ok(ExecutionResult::Success(
                "Table created successfully".to_string(),
            ))
        }
    }

    /// Execute DROP TABLE
    async fn execute_drop_table(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        // Check if table deletion service is available
        let deletion_service = self.table_deletion_service.as_ref().ok_or_else(|| {
            KalamDbError::InvalidOperation(
                "DROP TABLE not available - table deletion service not configured".to_string(),
            )
        })?;

        let namespace_id = crate::catalog::NamespaceId::new("default"); // TODO: Get from context
        let stmt = DropTableStatement::parse(sql, &namespace_id)?;

        // Execute the drop operation
        let result = deletion_service.drop_table(
            &stmt.namespace_id,
            &stmt.table_name,
            stmt.table_type,
            stmt.if_exists,
        )?;

        // If if_exists is true and table didn't exist, return success message
        if result.is_none() {
            return Ok(ExecutionResult::Success(format!(
                "Table {}.{} does not exist (skipped)",
                stmt.namespace_id, stmt.table_name
            )));
        }

        let deletion_result = result.unwrap();
        Ok(ExecutionResult::Success(format!(
            "Table {}.{} dropped successfully ({} Parquet files deleted, {} bytes freed)",
            stmt.namespace_id,
            stmt.table_name,
            deletion_result.files_deleted,
            deletion_result.bytes_freed
        )))
    }

    /// Convert namespaces list to Arrow RecordBatch
    fn namespaces_to_record_batch(namespaces: Vec<Namespace>) -> Result<RecordBatch, KalamDbError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("table_count", DataType::UInt32, false),
            Field::new("created_at", DataType::Utf8, false),
        ]));

        let names: ArrayRef = Arc::new(StringArray::from(
            namespaces
                .iter()
                .map(|ns| ns.name.as_str())
                .collect::<Vec<_>>(),
        ));

        let table_counts: ArrayRef = Arc::new(UInt32Array::from(
            namespaces
                .iter()
                .map(|ns| ns.table_count)
                .collect::<Vec<_>>(),
        ));

        let created_at: ArrayRef = Arc::new(StringArray::from(
            namespaces
                .iter()
                .map(|ns| ns.created_at.to_rfc3339())
                .collect::<Vec<_>>(),
        ));

        RecordBatch::try_new(schema, vec![names, table_counts, created_at])
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    /// Convert table list to RecordBatch for SHOW TABLES
    fn tables_to_record_batch(
        tables: Vec<kalamdb_sql::Table>,
    ) -> Result<RecordBatch, KalamDbError> {
        

        let schema = Arc::new(Schema::new(vec![
            Field::new("namespace", DataType::Utf8, false),
            Field::new("table_name", DataType::Utf8, false),
            Field::new("table_type", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
        ]));

        let namespaces: ArrayRef = Arc::new(StringArray::from(
            tables
                .iter()
                .map(|t| t.namespace.as_str())
                .collect::<Vec<_>>(),
        ));

        let names: ArrayRef = Arc::new(StringArray::from(
            tables
                .iter()
                .map(|t| t.table_name.as_str())
                .collect::<Vec<_>>(),
        ));

        let types: ArrayRef = Arc::new(StringArray::from(
            tables
                .iter()
                .map(|t| t.table_type.as_str())
                .collect::<Vec<_>>(),
        ));

        let created_at: ArrayRef = Arc::new(StringArray::from(
            tables
                .iter()
                .map(|t| {
                    chrono::DateTime::from_timestamp_millis(t.created_at)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| "unknown".to_string())
                })
                .collect::<Vec<_>>(),
        ));

        RecordBatch::try_new(schema, vec![namespaces, names, types, created_at])
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    /// Convert table details to RecordBatch for DESCRIBE TABLE
    fn table_details_to_record_batch(
        table: &kalamdb_sql::Table,
    ) -> Result<RecordBatch, KalamDbError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("property", DataType::Utf8, false),
            Field::new("value", DataType::Utf8, false),
        ]));

        let properties = vec![
            "table_id",
            "table_name",
            "namespace",
            "table_type",
            "storage_location",
            "flush_policy",
            "schema_version",
            "deleted_retention_hours",
            "created_at",
        ];

        let created_at_str = chrono::DateTime::from_timestamp_millis(table.created_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "unknown".to_string());

        let schema_version_str = table.schema_version.to_string();
        let retention_hours_str = table.deleted_retention_hours.to_string();

        let values = vec![
            table.table_id.as_str(),
            table.table_name.as_str(),
            table.namespace.as_str(),
            table.table_type.as_str(),
            table.storage_location.as_str(),
            table.flush_policy.as_str(),
            &schema_version_str,
            &retention_hours_str,
            &created_at_str,
        ];

        let property_array: ArrayRef = Arc::new(StringArray::from(properties));
        let value_array: ArrayRef = Arc::new(StringArray::from(values));

        RecordBatch::try_new(schema, vec![property_array, value_array])
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    /// Convert table statistics to RecordBatch for SHOW STATS
    fn table_stats_to_record_batch(
        table: &kalamdb_sql::Table,
    ) -> Result<RecordBatch, KalamDbError> {
        

        let schema = Arc::new(Schema::new(vec![
            Field::new("statistic", DataType::Utf8, false),
            Field::new("value", DataType::Utf8, false),
        ]));

        // For Phase 16, we return metadata-based statistics
        // Future phases can query actual row counts from hot/cold storage
        let statistics = vec![
            "table_name",
            "namespace",
            "table_type",
            "storage_location",
            "schema_version",
            "created_at",
            "status",
        ];

        let created_at_str = chrono::DateTime::from_timestamp_millis(table.created_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "unknown".to_string());

        let schema_version_str = table.schema_version.to_string();

        let values = vec![
            table.table_name.as_str(),
            table.namespace.as_str(),
            table.table_type.as_str(),
            table.storage_location.as_str(),
            &schema_version_str,
            &created_at_str,
            "active", // Placeholder - could query actual status
        ];

        let statistic_array: ArrayRef = Arc::new(StringArray::from(statistics));
        let value_array: ArrayRef = Arc::new(StringArray::from(values));

        RecordBatch::try_new(schema, vec![statistic_array, value_array])
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::prelude::SessionContext;
    use kalamdb_sql::KalamSql;
    use kalamdb_store::test_utils::TestDb;

    fn setup_test_executor() -> SqlExecutor {
        let test_db =
            TestDb::new(&["system_namespaces", "system_tables", "system_table_schemas"]).unwrap();

        let kalam_sql = Arc::new(KalamSql::new(test_db.db.clone()).unwrap());
        let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
        let session_context = Arc::new(SessionContext::new());

        // Initialize table services for tests
        let user_table_service =
            Arc::new(crate::services::UserTableService::new(kalam_sql.clone()));
        let shared_table_service = Arc::new(crate::services::SharedTableService::new(
            Arc::new(kalamdb_store::SharedTableStore::new(test_db.db.clone()).unwrap()),
            kalam_sql.clone(),
        ));
        let stream_table_service = Arc::new(crate::services::StreamTableService::new(
            Arc::new(kalamdb_store::StreamTableStore::new(test_db.db.clone()).unwrap()),
            kalam_sql.clone(),
        ));

        SqlExecutor::new(
            namespace_service,
            session_context,
            user_table_service,
            shared_table_service,
            stream_table_service,
        )
    }

    #[tokio::test]
    async fn test_execute_create_namespace() {
        let executor = setup_test_executor();

        let result = executor
            .execute("CREATE NAMESPACE app", None)
            .await
            .unwrap();

        match result {
            ExecutionResult::Success(msg) => {
                assert!(msg.contains("created successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_execute_show_namespaces() {
        let executor = setup_test_executor();

        // Create some namespaces
        executor
            .execute("CREATE NAMESPACE app1", None)
            .await
            .unwrap();
        executor
            .execute("CREATE NAMESPACE app2", None)
            .await
            .unwrap();

        let result = executor.execute("SHOW NAMESPACES", None).await.unwrap();

        match result {
            ExecutionResult::RecordBatch(batch) => {
                assert_eq!(batch.num_rows(), 2);
                assert_eq!(batch.num_columns(), 3);
            }
            _ => panic!("Expected RecordBatch result"),
        }
    }

    #[tokio::test]
    async fn test_execute_alter_namespace() {
        let executor = setup_test_executor();

        executor
            .execute("CREATE NAMESPACE app", None)
            .await
            .unwrap();

        let result = executor
            .execute("ALTER NAMESPACE app SET OPTIONS (enabled = true)", None)
            .await
            .unwrap();

        match result {
            ExecutionResult::Success(msg) => {
                assert!(msg.contains("updated successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_execute_drop_namespace() {
        let executor = setup_test_executor();

        executor
            .execute("CREATE NAMESPACE app", None)
            .await
            .unwrap();

        let result = executor.execute("DROP NAMESPACE app", None).await.unwrap();

        match result {
            ExecutionResult::Success(msg) => {
                assert!(msg.contains("dropped successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_execute_drop_namespace_with_tables() {
        let executor = setup_test_executor();

        executor
            .execute("CREATE NAMESPACE app", None)
            .await
            .unwrap();

        // Simulate adding a table
        executor
            .namespace_service
            .increment_table_count("app")
            .unwrap();

        let result = executor.execute("DROP NAMESPACE app", None).await;
        assert!(result.is_err());
    }
}
