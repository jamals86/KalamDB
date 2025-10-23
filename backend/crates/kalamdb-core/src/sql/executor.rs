//! SQL executor for DDL and DML statements
//!
//! This module provides execution logic for SQL statements,
//! coordinating between parsers, services, and DataFusion.
//!
//! # Security: SQL Injection Prevention
//!
//! KalamDB uses Apache DataFusion for SQL query execution, which provides built-in
//! protection against SQL injection attacks through:
//!
//! 1. **Parameterized Query API**: DataFusion's SessionContext API uses structured
//!    parsing and execution plans, not string concatenation. All user input goes
//!    through the sqlparser crate's AST (Abstract Syntax Tree) parser, which
//!    tokenizes and validates SQL syntax before execution.
//!
//! 2. **Type-Safe Execution**: DataFusion's logical and physical plan execution
//!    operates on strongly-typed Arrow data structures. User input is validated
//!    against schema types before query execution.
//!
//! 3. **No Dynamic SQL Construction**: The executor does not concatenate user input
//!    into SQL strings. All SQL statements are parsed as complete units through
//!    `ctx.sql()` which uses the sqlparser crate's tokenizer.
//!
//! 4. **Statement Isolation**: Multiple statements are split and executed individually,
//!    preventing SQL injection through statement terminators.
//!
//! This architecture makes traditional SQL injection (e.g., `'; DROP TABLE users; --`)
//! impossible because malicious input is treated as literal data values, not executable SQL.

use crate::catalog::Namespace;
use crate::catalog::{NamespaceId, TableMetadata, TableName, TableType, UserId};
use kalamdb_commons::models::StorageId;
use kalamdb_sql::statement_classifier::SqlStatement;
use crate::error::KalamDbError;
use crate::schema::arrow_schema::ArrowSchemaWithOptions;
use crate::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::tables::{
    system::{
        NamespacesTableProvider, SystemTablesTableProvider,
        UsersTableProvider as SystemUsersTableProvider,
    },
    SharedTableProvider, StreamTableProvider, UserTableProvider,
};
use datafusion::arrow::array::{ArrayRef, StringArray, UInt32Array};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::prelude::SessionContext;
use datafusion::sql::sqlparser;
use kalamdb_sql::ddl::{
    AlterNamespaceStatement, CreateNamespaceStatement, CreateTableStatement,
    DescribeTableStatement, DropNamespaceStatement, DropTableStatement, FlushPolicy as DdlFlushPolicy,
    ShowNamespacesStatement, ShowTableStatsStatement, ShowTablesStatement, parse_job_command,
};
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

    /// Subscription metadata (for SUBSCRIBE TO commands)
    Subscription(serde_json::Value),
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
    // Session factory for creating per-user sessions
    session_factory: DataFusionSessionFactory,
    // System tables
    jobs_table_provider: Option<Arc<crate::tables::system::JobsTableProvider>>,
    // Storage registry for template validation
    storage_registry: Option<Arc<crate::storage::StorageRegistry>>,
    // Job manager for flush operations
    job_manager: Option<Arc<dyn crate::jobs::JobManager>>,
}

/// UPDATE statement parsed info
#[derive(Debug)]
struct UpdateInfo {
    namespace: String,
    table: String,
    set_values: Vec<(String, serde_json::Value)>,
    where_clause: String,
}

/// DELETE statement parsed info
#[derive(Debug)]
struct DeleteInfo {
    namespace: String,
    table: String,
    where_clause: String,
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
        let session_factory = DataFusionSessionFactory::default();

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
            session_factory,
            jobs_table_provider: None,
            storage_registry: None,
            job_manager: None,
        }
    }

    /// Set the storage registry (optional, for storage template validation)
    pub fn with_storage_registry(mut self, registry: Arc<crate::storage::StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
    }

    /// Set the job manager (optional, for FLUSH TABLE support)
    pub fn with_job_manager(mut self, job_manager: Arc<dyn crate::jobs::JobManager>) -> Self {
        self.job_manager = Some(job_manager);
        self
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
        self.jobs_table_provider = Some(Arc::new(crate::tables::system::JobsTableProvider::new(
            kalam_sql.clone(),
        )));
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
                // For user tables, DO NOT register with DataFusion at CREATE TABLE time
                // because the provider needs the actual user_id from each query request.
                // User tables will be registered dynamically at query time via reregister_user_tables_for_user()

                // Just return Ok - the table metadata is already in system.tables,
                // schema is in system.table_schemas, and column family is created.
                // That's all we need for CREATE TABLE.
                return Ok(());
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

                // Check if table already exists
                let table_exists = df_schema.table_exist(table_name.as_str());

                if table_exists {
                    // Deregister the old table first
                    df_schema
                        .deregister_table(table_name.as_str())
                        .map_err(|e| {
                            KalamDbError::Other(format!("Failed to deregister shared table: {}", e))
                        })?;
                }

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

                // Check if table already exists
                let table_exists = df_schema.table_exist(table_name.as_str());

                if table_exists {
                    // Deregister the old table first
                    df_schema
                        .deregister_table(table_name.as_str())
                        .map_err(|e| {
                            KalamDbError::Other(format!("Failed to deregister stream table: {}", e))
                        })?;
                }

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

    /// Helper: Validate and resolve storage_id for table creation
    /// 
    /// # Arguments
    /// * `storage_id` - Optional storage_id from the CREATE TABLE statement
    /// 
    /// # Returns
    /// Resolved storage_id (defaults to 'local' if None)
    /// 
    /// # Errors
    /// Returns error if storage_id doesn't exist in system.storages
    fn validate_storage_id(
        &self,
        storage_id: Option<StorageId>,
    ) -> Result<StorageId, KalamDbError> {
        // T167a: Default to 'local' if not specified
        let storage_id = storage_id.unwrap_or_else(|| StorageId::local());

        // T167b & T167c: Validate storage_id exists in system.storages (NOT NULL enforcement)
        if let Some(kalam_sql) = &self.kalam_sql {
            let storage_exists = kalam_sql.get_storage(storage_id.as_str()).is_ok();
            if !storage_exists {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Storage '{}' does not exist in system.storages",
                    storage_id
                )));
            }
        }

        Ok(storage_id)
    }

    /// Execute a SQL statement
    pub async fn execute(
        &self,
        sql: &str,
        user_id: Option<&UserId>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Classify the SQL statement and dispatch to appropriate handler
        match SqlStatement::classify(sql) {
            SqlStatement::CreateNamespace => self.execute_create_namespace(sql).await,
            SqlStatement::AlterNamespace => self.execute_alter_namespace(sql).await,
            SqlStatement::DropNamespace => self.execute_drop_namespace(sql).await,
            SqlStatement::ShowNamespaces => self.execute_show_namespaces(sql).await,
            SqlStatement::CreateStorage => self.execute_create_storage(sql).await,
            SqlStatement::AlterStorage => self.execute_alter_storage(sql).await,
            SqlStatement::DropStorage => self.execute_drop_storage(sql).await,
            SqlStatement::ShowStorages => self.execute_show_storages(sql, user_id).await,
            SqlStatement::CreateTable => self.execute_create_table(sql, user_id).await,
            SqlStatement::DropTable => self.execute_drop_table(sql).await,
            SqlStatement::ShowTables => self.execute_show_tables(sql).await,
            SqlStatement::DescribeTable => self.execute_describe_table(sql).await,
            SqlStatement::ShowStats => self.execute_show_table_stats(sql).await,
            SqlStatement::FlushTable => self.execute_flush_table(sql).await,
            SqlStatement::FlushAllTables => self.execute_flush_all_tables(sql).await,
            SqlStatement::KillJob => self.execute_kill_job(sql).await,
            SqlStatement::Subscribe => self.execute_subscribe(sql).await,
            SqlStatement::Update => self.execute_update(sql, user_id).await,
            SqlStatement::Delete => self.execute_delete(sql, user_id).await,
            SqlStatement::Select | SqlStatement::Insert => {
                // Extract referenced tables from SQL and load only those
                let referenced_tables = Self::extract_table_references(sql);
                self.execute_datafusion_query_with_tables(sql, user_id, referenced_tables)
                    .await
            }
            SqlStatement::Unknown => Err(KalamDbError::InvalidSql(format!(
                "Unsupported SQL statement: {}",
                sql.lines().next().unwrap_or("")
            ))),
        }
    }

    /// Extract table references from SQL query
    ///
    /// Returns a set of fully qualified table names (namespace.table_name) or "system.*" for system tables.
    /// If parsing fails or no tables found, returns None (will load all tables as fallback).
    fn extract_table_references(sql: &str) -> Option<std::collections::HashSet<String>> {
        use sqlparser::ast::Statement;
        use sqlparser::dialect::GenericDialect;
        use sqlparser::parser::Parser;

        let dialect = GenericDialect {};
        let parsed = Parser::parse_sql(&dialect, sql).ok()?;

        let mut tables = std::collections::HashSet::new();

        for statement in parsed {
            match statement {
                Statement::Query(query) => {
                    // Extract from SELECT body
                    Self::extract_tables_from_set_expr(&query.body, &mut tables);
                }
                Statement::Insert(insert) => {
                    // INSERT INTO table_name
                    let table_str = insert.table_name.to_string();
                    tables.insert(table_str);
                }
                _ => {}
            }
        }

        if tables.is_empty() {
            None
        } else {
            Some(tables)
        }
    }

    /// Helper to recursively extract table names from SetExpr
    fn extract_tables_from_set_expr(
        set_expr: &sqlparser::ast::SetExpr,
        tables: &mut std::collections::HashSet<String>,
    ) {
        use sqlparser::ast::SetExpr;

        match set_expr {
            SetExpr::Select(select) => {
                // Extract from FROM clause
                for table_with_joins in &select.from {
                    Self::extract_table_from_factor(&table_with_joins.relation, tables);

                    // Also check JOINs
                    for join in &table_with_joins.joins {
                        Self::extract_table_from_factor(&join.relation, tables);
                    }
                }
            }
            SetExpr::Query(query) => {
                // Recursive for subqueries
                Self::extract_tables_from_set_expr(&query.body, tables);
            }
            SetExpr::SetOperation { left, right, .. } => {
                // UNION, INTERSECT, EXCEPT
                Self::extract_tables_from_set_expr(left, tables);
                Self::extract_tables_from_set_expr(right, tables);
            }
            _ => {}
        }
    }

    /// Helper to extract table name from TableFactor
    fn extract_table_from_factor(
        factor: &sqlparser::ast::TableFactor,
        tables: &mut std::collections::HashSet<String>,
    ) {
        use sqlparser::ast::TableFactor;

        match factor {
            TableFactor::Table { name, .. } => {
                let table_str = name.to_string();
                tables.insert(table_str);
            }
            TableFactor::Derived { subquery, .. } => {
                // Subquery - extract from it
                Self::extract_tables_from_set_expr(&subquery.body, tables);
            }
            TableFactor::TableFunction { .. } | TableFactor::UNNEST { .. } => {
                // Skip table functions for now
            }
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => {
                // Nested join
                Self::extract_table_from_factor(&table_with_joins.relation, tables);
                for join in &table_with_joins.joins {
                    Self::extract_table_from_factor(&join.relation, tables);
                }
            }
            _ => {}
        }
    }

    /// Check if a query only accesses system tables (no user/shared/stream tables)
    ///
    /// This allows us to use a lightweight session that doesn't load all user tables.
    fn is_system_only_query(sql_upper: &str) -> bool {
        // Check if query contains FROM or JOIN with system.* tables only
        // Simple heuristic: if it contains "FROM SYSTEM." or "JOIN SYSTEM." and no other FROM/JOIN
        let has_system_table =
            sql_upper.contains("FROM SYSTEM.") || sql_upper.contains("JOIN SYSTEM.");

        if !has_system_table {
            return false;
        }

        // Check if there are any non-system table references
        // This is a simple check - could be more sophisticated
        let words: Vec<&str> = sql_upper.split_whitespace().collect();
        for i in 0..words.len() {
            if (words[i] == "FROM" || words[i] == "JOIN") && i + 1 < words.len() {
                let table_ref = words[i + 1];
                // If it's not a system.* reference and not a subquery, it's a user table
                if !table_ref.starts_with("SYSTEM.") && !table_ref.starts_with("(") {
                    return false;
                }
            }
        }

        true
    }

    /// Execute a query that only accesses system tables (optimized - no user table loading)
    async fn execute_system_query(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        // Create a lightweight session with ONLY system tables registered
        let session = self.session_factory.create_session();

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("KalamSQL not configured".to_string()))?;

        // Register only system tables (no user table scanning!)
        self.register_system_tables_in_session(&session, kalam_sql)?;

        // Execute SQL via DataFusion
        let df = session.sql(sql).await.map_err(|e| {
            // Extract the root cause to avoid "Error planning query: Error during planning:" duplication
            let err_msg = e.to_string();
            if err_msg.contains("Error during planning:") {
                // DataFusion already says "Error during planning", don't duplicate
                KalamDbError::Other(err_msg)
            } else {
                KalamDbError::Other(format!("Error planning query: {}", err_msg))
            }
        })?;

        // Collect results
        let batches = df
            .collect()
            .await
            .map_err(|e| KalamDbError::Other(format!("Error executing query: {}", e)))?;

        // Return batches
        if batches.is_empty() {
            Ok(ExecutionResult::RecordBatches(vec![]))
        } else if batches.len() == 1 {
            Ok(ExecutionResult::RecordBatch(batches[0].clone()))
        } else {
            Ok(ExecutionResult::RecordBatches(batches))
        }
    }

    /// Create a fresh SessionContext for a specific user with only their tables registered
    ///
    /// This ensures user data isolation by creating a dedicated session with user-scoped TableProviders.
    fn register_system_tables_in_session(
        &self,
        session: &SessionContext,
        kalam_sql: &Arc<KalamSql>,
    ) -> Result<(), KalamDbError> {
        use datafusion::catalog::schema::MemorySchemaProvider;

        let catalog_name = "kalam";
        let catalog = session
            .catalog(catalog_name)
            .ok_or_else(|| KalamDbError::Other(format!("Catalog '{}' not found", catalog_name)))?;

        if catalog.schema("system").is_none() {
            let system_schema = Arc::new(MemorySchemaProvider::new());
            catalog
                .register_schema("system", system_schema)
                .map_err(|e| {
                    KalamDbError::Other(format!("Failed to register system schema: {}", e))
                })?;
        }

        let system_schema = catalog.schema("system").ok_or_else(|| {
            KalamDbError::Other("System schema not found after registration".to_string())
        })?;

        system_schema
            .register_table(
                "namespaces".to_string(),
                Arc::new(NamespacesTableProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to register system.namespaces: {}", e))
            })?;

        system_schema
            .register_table(
                "tables".to_string(),
                Arc::new(SystemTablesTableProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to register system.tables: {}", e)))?;

        system_schema
            .register_table(
                "users".to_string(),
                Arc::new(SystemUsersTableProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to register system.users: {}", e)))?;

        // Register additional system tables
        use crate::tables::system::{
            JobsTableProvider, LiveQueriesTableProvider, StorageLocationsTableProvider,
            SystemStoragesProvider,
        };

        system_schema
            .register_table(
                "storages".to_string(),
                Arc::new(SystemStoragesProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to register system.storages: {}", e))
            })?;

        system_schema
            .register_table(
                "storage_locations".to_string(),
                Arc::new(StorageLocationsTableProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to register system.storage_locations: {}",
                    e
                ))
            })?;

        system_schema
            .register_table(
                "live_queries".to_string(),
                Arc::new(LiveQueriesTableProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to register system.live_queries: {}", e))
            })?;

        system_schema
            .register_table(
                "jobs".to_string(),
                Arc::new(JobsTableProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to register system.jobs: {}", e)))?;

        Ok(())
    }

    async fn create_user_session_context(
        &self,
        user_id: &UserId,
        table_filter: Option<&std::collections::HashSet<String>>,
    ) -> Result<SessionContext, KalamDbError> {
        use datafusion::catalog::schema::MemorySchemaProvider;

        // Create fresh SessionContext using the shared RuntimeEnv (efficient - no memory duplication)
        let user_session = self.session_factory.create_session();

        // Get KalamSQL for querying system tables
        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("KalamSQL not configured".to_string()))?;

        self.register_system_tables_in_session(&user_session, kalam_sql)?;

        // Get the "kalam" catalog
        let catalog_name = "kalam";
        let catalog = user_session
            .catalog(catalog_name)
            .ok_or_else(|| KalamDbError::Other(format!("Catalog '{}' not found", catalog_name)))?;

        // Get tables from system.tables - either all or filtered
        let all_tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan system.tables: {}", e)))?;

        // Filter tables based on table_filter if provided
        let tables_to_load: Vec<_> = if let Some(filter) = table_filter {
            all_tables
                .into_iter()
                .filter(|t| {
                    // Build fully qualified name: namespace.table_name
                    let qualified = format!("{}.{}", t.namespace, t.table_name);
                    filter.contains(&qualified)
                        || filter.contains(&t.table_name)  // Also match unqualified
                        || filter.contains(&format!("system.{}", t.table_name)) // System table reference
                })
                .collect()
        } else {
            all_tables
        };

        // Create namespaces for all unique namespaces in the tables
        let unique_namespaces: std::collections::HashSet<_> = tables_to_load
            .iter()
            .map(|t| t.namespace.as_str())
            .collect();

        for namespace_str in unique_namespaces {
            if catalog.schema(namespace_str).is_none() {
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
        }

        // Register shared tables (no user isolation needed)
        for table in tables_to_load.iter().filter(|t| t.table_type == "shared") {
            let table_name = TableName::new(&table.table_name);
            let namespace_id = NamespaceId::new(&table.namespace);
            let table_id = format!("{}:{}", table.namespace, table.table_name);

            // Get schema
            let table_schema = kalam_sql
                .get_table_schema(&table_id, None)
                .map_err(|e| {
                    KalamDbError::Other(format!("Failed to get schema for {}: {}", table_id, e))
                })?
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!("Schema not found for table {}", table_id))
                })?;

            let arrow_schema_with_opts = ArrowSchemaWithOptions::from_json_string(
                &table_schema.arrow_schema,
            )
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to parse arrow schema for {}: {}",
                    table_id, e
                ))
            })?;
            let schema = arrow_schema_with_opts.schema;

            // Create metadata
            let metadata = TableMetadata {
                table_name: table_name.clone(),
                table_type: TableType::Shared,
                namespace: namespace_id.clone(),
                created_at: chrono::DateTime::from_timestamp_millis(table.created_at)
                    .unwrap_or_else(|| chrono::Utc::now()),
                storage_location: table.storage_location.clone(),
                flush_policy: serde_json::from_str(&table.flush_policy).unwrap_or_default(),
                schema_version: table.schema_version as u32,
                deleted_retention_hours: if table.deleted_retention_hours > 0 {
                    Some(table.deleted_retention_hours as u32)
                } else {
                    None
                },
            };

            // Get shared table store
            let store = self.shared_table_store.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("SharedTableStore not configured".to_string())
            })?;

            // Create provider
            let provider = Arc::new(SharedTableProvider::new(metadata, schema, store.clone()));

            // Register with fully qualified name
            let qualified_name = format!("{}.{}", namespace_id.as_str(), table_name.as_str());
            user_session
                .register_table(&qualified_name, provider.clone())
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to register shared table {}: {}",
                        qualified_name, e
                    ))
                })?;

            // Get the namespace schema and register there as well
            let df_schema = catalog.schema(namespace_id.as_str()).ok_or_else(|| {
                KalamDbError::Other(format!("Schema '{}' not found", namespace_id.as_str()))
            })?;

            // Check if table already exists and deregister it first
            if df_schema.table_exist(table_name.as_str()) {
                df_schema
                    .deregister_table(table_name.as_str())
                    .map_err(|e| {
                        KalamDbError::Other(format!(
                            "Failed to deregister existing shared table {} in schema {}: {}",
                            table_name.as_str(),
                            namespace_id.as_str(),
                            e
                        ))
                    })?;
            }

            df_schema
                .register_table(table_name.as_str().to_string(), provider)
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to register shared table {} in schema {}: {}",
                        table_name.as_str(),
                        namespace_id.as_str(),
                        e
                    ))
                })?;
        }

        // Register user tables with the current user_id (ensures data isolation)
        for table in tables_to_load.iter().filter(|t| t.table_type == "user") {
            let table_name = TableName::new(&table.table_name);
            let namespace_id = NamespaceId::new(&table.namespace);
            let table_id = format!("{}:{}", table.namespace, table.table_name);

            // Get schema
            let table_schema = kalam_sql
                .get_table_schema(&table_id, None)
                .map_err(|e| {
                    KalamDbError::Other(format!("Failed to get schema for {}: {}", table_id, e))
                })?
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!("Schema not found for table {}", table_id))
                })?;

            let arrow_schema_with_opts = ArrowSchemaWithOptions::from_json_string(
                &table_schema.arrow_schema,
            )
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to parse arrow schema for {}: {}",
                    table_id, e
                ))
            })?;
            let schema = arrow_schema_with_opts.schema;

            // Create metadata
            let metadata = TableMetadata {
                table_name: table_name.clone(),
                table_type: TableType::User,
                namespace: namespace_id.clone(),
                created_at: chrono::DateTime::from_timestamp_millis(table.created_at)
                    .unwrap_or_else(|| chrono::Utc::now()),
                storage_location: table.storage_location.clone(),
                flush_policy: serde_json::from_str(&table.flush_policy).unwrap_or_default(),
                schema_version: table.schema_version as u32,
                deleted_retention_hours: if table.deleted_retention_hours > 0 {
                    Some(table.deleted_retention_hours as u32)
                } else {
                    None
                },
            };

            // Get user table store
            let store = self.user_table_store.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("UserTableStore not configured".to_string())
            })?;

            // Create provider with the CURRENT user_id (critical for data isolation)
            let provider = Arc::new(UserTableProvider::new(
                metadata,
                schema,
                store.clone(),
                user_id.clone(),
                vec![], // parquet_paths - empty for now
            ));

            // Register with fully qualified name
            let qualified_name = format!("{}.{}", namespace_id.as_str(), table_name.as_str());
            user_session
                .register_table(&qualified_name, provider.clone())
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to register user table {}: {}",
                        qualified_name, e
                    ))
                })?;

            // Get the namespace schema and register there as well
            let df_schema = catalog.schema(namespace_id.as_str()).ok_or_else(|| {
                KalamDbError::Other(format!("Schema '{}' not found", namespace_id.as_str()))
            })?;

            // Check if table already exists and deregister it first
            if df_schema.table_exist(table_name.as_str()) {
                df_schema
                    .deregister_table(table_name.as_str())
                    .map_err(|e| {
                        KalamDbError::Other(format!(
                            "Failed to deregister existing user table {} in schema {}: {}",
                            table_name.as_str(),
                            namespace_id.as_str(),
                            e
                        ))
                    })?;
            }

            df_schema
                .register_table(table_name.as_str().to_string(), provider)
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to register user table {} in schema {}: {}",
                        table_name.as_str(),
                        namespace_id.as_str(),
                        e
                    ))
                })?;
        }

        // Register stream tables
        // Stream tables are NOT user-specific - they are append-only event streams shared across users
        for table in tables_to_load.iter().filter(|t| t.table_type == "stream") {
            let table_name = TableName::new(&table.table_name);
            let namespace_id = NamespaceId::new(&table.namespace);
            let table_id = format!("{}:{}", table.namespace, table.table_name);

            // Get schema
            let table_schema = kalam_sql
                .get_table_schema(&table_id, None)
                .map_err(|e| {
                    KalamDbError::Other(format!("Failed to get schema for {}: {}", table_id, e))
                })?
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!("Schema not found for table {}", table_id))
                })?;

            let arrow_schema_with_opts = ArrowSchemaWithOptions::from_json_string(
                &table_schema.arrow_schema,
            )
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to parse arrow schema for {}: {}",
                    table_id, e
                ))
            })?;
            let schema = arrow_schema_with_opts.schema;

            // Create metadata
            let metadata = TableMetadata {
                table_name: table_name.clone(),
                table_type: TableType::Stream,
                namespace: namespace_id.clone(),
                created_at: chrono::DateTime::from_timestamp_millis(table.created_at)
                    .unwrap_or_else(|| chrono::Utc::now()),
                storage_location: table.storage_location.clone(),
                flush_policy: serde_json::from_str(&table.flush_policy).unwrap_or_default(),
                schema_version: table.schema_version as u32,
                deleted_retention_hours: if table.deleted_retention_hours > 0 {
                    Some(table.deleted_retention_hours as u32)
                } else {
                    None
                },
            };

            // Get stream table store
            let store = self.stream_table_store.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("StreamTableStore not configured".to_string())
            })?;

            // Create provider
            let provider = Arc::new(StreamTableProvider::new(
                metadata,
                schema,
                store.clone(),
                None,  // retention_seconds - TODO: get from table metadata
                false, // ephemeral - TODO: get from table metadata
                None,  // max_buffer - TODO: get from table metadata
            ));

            // Register with fully qualified name
            let qualified_name = format!("{}.{}", namespace_id.as_str(), table_name.as_str());
            user_session
                .register_table(&qualified_name, provider.clone())
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to register stream table {}: {}",
                        qualified_name, e
                    ))
                })?;

            // Get the namespace schema and register there as well
            let df_schema = catalog.schema(namespace_id.as_str()).ok_or_else(|| {
                KalamDbError::Other(format!("Schema '{}' not found", namespace_id.as_str()))
            })?;

            // Check if table already exists and deregister it first
            if df_schema.table_exist(table_name.as_str()) {
                df_schema
                    .deregister_table(table_name.as_str())
                    .map_err(|e| {
                        KalamDbError::Other(format!(
                            "Failed to deregister existing stream table {} in schema {}: {}",
                            table_name.as_str(),
                            namespace_id.as_str(),
                            e
                        ))
                    })?;
            }

            df_schema
                .register_table(table_name.as_str().to_string(), provider)
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to register stream table {} in schema {}: {}",
                        table_name.as_str(),
                        namespace_id.as_str(),
                        e
                    ))
                })?;
        }

        Ok(user_session)
    }

    /// Execute query via DataFusion with selective table loading
    ///
    /// Loads only the tables referenced in the SQL query for better performance.
    /// If table extraction fails, falls back to loading all tables.
    async fn execute_datafusion_query_with_tables(
        &self,
        sql: &str,
        user_id: Option<&UserId>,
        table_refs: Option<std::collections::HashSet<String>>,
    ) -> Result<ExecutionResult, KalamDbError> {
        let anonymous_user = UserId::from("anonymous");
        let uid = user_id.unwrap_or(&anonymous_user);

        // Create session with selective table loading
        let session = self
            .create_user_session_context(uid, table_refs.as_ref())
            .await?;

        // Execute SQL via DataFusion
        let df = session.sql(sql).await.map_err(|e| {
            // Extract the root cause to avoid "Error planning query: Error during planning:" duplication
            let err_msg = e.to_string();
            if err_msg.contains("Error during planning:") {
                // DataFusion already says "Error during planning", don't duplicate
                KalamDbError::Other(err_msg)
            } else {
                KalamDbError::Other(format!("Error planning query: {}", err_msg))
            }
        })?;

        // Collect results
        let batches = df
            .collect()
            .await
            .map_err(|e| KalamDbError::Other(format!("Error executing query: {}", e)))?;

        // Return batches
        if batches.is_empty() {
            Ok(ExecutionResult::RecordBatches(vec![]))
        } else if batches.len() == 1 {
            Ok(ExecutionResult::RecordBatch(batches[0].clone()))
        } else {
            Ok(ExecutionResult::RecordBatches(batches))
        }
    }

    /// Execute query via DataFusion (legacy - loads all tables)
    ///
    /// Always creates a fresh SessionContext to ensure tables are up-to-date after CREATE TABLE operations.
    /// For queries with user_id, creates per-user providers with data isolation.
    /// For queries without user_id, creates session with anonymous user (shared/stream tables accessible).
    async fn execute_datafusion_query(
        &self,
        sql: &str,
        user_id: Option<&UserId>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Always create a fresh SessionContext (whether user_id provided or not)
        // This ensures newly created tables are available
        let anonymous_user = UserId::from("anonymous");
        let uid = user_id.unwrap_or(&anonymous_user);
        let session = self.create_user_session_context(uid, None).await?;

        // Execute SQL via DataFusion
        let df = session.sql(sql).await.map_err(|e| {
            // Extract the root cause to avoid "Error planning query: Error during planning:" duplication
            let err_msg = e.to_string();
            if err_msg.contains("Error during planning:") {
                // DataFusion already says "Error during planning", don't duplicate
                KalamDbError::Other(err_msg)
            } else {
                KalamDbError::Other(format!("Error planning query: {}", err_msg))
            }
        })?;

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
        let stmt = CreateNamespaceStatement::parse(sql)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;
        let created = self
            .namespace_service
            .create(stmt.name.as_str(), stmt.if_not_exists)?;

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

    /// Execute SHOW STORAGES
    async fn execute_show_storages(
        &self,
        sql: &str,
        user_id: Option<&UserId>,
    ) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::ShowStoragesStatement;

        let _stmt = ShowStoragesStatement::parse(sql).map_err(|e| KalamDbError::InvalidSql(e))?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Get all storages
        let storages = kalam_sql
            .scan_all_storages()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan storages: {}", e)))?;

        // Sort: 'local' first, then alphabetically
        let mut sorted_storages = storages;
        sorted_storages.sort_by(|a, b| {
            if a.storage_id == "local" {
                std::cmp::Ordering::Less
            } else if b.storage_id == "local" {
                std::cmp::Ordering::Greater
            } else {
                a.storage_id.cmp(&b.storage_id)
            }
        });

        // Convert to RecordBatch
        let mask_credentials = !Self::is_admin(user_id);
        let batch = Self::storages_to_record_batch(sorted_storages, mask_credentials)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute CREATE STORAGE
    async fn execute_create_storage(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::CreateStorageStatement;

        let stmt = CreateStorageStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("CREATE STORAGE parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Check if storage already exists
        if let Some(_) = kalam_sql
            .get_storage(&stmt.storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to check storage: {}", e)))?
        {
            return Err(KalamDbError::InvalidOperation(format!(
                "Storage '{}' already exists",
                stmt.storage_id
            )));
        }

        // Validate templates using StorageRegistry (only if provided)
        if let Some(storage_registry) = &self.storage_registry {
            if !stmt.shared_tables_template.is_empty() {
                storage_registry.validate_template(&stmt.shared_tables_template, false)?;
            }
            if !stmt.user_tables_template.is_empty() {
                storage_registry.validate_template(&stmt.user_tables_template, true)?;
            }
        }

        // Validate credentials JSON (if provided)
        let normalized_credentials = if let Some(raw) = stmt.credentials.as_ref() {
            let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
                KalamDbError::InvalidOperation(format!("Invalid credentials JSON: {}", e))
            })?;

            if !value.is_object() {
                return Err(KalamDbError::InvalidOperation(
                    "Credentials must be a JSON object".to_string(),
                ));
            }

            Some(serde_json::to_string(&value).map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to normalize credentials JSON: {}",
                    e
                ))
            })?)
        } else {
            None
        };

        // Create storage record
        let storage = kalamdb_sql::Storage {
            storage_id: stmt.storage_id.clone(),
            storage_name: stmt.storage_name,
            description: stmt.description,
            storage_type: stmt.storage_type,
            base_directory: stmt.base_directory,
            credentials: normalized_credentials,
            shared_tables_template: stmt.shared_tables_template,
            user_tables_template: stmt.user_tables_template,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        // Insert into system.storages
        kalam_sql
            .insert_storage(&storage)
            .map_err(|e| KalamDbError::Other(format!("Failed to create storage: {}", e)))?;

        Ok(ExecutionResult::Success(format!(
            "Storage '{}' created successfully",
            stmt.storage_id
        )))
    }

    /// Execute ALTER STORAGE
    async fn execute_alter_storage(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::AlterStorageStatement;

        let stmt = AlterStorageStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("ALTER STORAGE parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Get existing storage
        let mut storage = kalam_sql
            .get_storage(&stmt.storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get storage: {}", e)))?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!("Storage '{}' not found", stmt.storage_id))
            })?;

        // Apply updates
        if let Some(name) = stmt.storage_name {
            storage.storage_name = name;
        }
        if let Some(desc) = stmt.description {
            storage.description = Some(desc);
        }
        if let Some(template) = stmt.shared_tables_template {
            // Validate new template
            if let Some(storage_registry) = &self.storage_registry {
                storage_registry.validate_template(&template, false)?;
            }
            storage.shared_tables_template = template;
        }
        if let Some(template) = stmt.user_tables_template {
            // Validate new template
            if let Some(storage_registry) = &self.storage_registry {
                storage_registry.validate_template(&template, true)?;
            }
            storage.user_tables_template = template;
        }

        storage.updated_at = chrono::Utc::now().timestamp();

        // Update in system.storages
        kalam_sql
            .insert_storage(&storage) // Using insert as upsert
            .map_err(|e| KalamDbError::Other(format!("Failed to update storage: {}", e)))?;

        Ok(ExecutionResult::Success(format!(
            "Storage '{}' updated successfully",
            stmt.storage_id
        )))
    }

    /// Execute DROP STORAGE
    async fn execute_drop_storage(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::DropStorageStatement;

        let stmt = DropStorageStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("DROP STORAGE parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Check if storage exists
        match kalam_sql.get_storage(&stmt.storage_id) {
            Ok(Some(_)) => {
                // Storage exists, continue with the deletion logic
            }
            Ok(None) => {
                return Err(KalamDbError::NotFound(format!(
                    "Storage '{}' does not exist and cannot be dropped",
                    stmt.storage_id
                )));
            }
            Err(e) => {
                return Err(KalamDbError::Other(format!(
                    "Failed to check storage: {}",
                    e
                )));
            }
        }

        // Check if any tables reference this storage
        let tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        let dependent_tables: Vec<_> = tables
            .iter()
            .filter(|t| {
                t.storage_id
                    .as_ref()
                    .map(|id| id == &stmt.storage_id)
                    .unwrap_or(false)
            })
            .collect();

        if !dependent_tables.is_empty() {
            let table_names: Vec<String> = dependent_tables
                .iter()
                .take(10)
                .map(|t| format!("{}.{}", t.namespace, t.table_name))
                .collect();

            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot delete storage '{}': {} table(s) still reference it. Tables: {}{}",
                stmt.storage_id,
                dependent_tables.len(),
                table_names.join(", "),
                if dependent_tables.len() > 10 {
                    "..."
                } else {
                    ""
                }
            )));
        }

        // Prevent deletion of 'local' storage (after referential integrity check)
        if stmt.storage_id == "local" {
            return Err(KalamDbError::InvalidOperation(
                "Cannot delete 'local' storage (protected)".to_string(),
            ));
        }

        // Delete the storage
        kalam_sql
            .delete_storage(&stmt.storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to delete storage: {}", e)))?;

        Ok(ExecutionResult::Success(format!(
            "Storage '{}' deleted successfully",
            stmt.storage_id
        )))
    }

    /// Execute FLUSH TABLE
    ///
    /// Triggers asynchronous flush for a single table, returning job_id immediately.
    /// The flush operation runs in the background via JobManager.
    async fn execute_flush_table(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::FlushTableStatement;

        let stmt = FlushTableStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("FLUSH TABLE parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Verify table exists
        let tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        let table = tables
            .iter()
            .find(|t| t.namespace == stmt.namespace && t.table_name == stmt.table_name)
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table '{}.{}' does not exist",
                    stmt.namespace, stmt.table_name
                ))
            })?;

        // Only user tables can be flushed (shared/stream tables not supported yet)
        if table.table_type != "user" {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot flush {} table '{}.{}'. Only user tables can be flushed.",
                table.table_type, stmt.namespace, stmt.table_name
            )));
        }

        // Get required dependencies
        let job_manager = self.job_manager.as_ref().ok_or_else(|| {
            KalamDbError::Other("JobManager not initialized".to_string())
        })?;

        let user_table_store = self.user_table_store.as_ref().ok_or_else(|| {
            KalamDbError::Other("UserTableStore not initialized".to_string())
        })?;

        let storage_registry = self.storage_registry.as_ref().ok_or_else(|| {
            KalamDbError::Other("StorageRegistry not initialized".to_string())
        })?;

        let jobs_provider = self.jobs_table_provider.clone();

        // T21: Check if a flush job is already running for this table
        let table_full_name = format!("{}.{}", stmt.namespace, stmt.table_name);
        if let Some(ref provider) = jobs_provider {
            let all_jobs = provider.list_jobs()?;
            for job in all_jobs {
                if job.status == "running"
                    && job.job_type == "flush"
                    && job.table_name.as_deref() == Some(&table_full_name)
                {
                    return Err(KalamDbError::InvalidOperation(format!(
                        "Flush job already running for table '{}' (job_id: {}). Please wait for it to complete.",
                        table_full_name, job.job_id
                    )));
                }
            }
        }

        // Get table schema
        let schemas = kalam_sql
            .get_table_schemas_for_table(&table.table_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get schemas: {:?}", e)))?;

        if schemas.is_empty() {
            return Err(KalamDbError::NotFound(format!(
                "No schema found for table '{}.{}'",
                stmt.namespace, stmt.table_name
            )));
        }

        // Use the latest schema
        let latest_schema = schemas.into_iter().max_by_key(|s| s.version).unwrap();
        
        // Convert to Arrow schema
        let arrow_schema = crate::schema::arrow_schema::ArrowSchemaWithOptions::from_json_string(
            &latest_schema.arrow_schema
        )?;

        // Generate job_id for tracking
        let job_id = format!(
            "flush-{}-{}-{}",
            stmt.table_name,
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );
        let job_id_clone = job_id.clone();

        // Create flush job
        let namespace_id = NamespaceId::from(stmt.namespace.as_str());
        let table_name = TableName::new(stmt.table_name.clone());
        
        let flush_job = crate::flush::UserTableFlushJob::new(
            user_table_store.clone(),
            namespace_id,
            table_name.clone(),
            arrow_schema.schema,
            table.storage_location.clone(),
        )
        .with_storage_registry(storage_registry.clone());
        
        // Add jobs_provider if available
        let flush_job = if let Some(ref provider) = jobs_provider {
            flush_job.with_jobs_provider(provider.clone())
        } else {
            flush_job
        };

        // Clone necessary data for the async task
        let namespace_str = stmt.namespace.clone();
        let table_name_str = stmt.table_name.clone();

        // Spawn async flush task via JobManager
        let job_future = Box::pin(async move {
            log::info!(
                "Executing flush job: job_id={}, table={}.{}",
                job_id_clone,
                namespace_str,
                table_name_str
            );

            match flush_job.execute() {
                Ok(result) => {
                    log::info!(
                        "Flush job completed successfully: job_id={}, rows_flushed={}, users_count={}",
                        job_id_clone,
                        result.rows_flushed,
                        result.users_count
                    );
                    Ok(format!(
                        "Flushed {} rows for {} users",
                        result.rows_flushed,
                        result.users_count
                    ))
                }
                Err(e) => {
                    log::error!(
                        "Flush job failed: job_id={}, error={}",
                        job_id_clone,
                        e
                    );
                    Err(format!("Flush failed: {}", e))
                }
            }
        });

        job_manager.start_job(job_id.clone(), "flush".to_string(), job_future).await?;

        log::info!(
            "Flush job spawned: job_id={}, table={}.{}",
            job_id,
            stmt.namespace,
            stmt.table_name
        );

        Ok(ExecutionResult::Success(format!(
            "Flush started for table '{}.{}'. Job ID: {}",
            stmt.namespace, stmt.table_name, job_id
        )))
    }

    /// Execute FLUSH ALL TABLES
    ///
    /// Triggers asynchronous flush for all user tables in a namespace, returning
    /// array of job_ids (one per table).
    async fn execute_flush_all_tables(
        &self,
        sql: &str,
    ) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::FlushAllTablesStatement;

        let stmt = FlushAllTablesStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("FLUSH ALL TABLES parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Verify namespace exists
        match kalam_sql.get_namespace(&stmt.namespace) {
            Ok(Some(_)) => { /* namespace exists */ }
            Ok(None) => {
                return Err(KalamDbError::NotFound(format!(
                    "Namespace '{}' does not exist",
                    stmt.namespace
                )));
            }
            Err(e) => {
                return Err(KalamDbError::Other(format!(
                    "Failed to check namespace: {}",
                    e
                )));
            }
        }

        // Get all user tables in the namespace
        let tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        let user_tables: Vec<_> = tables
            .iter()
            .filter(|t| t.namespace == stmt.namespace && t.table_type == "user")
            .collect();

        if user_tables.is_empty() {
            return Ok(ExecutionResult::Success(format!(
                "No user tables found in namespace '{}' to flush",
                stmt.namespace
            )));
        }

        // Create flush job for each table
        let mut job_ids = Vec::new();
        for table in user_tables {
            let job_id = format!(
                "flush-{}-{}-{}",
                table.table_name,
                chrono::Utc::now().timestamp_millis(),
                uuid::Uuid::new_v4()
            );

            // Create job record
            let job_record = crate::tables::system::jobs_provider::JobRecord::new(
                job_id.clone(),
                "flush".to_string(),
                format!("node-{}", std::process::id()),
            )
            .with_table_name(format!("{}.{}", table.namespace, table.table_name));

            // Persist job to system.jobs
            if let Some(ref jobs_table_provider) = self.jobs_table_provider {
                jobs_table_provider.insert_job(job_record)?;
            }

            log::info!(
                "Flush job created: job_id={}, table={}.{}",
                job_id,
                table.namespace,
                table.table_name
            );

            job_ids.push(job_id);
        }

        // TODO: Spawn async flush tasks via JobManager (T250)

        Ok(ExecutionResult::Success(format!(
            "Flush started for {} table(s) in namespace '{}'. Job IDs: [{}]",
            job_ids.len(),
            stmt.namespace,
            job_ids.join(", ")
        )))
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

    /// Execute KILL JOB command
    async fn execute_kill_job(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::ddl::JobCommand;

        // Parse the KILL JOB command
        let command = parse_job_command(sql)
            .map_err(|e| KalamDbError::InvalidSql(format!("Failed to parse KILL JOB: {}", e)))?;

        let job_id = match command {
            JobCommand::Kill { job_id } => job_id,
        };

        // Get JobsTableProvider
        let jobs_provider = self.jobs_table_provider.as_ref().ok_or_else(|| {
            KalamDbError::InvalidOperation("Job management not configured".to_string())
        })?;

        // Cancel the job
        jobs_provider
            .cancel_job(&job_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to cancel job: {}", e)))?;

        Ok(ExecutionResult::Success(format!(
            "Job '{}' cancelled successfully",
            job_id
        )))
    }

    /// Execute SUBSCRIBE TO command
    ///
    /// Returns metadata instructing the client to establish a WebSocket connection.
    /// This command does NOT execute a subscription directly - it returns information
    /// needed for the client to connect via WebSocket.
    async fn execute_subscribe(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::ddl::SubscribeStatement;

        // Parse the SUBSCRIBE TO command
        let stmt = SubscribeStatement::parse(sql)
            .map_err(|e| KalamDbError::InvalidSql(format!("Failed to parse SUBSCRIBE TO: {}", e)))?;

        // Generate a unique subscription ID
        use uuid::Uuid;
        let subscription_id = format!("sub-{}", Uuid::new_v4());

        // Convert SUBSCRIBE statement to SELECT SQL for the subscription
        let select_sql = stmt.to_select_sql();

        // Build subscription metadata response
        let ws_url = std::env::var("WEBSOCKET_URL")
            .unwrap_or_else(|_| "ws://localhost:8080/v1/ws".to_string());

        // Create subscription response as JSON
        let mut subscription = serde_json::Map::new();
        subscription.insert("id".to_string(), serde_json::json!(subscription_id));
        subscription.insert("sql".to_string(), serde_json::json!(select_sql));
        
        // Add options if present
        if let Some(last_rows) = stmt.options.last_rows {
            let mut options = serde_json::Map::new();
            options.insert("last_rows".to_string(), serde_json::json!(last_rows));
            subscription.insert("options".to_string(), serde_json::json!(options));
        }

        let mut response = serde_json::Map::new();
        response.insert("status".to_string(), serde_json::json!("subscription_required"));
        response.insert("ws_url".to_string(), serde_json::json!(ws_url));
        response.insert("subscription".to_string(), serde_json::json!(subscription));
        response.insert(
            "message".to_string(),
            serde_json::json!(format!(
                "Connect to WebSocket at {} and send the subscription object",
                ws_url
            )),
        );

        Ok(ExecutionResult::Subscription(serde_json::Value::Object(response)))
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

        // Check for TABLE_TYPE clause to determine table type
        let has_table_type_user = sql_upper.contains("TABLE_TYPE")
            && (sql_upper.contains("TABLE_TYPE USER")
                || sql_upper.contains("TABLE_TYPE 'USER'")
                || sql_upper.contains("TABLE_TYPE \"USER\""));
        let has_table_type_shared = sql_upper.contains("TABLE_TYPE")
            && (sql_upper.contains("TABLE_TYPE SHARED")
                || sql_upper.contains("TABLE_TYPE 'SHARED'")
                || sql_upper.contains("TABLE_TYPE \"SHARED\""));
        let has_table_type_stream = sql_upper.contains("TABLE_TYPE")
            && (sql_upper.contains("TABLE_TYPE STREAM")
                || sql_upper.contains("TABLE_TYPE 'STREAM'")
                || sql_upper.contains("TABLE_TYPE \"STREAM\""));

        if sql_upper.contains("USER TABLE")
            || sql_upper.contains("${USER_ID}")
            || has_table_type_user
        {
            // User table - requires user_id (from header or OWNER_ID clause)
            // Try to extract OWNER_ID from SQL if user_id header is not provided
            let extracted_owner_id: Option<crate::catalog::UserId> = if user_id.is_none() {
                // Extract OWNER_ID from SQL using regex
                use regex::Regex;
                let owner_id_re = Regex::new(r#"(?i)OWNER_ID\s+['"]([^'"]+)['"]"#).unwrap();
                owner_id_re
                    .captures(sql)
                    .map(|caps| crate::catalog::UserId::from(caps[1].to_string().as_str()))
            } else {
                None
            };

            let actual_user_id = if let Some(uid) = user_id {
                uid
            } else if let Some(ref extracted_uid) = extracted_owner_id {
                extracted_uid
            } else {
                return Err(KalamDbError::InvalidOperation(
                    "CREATE USER TABLE requires X-USER-ID header or OWNER_ID clause".to_string(),
                ));
            };

            let namespace_id_for_parse =
                kalamdb_commons::models::NamespaceId::new(namespace_id.as_str());
            
            // Use unified parser for consistent parsing across all table types
            let stmt = CreateTableStatement::parse(sql, &namespace_id_for_parse)
                .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;
            
            // Verify this is actually a USER table
            if stmt.table_type != kalamdb_commons::models::TableType::User {
                return Err(KalamDbError::InvalidSql(
                    "Expected CREATE USER TABLE statement".to_string(),
                ));
            }
            
            // stmt fields are already the right types from kalamdb_commons
            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone();
            let schema = stmt.schema.clone();
            let flush_policy = stmt.flush_policy.clone();
            let deleted_retention_hours = stmt.deleted_retention_hours;
            // Extract storage fields before stmt is moved
            let stmt_storage_id = stmt.storage_id.clone();
            let stmt_use_user_storage = stmt.use_user_storage;

            // Validate and resolve storage_id
            let storage_id = self.validate_storage_id(stmt_storage_id)?;

            let metadata = self.user_table_service.create_table(stmt)?;

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
                    storage_id: Some(storage_id.as_str().to_string()),
                    use_user_storage: stmt_use_user_storage,
                    flush_policy: serde_json::to_string(&flush_policy.clone().unwrap_or_default())
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
                    actual_user_id.clone(),
                )
                .await?;
            }

            Ok(ExecutionResult::Success(
                "User table created successfully".to_string(),
            ))
        } else if sql_upper.contains("STREAM TABLE")
            || sql_upper.contains("TTL")
            || sql_upper.contains("BUFFER_SIZE")
            || has_table_type_stream
        {
            // Stream table - use unified parser
            let stmt = CreateTableStatement::parse(sql, &namespace_id)
                .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;
                
            // Verify this is actually a STREAM table
            if stmt.table_type != kalamdb_commons::models::TableType::Stream {
                return Err(KalamDbError::InvalidSql(
                    "Expected CREATE STREAM TABLE statement".to_string(),
                ));
            }
            
            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone(); // Use the namespace from the statement
            let schema = stmt.schema.clone();
            let retention_seconds = stmt.ttl_seconds.map(|t| t as u32);

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
                    storage_id: Some("local".to_string()), // Stream tables always use local storage
                    use_user_storage: false, // Stream tables don't support user storage
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
        } else if has_table_type_shared {
            // Shared table specified via TABLE_TYPE
            // Use unified parser for consistent parsing
            let stmt = CreateTableStatement::parse(sql, &namespace_id)
                .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;
            
            // Verify this is actually a SHARED table
            if stmt.table_type != kalamdb_commons::models::TableType::Shared {
                return Err(KalamDbError::InvalidSql(
                    "Expected CREATE SHARED TABLE statement".to_string(),
                ));
            }
            
            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone(); // Use the namespace from the statement
            let schema = stmt.schema.clone();
            let flush_policy = stmt.flush_policy.clone();
            let deleted_retention = stmt.deleted_retention_hours.map(|h| h as u64 * 3600);
            
            // Extract storage_id before stmt is moved
            let stmt_storage_id = stmt.storage_id.clone();

            // Validate and resolve storage_id
            let storage_id = self.validate_storage_id(stmt_storage_id)?;

            let (metadata, was_created) = self.shared_table_service.create_table(stmt)?;

            // Only insert into system.tables and register with DataFusion if table was newly created
            if was_created {
                // Insert into system.tables via KalamSQL
                if let Some(kalam_sql) = &self.kalam_sql {
                    let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());

                    // Serialize flush policy for storage
                    let flush_policy_json = serde_json::to_string(&flush_policy.unwrap_or_default())
                        .unwrap_or_else(|_| "{}".to_string());

                    let table = kalamdb_sql::models::Table {
                        table_id,
                        table_name: table_name.as_str().to_string(),
                        namespace: namespace_id.as_str().to_string(),
                        table_type: "shared".to_string(),
                        created_at: chrono::Utc::now().timestamp_millis(),
                        storage_location: metadata.storage_location.clone(),
                        storage_id: Some(storage_id.as_str().to_string()),
                        use_user_storage: false, // Shared tables don't support user storage
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
            } else {
                // Table already existed (IF NOT EXISTS case)
                Ok(ExecutionResult::Success(format!(
                    "Table {}.{} already exists (skipped)",
                    namespace_id, table_name
                )))
            }
        } else {
            // No TABLE_TYPE specified - default to shared table (most common case)
            // Use unified parser for consistent parsing
            let stmt = CreateTableStatement::parse(sql, &namespace_id)
                .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;
            
            // The unified parser defaults to SHARED when no table type is specified
            // So we don't need to verify the type here
            
            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone();
            let schema = stmt.schema.clone();
            let flush_policy = stmt.flush_policy.clone();
            let deleted_retention = stmt.deleted_retention_hours.map(|h| h as u64 * 3600);
            
            // Extract storage_id before stmt is moved
            let stmt_storage_id = stmt.storage_id.clone();

            // Validate and resolve storage_id
            let storage_id = self.validate_storage_id(stmt_storage_id)?;

            let (metadata, was_created) = self.shared_table_service.create_table(stmt)?;

            if was_created {
                if let Some(kalam_sql) = &self.kalam_sql {
                    let table_id = format!("{}:{}", namespace_id.as_str(), table_name.as_str());
                    
                    // Serialize flush policy for storage
                    let flush_policy_json = serde_json::to_string(&flush_policy.unwrap_or_default())
                        .unwrap_or_else(|_| "{}".to_string());

                    let table = kalamdb_sql::models::Table {
                        table_id,
                        table_name: table_name.as_str().to_string(),
                        namespace: namespace_id.as_str().to_string(),
                        table_type: "shared".to_string(),
                        created_at: chrono::Utc::now().timestamp_millis(),
                        storage_location: metadata.storage_location.clone(),
                        storage_id: Some(storage_id.as_str().to_string()),
                        use_user_storage: false,
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
            } else {
                Ok(ExecutionResult::Success(format!(
                    "Table {}.{} already exists (skipped)",
                    namespace_id, table_name
                )))
            }
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

        // Use default namespace - the SQL parser will extract namespace from qualified name (namespace.table)
        let default_namespace = kalamdb_commons::NamespaceId::new("default".to_string());
        let stmt = DropTableStatement::parse(sql, &default_namespace)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        // Convert TableKind to TableType
        let result = deletion_service.drop_table(
            &stmt.namespace_id,
            &stmt.table_name,
            stmt.table_type.into(),
            stmt.if_exists,
        )?;

        // If if_exists is true and table didn't exist, return success message
        if result.is_none() {
            return Ok(ExecutionResult::Success(format!(
                "Table {}.{} does not exist (skipped)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            )));
        }

        let deletion_result = result.unwrap();
        Ok(ExecutionResult::Success(format!(
            "Table {}.{} dropped successfully ({} Parquet files deleted, {} bytes freed)",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            deletion_result.files_deleted,
            deletion_result.bytes_freed
        )))
    }

    /// Execute UPDATE statement
    ///
    /// Simple implementation for integration tests:
    /// UPDATE namespace.table SET col1=val1, col2=val2 WHERE key_col = 'key_value'
    ///
    /// For Phase 18, this is a temporary implementation that:
    /// 1. Parses the SQL to extract table, SET clause, WHERE clause
    /// 2. Scans all rows and filters in-memory (simple WHERE evaluation)
    /// 3. Calls TableProvider update methods directly
    /// 4. For user tables, creates per-user SessionContext to ensure data isolation
    ///
    /// Future: Use DataFusion's DML capabilities when available
    async fn execute_update(
        &self,
        sql: &str,
        user_id: Option<&UserId>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Parse UPDATE statement (basic parser for integration tests)
        let update_info = self.parse_update_statement(sql)?;

        // Create appropriate SessionContext based on user_id
        let effective_user_id = user_id
            .cloned()
            .unwrap_or_else(|| UserId::from("anonymous"));

        // For UPDATE, only load the table being updated
        let table_name = format!("{}.{}", update_info.namespace, update_info.table);
        let mut table_refs = std::collections::HashSet::new();
        table_refs.insert(table_name);

        let session = self
            .create_user_session_context(&effective_user_id, Some(&table_refs))
            .await?;

        // Get table provider from the appropriate session
        let table_ref = datafusion::sql::TableReference::parse_str(&format!(
            "{}.{}",
            update_info.namespace, update_info.table
        ));
        let table_provider = session
            .table_provider(table_ref)
            .await
            .map_err(|e| KalamDbError::Other(format!("Table not found: {}", e)))?;

        // Try to cast to UserTableProvider first
        use crate::tables::{SharedTableProvider, UserTableProvider};

        // Check if it's a UserTableProvider
        if let Some(user_provider) = table_provider.as_any().downcast_ref::<UserTableProvider>() {
            // User table UPDATE with isolation
            let store = self.user_table_store.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("UserTableStore not configured".to_string())
            })?;

            // Scan only this user's rows
            let all_rows = store
                .scan_user(
                    &update_info.namespace,
                    &update_info.table,
                    effective_user_id.as_str(),
                )
                .map_err(|e| KalamDbError::Other(format!("Scan failed: {}", e)))?;

            // Filter rows by WHERE clause (simple evaluation: col = 'value')
            let (where_col, where_val) = self.parse_simple_where(&update_info.where_clause)?;

            let mut updated_count = 0;
            for (row_id, row_data) in all_rows {
                // Check if row matches WHERE clause
                if let Some(obj) = row_data.as_object() {
                    if let Some(col_value) = obj.get(&where_col) {
                        if Self::value_matches(col_value, &where_val) {
                            // Build update JSON from SET clause
                            let mut updates = serde_json::Map::new();
                            for (col, val) in &update_info.set_values {
                                updates.insert(col.clone(), val.clone());
                            }

                            // Call update method
                            user_provider
                                .update_row(&row_id, serde_json::Value::Object(updates))
                                .map_err(|e| {
                                    KalamDbError::Other(format!("Update failed: {}", e))
                                })?;

                            updated_count += 1;
                        }
                    }
                }
            }

            return Ok(ExecutionResult::Success(format!(
                "Updated {} row(s)",
                updated_count
            )));
        }

        // Otherwise, try SharedTableProvider
        let shared_provider = table_provider
            .as_any()
            .downcast_ref::<SharedTableProvider>()
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(
                    "UPDATE only supported on shared and user tables currently".to_string(),
                )
            })?;

        // Scan all rows from store
        let all_rows = self
            .shared_table_store
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("Store not configured".to_string()))?
            .scan(&update_info.namespace, &update_info.table)
            .map_err(|e| KalamDbError::Other(format!("Scan failed: {}", e)))?;

        // Filter rows by WHERE clause (simple evaluation: col = 'value')
        let (where_col, where_val) = self.parse_simple_where(&update_info.where_clause)?;

        let mut updated_count = 0;
        for (row_id, row_data) in all_rows {
            // Check if row matches WHERE clause
            if let Some(obj) = row_data.as_object() {
                if let Some(col_value) = obj.get(&where_col) {
                    if Self::value_matches(col_value, &where_val) {
                        // Build update JSON from SET clause
                        let mut updates = serde_json::Map::new();
                        for (col, val) in &update_info.set_values {
                            updates.insert(col.clone(), val.clone());
                        }

                        // Call update method
                        shared_provider
                            .update(&row_id, serde_json::Value::Object(updates))
                            .map_err(|e| KalamDbError::Other(format!("Update failed: {}", e)))?;

                        updated_count += 1;
                    }
                }
            }
        }

        Ok(ExecutionResult::Success(format!(
            "Updated {} row(s)",
            updated_count
        )))
    }

    /// Execute DELETE statement
    ///
    /// DELETE FROM namespace.table WHERE key_col = 'key_value'
    ///
    /// Implements soft delete (sets _deleted=true)
    /// For user tables, creates per-user SessionContext to ensure data isolation
    async fn execute_delete(
        &self,
        sql: &str,
        user_id: Option<&UserId>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Parse DELETE statement
        let delete_info = self.parse_delete_statement(sql)?;

        // Create appropriate SessionContext based on user_id
        let effective_user_id = user_id
            .cloned()
            .unwrap_or_else(|| UserId::from("anonymous"));

        // For DELETE, only load the table being deleted from
        let table_name = format!("{}.{}", delete_info.namespace, delete_info.table);
        let mut table_refs = std::collections::HashSet::new();
        table_refs.insert(table_name);

        let session = self
            .create_user_session_context(&effective_user_id, Some(&table_refs))
            .await?;

        // Get table provider from the appropriate session
        let table_ref = datafusion::sql::TableReference::parse_str(&format!(
            "{}.{}",
            delete_info.namespace, delete_info.table
        ));
        let table_provider = session
            .table_provider(table_ref)
            .await
            .map_err(|e| KalamDbError::Other(format!("Table not found: {}", e)))?;

        // Try to cast to UserTableProvider first
        use crate::tables::{SharedTableProvider, UserTableProvider};

        // Check if it's a UserTableProvider
        if let Some(user_provider) = table_provider.as_any().downcast_ref::<UserTableProvider>() {
            // User table DELETE with isolation
            let store = self.user_table_store.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("UserTableStore not configured".to_string())
            })?;

            // Scan only this user's rows
            let all_rows = store
                .scan_user(
                    &delete_info.namespace,
                    &delete_info.table,
                    effective_user_id.as_str(),
                )
                .map_err(|e| KalamDbError::Other(format!("Scan failed: {}", e)))?;

            // Filter rows by WHERE clause
            let (where_col, where_val) = self.parse_simple_where(&delete_info.where_clause)?;

            let mut deleted_count = 0;
            for (row_id, row_data) in all_rows {
                // Check if row matches WHERE clause
                if let Some(obj) = row_data.as_object() {
                    if let Some(col_value) = obj.get(&where_col) {
                        if Self::value_matches(col_value, &where_val) {
                            // Call delete method (soft delete for user tables)
                            user_provider.delete_row(&row_id).map_err(|e| {
                                KalamDbError::Other(format!("Delete failed: {}", e))
                            })?;

                            deleted_count += 1;
                        }
                    }
                }
            }

            return Ok(ExecutionResult::Success(format!(
                "Deleted {} row(s)",
                deleted_count
            )));
        }

        // Otherwise, try SharedTableProvider
        let shared_provider = table_provider
            .as_any()
            .downcast_ref::<SharedTableProvider>()
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(
                    "DELETE only supported on shared and user tables currently".to_string(),
                )
            })?;

        // Scan all rows from store
        let all_rows = self
            .shared_table_store
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("Store not configured".to_string()))?
            .scan(&delete_info.namespace, &delete_info.table)
            .map_err(|e| KalamDbError::Other(format!("Scan failed: {}", e)))?;

        // Filter rows by WHERE clause
        let (where_col, where_val) = self.parse_simple_where(&delete_info.where_clause)?;

        let mut deleted_count = 0;
        for (row_id, row_data) in all_rows {
            // Check if row matches WHERE clause
            if let Some(obj) = row_data.as_object() {
                if let Some(col_value) = obj.get(&where_col) {
                    if Self::value_matches(col_value, &where_val) {
                        // Call soft delete method
                        shared_provider
                            .delete_soft(&row_id)
                            .map_err(|e| KalamDbError::Other(format!("Delete failed: {}", e)))?;

                        deleted_count += 1;
                    }
                }
            }
        }

        Ok(ExecutionResult::Success(format!(
            "Deleted {} row(s)",
            deleted_count
        )))
    }

    /// Parse simple WHERE clause: col = 'value'
    fn parse_simple_where(&self, where_clause: &str) -> Result<(String, String), KalamDbError> {
        let parts: Vec<&str> = where_clause.split('=').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(KalamDbError::InvalidSql(format!(
                "Unsupported WHERE clause (only simple col='value' supported): {}",
                where_clause
            )));
        }
        let col_name = parts[0].to_string();
        let col_value = parts[1].trim_matches('\'').trim_matches('"').to_string();
        Ok((col_name, col_value))
    }

    /// Compare a JSON value with a string literal from a WHERE clause
    fn value_matches(value: &serde_json::Value, target: &str) -> bool {
        match value {
            serde_json::Value::String(s) => s == target,
            serde_json::Value::Number(n) => n.to_string() == target,
            serde_json::Value::Bool(b) => {
                let target_lower = target.to_ascii_lowercase();
                match target_lower.as_str() {
                    "true" => *b,
                    "false" => !*b,
                    _ => false,
                }
            }
            serde_json::Value::Null => target.eq_ignore_ascii_case("null"),
            _ => false,
        }
    }

    /// Parse UPDATE statement (simple parser for integration tests)
    fn parse_update_statement(&self, sql: &str) -> Result<UpdateInfo, KalamDbError> {
        // Regex pattern: UPDATE namespace.table SET col1=val1, col2=val2 WHERE condition
        let re = regex::Regex::new(
            r"(?i)UPDATE\s+([a-zA-Z0-9_]+)\.([a-zA-Z0-9_]+)\s+SET\s+(.+?)\s+WHERE\s+(.+)",
        )
        .unwrap();

        let captures = re
            .captures(sql.trim())
            .ok_or_else(|| KalamDbError::InvalidSql(format!("Invalid UPDATE syntax: {}", sql)))?;

        let namespace = captures[1].to_string();
        let table = captures[2].to_string();
        let set_clause = captures[3].to_string();
        let where_clause = captures[4].to_string();

        // Parse SET clause (col1=val1, col2=val2)
        let mut set_values = Vec::new();
        for assignment in set_clause.split(',') {
            let parts: Vec<&str> = assignment.split('=').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(KalamDbError::InvalidSql(format!(
                    "Invalid SET clause: {}",
                    assignment
                )));
            }
            let col_name = parts[0].to_string();
            let col_value = parts[1].trim_matches('\'').trim_matches('"').to_string();
            set_values.push((col_name, serde_json::json!(col_value)));
        }

        Ok(UpdateInfo {
            namespace,
            table,
            set_values,
            where_clause,
        })
    }

    /// Parse DELETE statement
    fn parse_delete_statement(&self, sql: &str) -> Result<DeleteInfo, KalamDbError> {
        // Regex pattern: DELETE FROM namespace.table WHERE condition
        let re = regex::Regex::new(
            r"(?i)DELETE\s+FROM\s+([a-zA-Z0-9_]+)\.([a-zA-Z0-9_]+)\s+WHERE\s+(.+)",
        )
        .unwrap();

        let captures = re
            .captures(sql.trim())
            .ok_or_else(|| KalamDbError::InvalidSql(format!("Invalid DELETE syntax: {}", sql)))?;

        Ok(DeleteInfo {
            namespace: captures[1].to_string(),
            table: captures[2].to_string(),
            where_clause: captures[3].to_string(),
        })
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

    /// Convert storage list to RecordBatch for SHOW STORAGES
    fn storages_to_record_batch(
        storages: Vec<kalamdb_sql::Storage>,
        mask_credentials: bool,
    ) -> Result<RecordBatch, KalamDbError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("storage_id", DataType::Utf8, false),
            Field::new("storage_name", DataType::Utf8, false),
            Field::new("storage_type", DataType::Utf8, false),
            Field::new("base_directory", DataType::Utf8, false),
            Field::new("credentials", DataType::Utf8, true),
            Field::new("description", DataType::Utf8, true),
        ]));

        let ids: ArrayRef = Arc::new(StringArray::from(
            storages
                .iter()
                .map(|s| s.storage_id.as_str())
                .collect::<Vec<_>>(),
        ));

        let names: ArrayRef = Arc::new(StringArray::from(
            storages
                .iter()
                .map(|s| s.storage_name.as_str())
                .collect::<Vec<_>>(),
        ));

        let types: ArrayRef = Arc::new(StringArray::from(
            storages
                .iter()
                .map(|s| s.storage_type.as_str())
                .collect::<Vec<_>>(),
        ));

        let directories: ArrayRef = Arc::new(StringArray::from(
            storages
                .iter()
                .map(|s| s.base_directory.as_str())
                .collect::<Vec<_>>(),
        ));

        let credentials_values: Vec<Option<String>> = storages
            .iter()
            .map(|s| match (&s.credentials, mask_credentials) {
                (Some(_), true) => Some("***".to_string()),
                (Some(value), false) => Some(value.clone()),
                (None, _) => None,
            })
            .collect();

        let credentials: ArrayRef = Arc::new(StringArray::from(credentials_values));

        let descriptions: ArrayRef = Arc::new(StringArray::from(
            storages
                .iter()
                .map(|s| s.description.as_deref())
                .collect::<Vec<_>>(),
        ));

        RecordBatch::try_new(
            schema,
            vec![ids, names, types, directories, credentials, descriptions],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    fn is_admin(user_id: Option<&UserId>) -> bool {
        match user_id {
            Some(id) => {
                let id_str = id.as_ref().to_lowercase();
                id_str == "admin" || id_str == "system"
            }
            None => false,
        }
    }

    /// Convert table details to RecordBatch for DESCRIBE TABLE
    fn table_details_to_record_batch(
        table: &kalamdb_sql::Table,
    ) -> Result<RecordBatch, KalamDbError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("property", DataType::Utf8, false),
            Field::new("value", DataType::Utf8, false),
        ]));

        let created_at_str = chrono::DateTime::from_timestamp_millis(table.created_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "unknown".to_string());

        let schema_version_str = table.schema_version.to_string();
        let retention_hours_str = table.deleted_retention_hours.to_string();

        // Check if this is a stream table to show stream-specific metadata
        let is_stream_table = table.table_type.eq_ignore_ascii_case("stream");

        let (properties, values): (Vec<&str>, Vec<String>) = if is_stream_table {
            // Stream table properties (NO system columns, show stream-specific config)
            (
                vec![
                    "table_id",
                    "table_name",
                    "namespace",
                    "table_type",
                    "schema_version",
                    "created_at",
                    "note",
                ],
                vec![
                    table.table_id.to_string(),
                    table.table_name.to_string(),
                    table.namespace.to_string(),
                    table.table_type.to_string(),
                    schema_version_str,
                    created_at_str,
                    "Stream tables: NO _updated/_deleted columns, NO Parquet storage (ephemeral)"
                        .to_string(),
                ],
            )
        } else {
            // User/Shared table properties (show storage and retention)
            (
                vec![
                    "table_id",
                    "table_name",
                    "namespace",
                    "table_type",
                    "storage_location",
                    "flush_policy",
                    "schema_version",
                    "deleted_retention_hours",
                    "created_at",
                ],
                vec![
                    table.table_id.to_string(),
                    table.table_name.to_string(),
                    table.namespace.to_string(),
                    table.table_type.to_string(),
                    table.storage_location.to_string(),
                    table.flush_policy.to_string(),
                    schema_version_str,
                    retention_hours_str,
                    created_at_str,
                ],
            )
        };

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
        let user_table_store =
            Arc::new(kalamdb_store::UserTableStore::new(test_db.db.clone()).unwrap());
        let user_table_service = Arc::new(crate::services::UserTableService::new(
            kalam_sql.clone(),
            user_table_store,
        ));
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
    async fn test_describe_stream_table_shows_no_system_columns() {
        // Create test database with necessary column families
        let test_db =
            TestDb::new(&["system_namespaces", "system_tables", "system_table_schemas"]).unwrap();
        let kalam_sql = Arc::new(KalamSql::new(test_db.db.clone()).unwrap());
        let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
        let session_context = Arc::new(SessionContext::new());

        let user_table_store =
            Arc::new(kalamdb_store::UserTableStore::new(test_db.db.clone()).unwrap());
        let user_table_service = Arc::new(crate::services::UserTableService::new(
            kalam_sql.clone(),
            user_table_store.clone(),
        ));
        let shared_table_store =
            Arc::new(kalamdb_store::SharedTableStore::new(test_db.db.clone()).unwrap());
        let shared_table_service = Arc::new(crate::services::SharedTableService::new(
            shared_table_store.clone(),
            kalam_sql.clone(),
        ));
        let stream_table_store =
            Arc::new(kalamdb_store::StreamTableStore::new(test_db.db.clone()).unwrap());
        let stream_table_service = Arc::new(crate::services::StreamTableService::new(
            stream_table_store.clone(),
            kalam_sql.clone(),
        ));

        let executor = SqlExecutor::new(
            namespace_service,
            session_context,
            user_table_service,
            shared_table_service,
            stream_table_service,
        )
        .with_stores(
            user_table_store,
            shared_table_store,
            stream_table_store,
            kalam_sql.clone(),
        );

        // Create a test stream table entry in system_tables
        let table = kalamdb_sql::models::Table {
            table_id: "app:events".to_string(),
            table_name: "events".to_string(),
            namespace: "app".to_string(),
            table_type: "Stream".to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            storage_location: String::new(),
            storage_id: Some("local".to_string()),
            use_user_storage: false,
            flush_policy: String::new(),
            schema_version: 1,
            deleted_retention_hours: 0,
        };

        kalam_sql.insert_table(&table).unwrap();

        // Execute DESCRIBE TABLE
        let result = executor
            .execute("DESCRIBE TABLE app.events", None)
            .await
            .unwrap();

        // Verify stream table shows appropriate properties
        match result {
            ExecutionResult::RecordBatch(batch) => {
                assert_eq!(batch.num_columns(), 2);
                assert!(batch.num_rows() > 0);

                // Convert to string to check content
                let properties_col = batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap();

                let values_col = batch
                    .column(1)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap();

                // Check that table_type is Stream
                let mut found_stream_type = false;
                let mut found_note = false;
                for i in 0..batch.num_rows() {
                    if properties_col.value(i) == "table_type" {
                        assert_eq!(values_col.value(i), "Stream");
                        found_stream_type = true;
                    }
                    if properties_col.value(i) == "note" {
                        assert!(values_col.value(i).contains("NO _updated/_deleted"));
                        assert!(values_col.value(i).contains("ephemeral"));
                        found_note = true;
                    }
                }
                assert!(found_stream_type, "Should show table_type = Stream");
                assert!(found_note, "Should show note about NO system columns");
            }
            _ => panic!("Expected RecordBatch result"),
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
