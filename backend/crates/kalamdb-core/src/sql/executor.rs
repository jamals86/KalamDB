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

use crate::auth::rbac;
use crate::catalog::{NamespaceId, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::schema_registry::arrow_schema::ArrowSchemaWithOptions;
use crate::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::stores::system_table::{SharedTableStoreExt, UserTableStoreExt};
use crate::tables::{new_user_table_store, SharedTableStore, StreamTableStore, UserTableStore};
use crate::tables::{SharedTableProvider, StreamTableProvider};
// All system tables now use EntityStore-based v2 providers
use crate::tables::system::JobsTableProvider;
use crate::tables::system::UsersTableProvider;
// Phase 15 (008-schema-consolidation): Import EntityStore trait for TableSchemaStore
use datafusion::arrow::array::{ArrayRef, StringArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::prelude::SessionContext;
use datafusion::sql::sqlparser;
use kalamdb_auth::password::{self, PasswordPolicy};
use kalamdb_commons::models::{
    AuditLogId, NamespaceId as CommonNamespaceId, NodeId, StorageId, TableId, UserName,
};
use kalamdb_commons::system::{AuditLogEntry, Namespace};
use kalamdb_commons::{JobStatus, Role, TableAccess};
use kalamdb_sql::ddl::{
    parse_job_command, AlterNamespaceStatement, AlterUserStatement, CreateNamespaceStatement,
    CreateTableStatement, CreateUserStatement, DescribeTableStatement, DropNamespaceStatement,
    DropTableStatement, DropUserStatement, KillLiveQueryStatement, ShowNamespacesStatement,
    ShowTableStatsStatement, ShowTablesStatement,
};
use kalamdb_sql::statement_classifier::SqlStatement;
use kalamdb_sql::KalamSql;
use kalamdb_sql::RocksDbAdapter;
use serde_json::json;
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

/// Execution context for SQL queries
///
/// Contains authentication and authorization information for the current query execution.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// User ID of the user executing the query
    pub user_id: UserId,
    /// Role of the user (User, Service, Dba, System)
    pub user_role: Role,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new(user_id: UserId, user_role: Role) -> Self {
        Self { user_id, user_role }
    }

    /// Create an anonymous execution context with User role
    pub fn anonymous() -> Self {
        Self {
            user_id: UserId::from("anonymous"),
            user_role: Role::User,
        }
    }

    /// Check if the user is an administrator (Dba or System role)
    pub fn is_admin(&self) -> bool {
        matches!(self.user_role, Role::Dba | Role::System)
    }

    /// Check if the user is a system user
    pub fn is_system(&self) -> bool {
        matches!(self.user_role, Role::System)
    }
}

/// Additional metadata for a statement execution (per request).
#[derive(Debug, Clone, Default)]
pub struct ExecutionMetadata {
    /// Optional client IP address (for audit logging).
    pub ip_address: Option<String>,
}

/// SQL executor
///
/// Executes SQL statements by dispatching to appropriate handlers.
/// All dependencies are fetched from AppContext.
pub struct SqlExecutor {
    /// Reference to AppContext for all dependencies
    app_context: Arc<crate::app_context::AppContext>,
    
    /// Services (zero-sized, created on-demand)
    namespace_service: Arc<NamespaceService>,
    user_table_service: Arc<UserTableService>,
    shared_table_service: Arc<SharedTableService>,
    stream_table_service: Arc<StreamTableService>,
    table_deletion_service: Arc<TableDeletionService>,
    
    /// Password complexity enforcement
    enforce_password_complexity: bool,
}

/// UPDATE statement parsed info
#[derive(Debug)]
struct UpdateInfo {
    namespace: NamespaceId,
    table: TableName,
    set_values: Vec<(String, serde_json::Value)>,
    where_clause: String,
}

/// DELETE statement parsed info
#[derive(Debug)]
struct DeleteInfo {
    namespace: NamespaceId,
    table: TableName,
    where_clause: String,
}

impl SqlExecutor {
    /// Create a new SQL executor with only AppContext
    ///
    /// All dependencies are fetched from AppContext.
    ///
    /// # Arguments
    /// - `app_context`: The application context containing all shared resources
    /// - `enforce_password_complexity`: Whether to enforce password complexity rules
    pub fn new(
        app_context: Arc<crate::app_context::AppContext>,
        enforce_password_complexity: bool,
    ) -> Self {
        // Create zero-sized service instances
        let kalam_sql = app_context.kalam_sql();
        let namespace_service = Arc::new(NamespaceService::new(kalam_sql));
        let user_table_service = Arc::new(UserTableService::new());
        let shared_table_service = Arc::new(SharedTableService::new());
        let stream_table_service = Arc::new(StreamTableService::new());
        let table_deletion_service = Arc::new(TableDeletionService::new());
        
        Self {
            app_context,
            namespace_service,
            user_table_service,
            shared_table_service,
            stream_table_service,
            table_deletion_service,
            enforce_password_complexity,
        }
    }

    /// Expose a clone of the underlying RocksDbAdapter
    pub fn get_rocks_adapter(&self) -> Option<Arc<RocksDbAdapter>> {
        Some(Arc::new(self.app_context.kalam_sql().adapter().clone()))
    }
    
    // ===== Helper methods to get dependencies from AppContext =====
    
    fn kalam_sql(&self) -> Arc<KalamSql> {
        self.app_context.kalam_sql()
    }
    
    fn user_table_store(&self) -> Arc<UserTableStore> {
        self.app_context.user_table_store()
    }
    
    fn shared_table_store(&self) -> Arc<SharedTableStore> {
        self.app_context.shared_table_store()
    }
    
    fn stream_table_store(&self) -> Arc<StreamTableStore> {
        self.app_context.stream_table_store()
    }
    
    fn storage_backend(&self) -> Arc<dyn kalamdb_store::StorageBackend> {
        self.app_context.storage_backend()
    }
    
    fn storage_registry(&self) -> Arc<crate::storage::StorageRegistry> {
        self.app_context.storage_registry()
    }
    
    fn job_manager(&self) -> Arc<dyn crate::jobs::JobsManager> {
        self.app_context.job_manager()
    }
    
    fn live_query_manager(&self) -> Arc<crate::live_query::LiveQueryManager> {
        self.app_context.live_query_manager()
    }
    
    fn schema_registry(&self) -> Arc<crate::schema_registry::registry::SchemaRegistry> {
        self.app_context.schema_registry()
    }
    
    fn unified_cache(&self) -> Arc<crate::catalog::SchemaCache> {
        self.app_context.schema_cache()
    }
    
    fn jobs_table_provider(&self) -> Arc<crate::tables::system::JobsTableProvider> {
        self.app_context.system_tables().jobs()
    }
    
    fn users_table_provider(&self) -> Arc<UsersTableProvider> {
        self.app_context.system_tables().users()
    }
    
    fn session_factory(&self) -> Arc<DataFusionSessionFactory> {
        self.app_context.session_factory()
    }
        namespace_service: Arc<NamespaceService>,
        user_table_service: Arc<UserTableService>,
        shared_table_service: Arc<SharedTableService>,
        stream_table_service: Arc<StreamTableService>,
    ) -> Self {
        let session_factory = DataFusionSessionFactory::default();
        Self {
            namespace_service,
            user_table_service,
            shared_table_service,
            stream_table_service,
            table_deletion_service: None,
            user_table_store: None,
            shared_table_store: None,
            stream_table_store: None,
            kalam_sql: None,
            storage_backend: None, // Phase 14: Initialize storage backend
            session_factory,
            jobs_table_provider: None,
            users_table_provider: None,
            storage_registry: None,
            job_manager: None,
            live_query_manager: None,
            schema_store: None,
            unified_cache: None, // Phase 10: Unified cache (set via with_storage_registry)
            enforce_password_complexity: false,
        }
    }

    /// Set the storage registry (optional, for storage template validation)
    pub fn with_storage_registry(mut self, registry: Arc<crate::storage::StorageRegistry>) -> Self {
        // Phase 10: Initialize unified SchemaCache (replaces old dual-cache architecture)
        let unified_cache = Arc::new(
            crate::catalog::SchemaCache::new(10000, Some(registry.clone()))
        );
        self.unified_cache = Some(unified_cache);
        
        self.storage_registry = Some(registry);
        self
    }

    /// Set the job manager (optional, for FLUSH TABLE support)
    pub fn with_job_manager(mut self, job_manager: Arc<dyn crate::jobs::JobsManager>) -> Self {
        self.job_manager = Some(job_manager);
        self
    }

    /// Set the live query manager for subscription coordination (optional)
    pub fn with_live_query_manager(
        mut self,
        manager: Arc<crate::live_query::LiveQueryManager>,
    ) -> Self {
        self.live_query_manager = Some(manager);
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
        // Phase 14: Note - we can't get kalamdb_store::StorageBackend from KalamSql yet
        // because it uses kalamdb_commons::StorageBackend. This will be fixed when
        // kalamdb-sql is migrated to use kalamdb_store::StorageBackend.
        // For now, storage_backend must be set separately via with_storage_backend()
        self.kalam_sql = Some(kalam_sql);
        self
    }

    /// Phase 14: Set the storage backend for EntityStore-based providers
    pub fn with_storage_backend(mut self, backend: Arc<dyn kalamdb_store::StorageBackend>) -> Self {
        // Phase 14: Use storage backend for new EntityStore-based providers
        self.storage_backend = Some(backend.clone());
        self.users_table_provider = Some(Arc::new(UsersTableProvider::new(backend.clone())));
        self.jobs_table_provider = Some(Arc::new(JobsTableProvider::new(backend)));
        self
    }

    /// Enable or disable password complexity enforcement.
    pub fn with_password_complexity(mut self, enforce: bool) -> Self {
        self.enforce_password_complexity = enforce;
        self
    }

    /// Phase 15 (008-schema-consolidation): Set schema store for DESCRIBE TABLE
    /// Note: SchemaCache is now part of unified_cache, set via with_storage_registry()
    pub fn with_schema_store(
        mut self,
        schema_store: Arc<crate::tables::system::schemas::TableSchemaStore>,
    ) -> Self {
        self.schema_store = Some(schema_store);
        self
    }

    /// Load and register existing tables from system_tables on initialization
    pub async fn load_existing_tables(&self, _default_user_id: UserId) -> Result<(), KalamDbError> {
        let kalam_sql = Some(&self.kalam_sql()).ok_or_else(|| {
            KalamDbError::InvalidOperation(
                "Cannot load tables - KalamSQL not configured".to_string(),
            )
        })?;

        // Get all tables from system_tables
        let tables = kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {:?}", e)))?;

        for table in tables {
            // Phase 2b: Get schema from information_schema.tables (TableDefinition.schema_history)
            let namespace_id = NamespaceId::from(table.namespace.as_str());
            let table_name = TableName::from(table.table_name.as_str());
            let table_def = match kalam_sql.get_table_definition(&namespace_id, &table_name) {
                Ok(Some(def)) => def,
                Ok(None) => {
                    // No TableDefinition found - skip this table (might be legacy)
                    continue;
                }
                Err(e) => {
                    // Log error but continue loading other tables
                    eprintln!(
                        "Warning: Failed to load schema for {}.{}: {:?}",
                        table.namespace, table.table_name, e
                    );
                    continue;
                }
            };

            // Get the latest schema from schema_history
            if table_def.schema_history.is_empty() {
                eprintln!(
                    "Warning: No schema history for {}.{}",
                    table.namespace, table.table_name
                );
                continue;
            }

            // Use the latest schema version (last in array)
            let latest_schema_version =
                &table_def.schema_history[table_def.schema_history.len() - 1];
            let schema_with_opts =
                ArrowSchemaWithOptions::from_json_string(&latest_schema_version.arrow_schema_json)
                    .map_err(|e| {
                        KalamDbError::Other(format!(
                            "Failed to parse schema for {}.{}: {:?}",
                            table.namespace, table.table_name, e
                        ))
                    })?;
            let _schema = schema_with_opts.schema;

            // Parse namespace and table_name
            // let namespace_id = NamespaceId::from(table.namespace.as_str());
            // let table_name = TableName::from(table.table_name.as_str());
            // let table_type = TableType::from_str(&table.table_type).ok_or_else(|| {
            //     KalamDbError::Other(format!("Invalid table type: {}", table.table_type))
            // })?;

            // Register based on table type
            // self.register_table_with_datafusion(
            //     &namespace_id,
            //     &table_name,
            //     table_type,
            //     schema,
            //     default_user_id.clone(),
            // )
            // .await?;
        }

        Ok(())
    }

    /// Register a table with DataFusion SessionContext
    async fn register_table_with_datafusion(
        &self,
        session: &SessionContext,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        table_type: TableType,
        schema: SchemaRef,
        _default_user_id: UserId,
    ) -> Result<(), KalamDbError> {
        use datafusion::catalog::memory::MemorySchemaProvider;

        // Use the "kalam" catalog (configured in DataFusionSessionFactory)
        let catalog_name = "kalam";

        let catalog = session
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

        // Phase 10: Table will be added to unified_cache in cache_table_metadata() 
        // after full metadata is available (T311-T314)

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

                // Phase 10: Create Arc<TableId> once for zero-allocation cache lookups
                let table_id = Arc::new(kalamdb_commons::models::TableId::new(
                    namespace_id.clone(),
                    table_name.clone(),
                ));

                // Get unified_cache for provider
                let unified_cache = self.unified_cache.as_ref()
                    .ok_or_else(|| KalamDbError::InvalidOperation("Unified cache not initialized".to_string()))?
                    .clone();

                let provider = Arc::new(SharedTableProvider::new(
                    table_id,
                    unified_cache,
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

                // Phase 10: Create Arc<TableId> once for zero-allocation cache lookups
                let table_id = Arc::new(kalamdb_commons::models::TableId::new(
                    namespace_id.clone(),
                    table_name.clone(),
                ));

                // Get unified_cache for provider
                let unified_cache = self.unified_cache.as_ref()
                    .ok_or_else(|| KalamDbError::InvalidOperation("Unified cache not initialized".to_string()))?
                    .clone();

                let mut provider = StreamTableProvider::new(
                    table_id,
                    unified_cache,
                    schema,
                    store.clone(),
                    None,  // retention_seconds - TODO: get from table metadata
                    false, // ephemeral - TODO: get from table metadata
                    None,  // max_buffer - TODO: get from table metadata
                );

                // Wire through LiveQueryManager for WebSocket notifications (T154)
                if let Some(manager) = &self.live_query_manager {
                    provider = provider.with_live_query_manager(Arc::clone(manager));
                }

                let provider = Arc::new(provider);

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
        let storage_id = storage_id.unwrap_or_else(StorageId::local);

        // T167b & T167c: Validate storage_id exists in system.storages (NOT NULL enforcement)
        if let Some(kalam_sql) = &self.kalam_sql {
            let storage_exists = kalam_sql.get_storage(&storage_id).is_ok();
            if !storage_exists {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Storage '{}' does not exist in system.storages",
                    storage_id
                )));
            }
        }

        Ok(storage_id)
    }

    /// Ensure the namespace referenced by a CREATE TABLE statement exists.
    ///
    /// We consult NamespaceService so the check shares the same RocksDB view as
    /// subsequent metadata writes; this keeps table creation from racing ahead
    /// of namespace persistence during concurrent workloads.
    fn ensure_namespace_exists(
        &self,
        namespace_id: &CommonNamespaceId,
    ) -> Result<(), KalamDbError> {
        let namespace_name = namespace_id.as_str();
        let exists = self.namespace_service.namespace_exists(namespace_name)?;
        if !exists {
            return Err(KalamDbError::InvalidOperation(format!(
                "Namespace '{}' does not exist. Create it first with CREATE NAMESPACE {}.",
                namespace_name, namespace_name
            )));
        }
        Ok(())
    }

    /// Helper (T311): Cache newly created table in unified SchemaCache
    ///
    /// Builds CachedTableData with resolved storage path template and inserts into cache.
    ///
    /// # Arguments
    /// * `namespace` - Namespace identifier
    /// * `table_name` - Table name
    /// * `table_type` - Table type
    /// * `storage_id` - Storage configuration reference
    /// * `flush_policy_ddl` - Flush policy from DDL parser
    /// * `arrow_schema` - Arrow schema from CREATE TABLE statement
    /// * `schema_version` - Current schema version
    /// * `deleted_retention_hours` - Soft delete retention period
    fn cache_table_metadata(
        &self,
        namespace: &CommonNamespaceId,
        table_name: &kalamdb_commons::models::TableName,
        table_type: TableType,
        storage_id: &StorageId,
        flush_policy_ddl: Option<kalamdb_sql::ddl::FlushPolicy>,
        arrow_schema: Arc<arrow::datatypes::Schema>,
        schema_version: u32,
        deleted_retention_hours: Option<u32>,
    ) -> Result<(), KalamDbError> {
        // Skip caching if unified_cache not initialized
        let cache = match &self.unified_cache {
            Some(c) => c,
            None => return Ok(()), // Cache not initialized - skip
        };

        // Convert DDL FlushPolicy to core FlushPolicy
        use kalamdb_sql::ddl::FlushPolicy as DdlFlushPolicy;
        let flush_policy = match flush_policy_ddl {
            Some(DdlFlushPolicy::RowLimit { row_limit }) => {
                crate::flush::FlushPolicy::RowLimit { row_limit }
            }
            Some(DdlFlushPolicy::TimeInterval { interval_seconds }) => {
                crate::flush::FlushPolicy::TimeInterval { interval_seconds }
            }
            Some(DdlFlushPolicy::Combined {
                row_limit,
                interval_seconds,
            }) => crate::flush::FlushPolicy::Combined {
                row_limit,
                interval_seconds,
            },
            None => crate::flush::FlushPolicy::default(),
        };

        // Build TableDefinition from Arrow schema
        // For now, create a minimal TableDefinition - we can enhance this later
        // to extract column details from Arrow schema
        use kalamdb_commons::models::schemas::{TableDefinition, TableOptions};
        
        let table_options = match table_type {
            TableType::User => TableOptions::user(),
            TableType::Shared => TableOptions::shared(),
            TableType::Stream => TableOptions::stream(deleted_retention_hours.unwrap_or(3600) as u64),
            TableType::System => TableOptions::system(),
        };

        let table_def = TableDefinition::new(
            kalamdb_commons::models::NamespaceId::new(namespace.as_str()),
            table_name.clone(),
            table_type,
            vec![], // Empty columns for now - TODO: convert from Arrow schema
            table_options,
            None, // No table comment
        ).map_err(|e| KalamDbError::Other(format!("Failed to create TableDefinition: {}", e)))?;

        // Resolve partial storage path template
        let namespace_id = crate::catalog::NamespaceId::new(namespace.as_str());
        let table_name_id = crate::catalog::TableName::new(table_name.as_str());
        
        let storage_path_template = cache.resolve_storage_path_template(
            &namespace_id,
            &table_name_id,
            table_type,
            storage_id,
        )?;

        // Build CachedTableData
        let table_id = TableId::new(namespace_id, table_name_id);
        let cached_data = Arc::new(crate::catalog::CachedTableData::new(
            table_id.clone(),
            table_type,
            chrono::Utc::now(),
            Some(storage_id.clone()),
            flush_policy,
            storage_path_template,
            schema_version,
            deleted_retention_hours,
            Arc::new(table_def),
        ));

        // Insert into cache
        cache.insert(table_id, cached_data);

        Ok(())
    }

    /// Create ExecutionContext from user_id
    ///
    /// Looks up the user in the database to determine their role.
    /// If user_id is None or user not found, returns an elevated anonymous context for tests/dev.
    fn create_execution_context(
        &self,
        user_id: Option<&UserId>,
    ) -> Result<ExecutionContext, KalamDbError> {
        let user_id = match user_id {
            Some(uid) => uid.clone(),
            None => {
                // Test harnesses and some internal callers pass None. Grant DBA privileges
                // to allow DDL (e.g., CREATE/DROP NAMESPACE) without explicit auth context.
                // In production, API layers always supply an authenticated user id.
                return Ok(ExecutionContext::new(UserId::from("anonymous"), Role::Dba));
            }
        };

        // Look up user to get their role
        if let Some(kalam_sql) = &self.kalam_sql {
            match kalam_sql.get_user_by_id(&user_id) {
                Ok(Some(user)) => {
                    // Check if user is soft-deleted
                    if user.deleted_at.is_some() {
                        log::warn!("User '{}' is deleted, rejecting request", user_id.as_str());
                        return Err(KalamDbError::PermissionDenied(
                            "User account has been deleted".to_string(),
                        ));
                    }
                    return Ok(ExecutionContext::new(user_id, user.role));
                }
                Ok(None) => {
                    // User not found - treat as anonymous
                    log::warn!(
                        "User '{}' not found in database, using anonymous context",
                        user_id.as_str()
                    );
                    return Ok(ExecutionContext::anonymous());
                }
                Err(e) => {
                    log::error!("Failed to look up user '{}': {:?}", user_id.as_str(), e);
                    // On error, default to anonymous for safety
                    return Ok(ExecutionContext::anonymous());
                }
            }
        }

        // If KalamSQL not initialized, default to User role
        Ok(ExecutionContext::new(user_id, Role::User))
    }

    /// Check if user is authorized to execute a SQL statement
    ///
    /// Authorization rules:
    /// - System and DBA users can execute all statements
    /// - Regular users can only access their own USER tables
    /// - System tables (system.*, information_schema.*) are readable by all authenticated users
    /// - DDL operations (CREATE/ALTER/DROP NAMESPACE, STORAGE, etc.) require admin privileges
    fn check_authorization(&self, ctx: &ExecutionContext, sql: &str) -> Result<(), KalamDbError> {
        // Admin users (DBA, System) can do anything
        if ctx.is_admin() {
            return Ok(());
        }

        // Classify the statement to determine authorization requirements
        let stmt_type = SqlStatement::classify(sql);

        match stmt_type {
            // Storage and global operations require admin privileges
            SqlStatement::CreateStorage
            | SqlStatement::AlterStorage
            | SqlStatement::DropStorage
            | SqlStatement::KillJob => {
                Err(KalamDbError::Unauthorized(format!(
                    "Admin privileges required to execute {}",
                    sql.lines().next().unwrap_or("this statement")
                )))
            }

            // User management requires admin privileges (except for self-modification)
            SqlStatement::CreateUser | SqlStatement::DropUser => {
                Err(KalamDbError::Unauthorized(
                    "Admin privileges required for user management".to_string(),
                ))
            }

            // ALTER USER allowed for self (changing own password), admin for others
            SqlStatement::AlterUser => {
                // Extract username from ALTER USER statement
                // For now, we'll allow it and let the execute_alter_user method do the check
                // TODO: Parse and verify user is modifying their own account
                Ok(())
            }

            // Namespace DDL requires admin privileges
            SqlStatement::CreateNamespace
            | SqlStatement::AlterNamespace
            | SqlStatement::DropNamespace => {
                Err(KalamDbError::Unauthorized(
                    "Admin privileges required for namespace operations".to_string(),
                ))
            }

            // Read-only operations on system tables are allowed for all authenticated users
            SqlStatement::ShowNamespaces
            | SqlStatement::ShowTables
            | SqlStatement::ShowStorages
            | SqlStatement::ShowStats
            | SqlStatement::DescribeTable => {
                Ok(())
            }

            // CREATE TABLE, DROP TABLE, FLUSH TABLE, ALTER TABLE - check table ownership in execute methods
            SqlStatement::CreateTable
            | SqlStatement::AlterTable
            | SqlStatement::DropTable
            | SqlStatement::FlushTable
            | SqlStatement::FlushAllTables => {
                // Table-level authorization will be checked in the execution methods
                Ok(())
            }

            // SELECT, INSERT, UPDATE, DELETE - check table access in execution
            SqlStatement::Select
            | SqlStatement::Insert
            | SqlStatement::Update
            | SqlStatement::Delete => {
                // Query-level authorization will be enforced by using per-user sessions
                // User tables are filtered by user_id in UserTableProvider
                Ok(())
            }

            // Subscriptions, transactions, and other operations allowed for all users
            SqlStatement::Subscribe
            | SqlStatement::KillLiveQuery
            | SqlStatement::BeginTransaction
            | SqlStatement::CommitTransaction
            | SqlStatement::RollbackTransaction => {
                Ok(())
            }

            SqlStatement::Unknown => {
                // Unknown statements will fail in execute anyway
                Ok(())
            }
        }
    }

    /// Execute a SQL statement

    /// New stateless API: requires &SessionContext to be passed in
    pub async fn execute(
        &self,
        session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        self.execute_with_metadata(session, sql, exec_ctx, None).await
    }

    /// Execute a SQL statement with additional request metadata (e.g., client IP).
    pub async fn execute_with_metadata(
        &self,
        session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
        metadata: Option<&ExecutionMetadata>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Check authorization before executing
        self.check_authorization(exec_ctx, sql)?;

        // Classify the SQL statement and dispatch to appropriate handler
        match SqlStatement::classify(sql) {
            SqlStatement::CreateNamespace => self.execute_create_namespace(session, sql, exec_ctx).await,
            SqlStatement::AlterNamespace => self.execute_alter_namespace(session, sql, exec_ctx).await,
            SqlStatement::DropNamespace => self.execute_drop_namespace(session, sql, exec_ctx).await,
            SqlStatement::ShowNamespaces => self.execute_show_namespaces(session, sql, exec_ctx).await,
            SqlStatement::CreateStorage => self.execute_create_storage(session, sql, exec_ctx).await,
            SqlStatement::AlterStorage => self.execute_alter_storage(session, sql, exec_ctx).await,
            SqlStatement::DropStorage => self.execute_drop_storage(session, sql, exec_ctx).await,
            SqlStatement::ShowStorages => self.execute_show_storages(session, sql, exec_ctx).await,
            SqlStatement::CreateTable => self.execute_create_table(session, sql, exec_ctx).await,
            SqlStatement::AlterTable => self.execute_alter_table(session, sql, exec_ctx, metadata).await,
            SqlStatement::DropTable => self.execute_drop_table(session, sql, exec_ctx).await,
            SqlStatement::ShowTables => self.execute_show_tables(session, sql, exec_ctx).await,
            SqlStatement::DescribeTable => self.execute_describe_table(session, sql, exec_ctx).await,
            SqlStatement::ShowStats => self.execute_show_table_stats(session, sql, exec_ctx).await,
            SqlStatement::FlushTable => self.execute_flush_table(session, sql, exec_ctx).await,
            SqlStatement::FlushAllTables => self.execute_flush_all_tables(session, sql, exec_ctx).await,
            SqlStatement::KillJob => self.execute_kill_job(session, sql, exec_ctx).await,
            SqlStatement::KillLiveQuery => self.execute_kill_live_query(session, sql, exec_ctx).await,
            SqlStatement::BeginTransaction => self.execute_begin_transaction().await,
            SqlStatement::CommitTransaction => self.execute_commit_transaction().await,
            SqlStatement::RollbackTransaction => self.execute_rollback_transaction().await,
            SqlStatement::Subscribe => self.execute_subscribe(session, sql, exec_ctx).await,
            SqlStatement::CreateUser => self.execute_create_user(session, sql, exec_ctx, metadata).await,
            SqlStatement::AlterUser => self.execute_alter_user(session, sql, exec_ctx, metadata).await,
            SqlStatement::DropUser => self.execute_drop_user(session, sql, exec_ctx, metadata).await,
            SqlStatement::Update => self.execute_update(session, sql, exec_ctx).await,
            SqlStatement::Delete => self.execute_delete(session, sql, exec_ctx).await,
            SqlStatement::Insert => {
                // Check write permissions for shared tables before executing
                let referenced_tables = Self::extract_table_references(sql);
                self.check_write_permissions(Some(&exec_ctx.user_id), &referenced_tables)?;
                self.execute_datafusion_query_with_tables(session, sql, exec_ctx, referenced_tables)
                    .await
            }
            SqlStatement::Select => {
                // Extract referenced tables from SQL and load only those
                let referenced_tables = Self::extract_table_references(sql);
                self.execute_datafusion_query_with_tables(session, sql, exec_ctx, referenced_tables)
                    .await
            }
            SqlStatement::Unknown => Err(KalamDbError::InvalidSql(format!(
                "Unsupported SQL statement: {}",
                sql.lines().next().unwrap_or("")
            ))),
        }
    }

    async fn execute_begin_transaction(&self) -> Result<ExecutionResult, KalamDbError> {
        Ok(ExecutionResult::Success(
            "Transaction started (BEGIN)".to_string(),
        ))
    }

    async fn execute_commit_transaction(&self) -> Result<ExecutionResult, KalamDbError> {
        Ok(ExecutionResult::Success(
            "Transaction committed (no-op)".to_string(),
        ))
    }

    async fn execute_rollback_transaction(&self) -> Result<ExecutionResult, KalamDbError> {
        Ok(ExecutionResult::Success(
            "Transaction rollback (not supported - no changes applied)".to_string(),
        ))
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
                    // INSERT INTO table
                    let table_str = insert.table.to_string();
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

    /// Execute a query that only accesses system tables (optimized - no user table loading)
    /// Create a fresh SessionContext for a specific user with only their tables registered
    ///
    /// This ensures user data isolation by creating a dedicated session with user-scoped TableProviders.
    fn register_system_tables_in_session(
        &self,
        session: &SessionContext,
        kalam_sql: &Arc<KalamSql>,
    ) -> Result<(), KalamDbError> {
        use datafusion::catalog::memory::MemorySchemaProvider;

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

        // Get storage backend for EntityStore-based providers
        let backend = self.storage_backend.as_ref().ok_or_else(|| {
            KalamDbError::InvalidOperation(
                "Storage backend not configured for system tables".to_string(),
            )
        })?;

        // Use EntityStore-based v2 providers for all system tables
        use crate::tables::system::{
            NamespacesTableProvider, TablesTableProvider, UsersTableProvider,
        };

        system_schema
            .register_table(
                "namespaces".to_string(),
                Arc::new(NamespacesTableProvider::new(backend.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to register system.namespaces: {}", e))
            })?;

        system_schema
            .register_table(
                "tables".to_string(),
                Arc::new(TablesTableProvider::new(backend.clone())),
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to register system.tables: {}", e)))?;

        system_schema
            .register_table(
                "users".to_string(),
                Arc::new(UsersTableProvider::new(backend.clone())),
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to register system.users: {}", e)))?;

        // Register additional system tables (using EntityStore-based v2 providers)
        use crate::tables::system::{
            JobsTableProvider, LiveQueriesTableProvider, StoragesTableProvider,
        };

        system_schema
            .register_table(
                "storages".to_string(),
                Arc::new(StoragesTableProvider::new(backend.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to register system.storages: {}", e))
            })?;
        system_schema
            .register_table(
                "live_queries".to_string(),
                Arc::new(LiveQueriesTableProvider::new(backend.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to register system.live_queries: {}", e))
            })?;

        system_schema
            .register_table(
                "jobs".to_string(),
                Arc::new(JobsTableProvider::new(backend.clone())),
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to register system.jobs: {}", e)))?;

        // Register information_schema tables
        use crate::tables::system::{
            InformationSchemaColumnsProvider, InformationSchemaTablesProvider,
        };

        // Check if information_schema exists, if not create it
        if catalog.schema("information_schema").is_none() {
            let info_schema = Arc::new(MemorySchemaProvider::new());
            catalog
                .register_schema("information_schema", info_schema)
                .map_err(|e| {
                    KalamDbError::Other(format!("Failed to register information_schema: {}", e))
                })?;
        }

        let info_schema = catalog.schema("information_schema").ok_or_else(|| {
            KalamDbError::Other("information_schema not found after registration".to_string())
        })?;

        info_schema
            .register_table(
                "tables".to_string(),
                Arc::new(InformationSchemaTablesProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to register information_schema.tables: {}",
                    e
                ))
            })?;

        info_schema
            .register_table(
                "columns".to_string(),
                Arc::new(InformationSchemaColumnsProvider::new(kalam_sql.clone())),
            )
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to register information_schema.columns: {}",
                    e
                ))
            })?;

        Ok(())
    }

    async fn create_user_session_context(
        &self,
        user_id: &UserId,
        table_filter: Option<&std::collections::HashSet<String>>,
    ) -> Result<SessionContext, KalamDbError> {
        use datafusion::catalog::memory::MemorySchemaProvider;

        // Create fresh SessionContext using the shared RuntimeEnv (efficient - no memory duplication)
        let user_session = self.session_factory.create_session();

        // Get KalamSQL for querying system tables
        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("KalamSQL not configured".to_string()))?;

        // Look up user role for RBAC authorization on shared tables
        let user_role = if user_id.as_str() == "anonymous" {
            Role::User
        } else {
            match kalam_sql.get_user_by_id(user_id) {
                Ok(Some(user)) => user.role,
                Ok(None) => {
                    log::warn!(
                        "User '{}' not found, defaulting to User role",
                        user_id.as_str()
                    );
                    Role::User
                }
                Err(e) => {
                    log::error!(
                        "Failed to lookup user '{}': {}, defaulting to User role",
                        user_id.as_str(),
                        e
                    );
                    Role::User
                }
            }
        };

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
                        || filter.contains(t.table_name.as_str())  // Also match unqualified
                        || filter.contains(&format!("system.{}", t.table_name.as_str()))
                    // System table reference
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

        // Register shared tables (with RBAC authorization check)
        for table in tables_to_load
            .iter()
            .filter(|t| t.table_type == TableType::Shared)
        {
            let table_name = table.table_name.clone();
            let namespace_id = table.namespace.clone();
            let table_id = format!("{}:{}", table.namespace, table.table_name);

            // RBAC: Check if user has access to this shared table
            // Note: owner_id field not yet implemented, so is_owner = false for now
            // This means Restricted tables will only be accessible to Service/Dba/System roles
            let access_level = table.access_level.unwrap_or(TableAccess::Private);
            let is_owner = false; // TODO: Implement owner tracking in system.tables

            if !rbac::can_access_shared_table(access_level, is_owner, user_role) {
                // Skip tables the user cannot access - don't register them
                log::debug!(
                    "User '{}' (role: {:?}) does not have access to shared table '{}.{}' (access_level: {:?})",
                    user_id.as_str(),
                    user_role,
                    namespace_id.as_str(),
                    table_name.as_str(),
                    access_level
                );
                continue;
            }

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

            // Get shared table store
            let store = self.shared_table_store.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("SharedTableStore not configured".to_string())
            })?;

            // Phase 10: Create Arc<TableId> once for zero-allocation cache lookups
            let table_id = Arc::new(kalamdb_commons::models::TableId::new(
                namespace_id.clone(),
                table_name.clone(),
            ));

            // Get unified_cache for provider
            let unified_cache = self.unified_cache.as_ref()
                .ok_or_else(|| KalamDbError::InvalidOperation("Unified cache not initialized".to_string()))?
                .clone();

            // Try to reuse a cached provider; otherwise create and cache it
            let provider: Arc<dyn datafusion::datasource::TableProvider + Send + Sync> =
                if let Some(existing) = unified_cache.get_provider(&*table_id) {
                    existing
                } else {
                    let created = Arc::new(SharedTableProvider::new(
                        table_id.clone(),
                        unified_cache.clone(),
                        schema,
                        store.clone(),
                    ));
                    unified_cache.insert_provider(table_id.as_ref().clone(), created.clone());
                    created
                };

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
        for table in tables_to_load
            .iter()
            .filter(|t| t.table_type == TableType::User)
        {
            let table_name = table.table_name.clone();
            let namespace_id = table.namespace.clone();
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

            // Create table-specific store with proper partition name
            // Instead of using the generic self.user_table_store, create a store
            // specific to this table with the correct column family name
            let backend = self.storage_backend.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("Storage backend not configured".to_string())
            })?;

            let table_store = Arc::new(crate::tables::new_user_table_store(
                backend.clone(),
                &namespace_id,
                &table_name,
            ));

            // Phase 10: Create Arc<TableId> once for zero-allocation cache lookups
            let table_id = Arc::new(kalamdb_commons::models::TableId::new(
                namespace_id.clone(),
                table_name.clone(),
            ));

            // Get unified_cache for provider
            let unified_cache = self.unified_cache.as_ref()
                .ok_or_else(|| KalamDbError::InvalidOperation("Unified cache not initialized".to_string()))?
                .clone();

            // Phase 3C: Get or create UserTableShared (cached singleton per table)
            let shared = if let Some(cached_shared) = unified_cache.get_user_table_shared(&table_id) {
                // Reuse existing shared state
                cached_shared
            } else {
                // Create new shared state
                // Note: Builder pattern doesn't work well with Arc, so we accept immutable shared state
                // LiveQueryManager and StorageRegistry can be added in future versions if needed per-table
                let new_shared = crate::tables::base_table_provider::UserTableShared::new(
                    table_id.clone(),
                    unified_cache.clone(),
                    schema,
                    table_store,
                );

                unified_cache.insert_user_table_shared((*table_id).clone(), new_shared.clone());
                new_shared
            };

            // Create per-request UserTableAccess wrapper with CURRENT user_id (critical for data isolation)
            let provider = crate::tables::UserTableAccess::new(
                shared,
                user_id.clone(),
                user_role,
            );

            let provider = Arc::new(provider);

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
        for table in tables_to_load
            .iter()
            .filter(|t| t.table_type == TableType::Stream)
        {
            let table_name = table.table_name.clone();
            let namespace_id = table.namespace.clone();
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

            // Get stream table store
            let store = self.stream_table_store.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("StreamTableStore not configured".to_string())
            })?;

            // Phase 10: Create Arc<TableId> once for zero-allocation cache lookups
            let table_id = Arc::new(kalamdb_commons::models::TableId::new(
                namespace_id.clone(),
                table_name.clone(),
            ));

            // Get unified_cache for provider
            let unified_cache = self.unified_cache.as_ref()
                .ok_or_else(|| KalamDbError::InvalidOperation("Unified cache not initialized".to_string()))?
                .clone();

            // Create provider
            // Try to reuse a cached provider; otherwise create and cache it
            let mut_provider = if let Some(existing) = unified_cache.get_provider(&*table_id) {
                // Downcast not needed for registration; we only reuse for registration
                existing
            } else {
                let created = Arc::new(StreamTableProvider::new(
                    table_id.clone(),
                    unified_cache.clone(),
                    schema,
                    store.clone(),
                    None,  // retention_seconds - TODO: get from table metadata
                    false, // ephemeral - TODO: get from table metadata
                    None,  // max_buffer - TODO: get from table metadata
                ));
                unified_cache.insert_provider(table_id.as_ref().clone(), created.clone());
                created
            };

            // Note: Skipping LiveQueryManager wiring for cached provider to keep cache purity
            let provider: Arc<dyn datafusion::datasource::TableProvider + Send + Sync> = mut_provider;

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

    /// Check write permissions for shared tables
    ///
    /// For SHARED tables with PUBLIC access level, only service/dba/system roles can write.
    /// Regular users can only READ public shared tables.
    fn check_write_permissions(
        &self,
        user_id: Option<&UserId>,
        table_refs: &Option<std::collections::HashSet<String>>,
    ) -> Result<(), KalamDbError> {
        // Get user role
        let ctx = self.create_execution_context(user_id)?;
        let user_role = ctx.user_role;

        // Admins and service accounts can write to everything
        if matches!(user_role, Role::System | Role::Dba | Role::Service) {
            return Ok(());
        }

        // For regular users, check each referenced table
        if let Some(table_refs) = table_refs {
            let kalam_sql = self
                .kalam_sql
                .as_ref()
                .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

            let all_tables = kalam_sql
                .scan_all_tables()
                .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

            for table_ref in table_refs {
                // Parse namespace.table or just table
                let parts: Vec<&str> = table_ref.split('.').collect();
                let (namespace, table_name) = if parts.len() == 2 {
                    (parts[0], parts[1])
                } else {
                    ("default", parts[0])
                };

                // Find the table
                if let Some(table) = all_tables.iter().find(|t| {
                    t.namespace.as_str() == namespace && t.table_name.as_str() == table_name
                }) {
                    // Only check SHARED tables
                    if table.table_type == TableType::Shared {
                        let access_level = table.access_level.unwrap_or(TableAccess::Private);
                        let is_owner = false; // TODO: Implement owner tracking

                        if !crate::auth::rbac::can_write_shared_table(
                            access_level,
                            is_owner,
                            user_role,
                        ) {
                            return Err(KalamDbError::Unauthorized(format!(
                                "User '{}' (role: {:?}) does not have write permission to shared table '{}.{}' (access_level: {:?})",
                                user_id.map(|u| u.as_str()).unwrap_or("anonymous"),
                                user_role,
                                namespace,
                                table_name,
                                access_level
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute query via DataFusion with selective table loading
    ///
    /// Loads only the tables referenced in the SQL query for better performance.
    /// If table extraction fails, falls back to loading all tables.
    async fn execute_datafusion_query_with_tables(
        &self,
        session: &SessionContext,
        sql: &str,
        _exec_ctx: &ExecutionContext,
        _table_refs: Option<std::collections::HashSet<String>>,
    ) -> Result<ExecutionResult, KalamDbError> {
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
    /// Execute CREATE NAMESPACE
    async fn execute_create_namespace(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
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
    async fn execute_show_namespaces(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
        let _stmt = ShowNamespacesStatement::parse(sql)?;
        let namespaces = self.namespace_service.list()?;

        // Convert to RecordBatch
        let batch = Self::namespaces_to_record_batch(namespaces)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute SHOW TABLES
    async fn execute_show_tables(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
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
                .filter(|t| &t.namespace == namespace_id)
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
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::ShowStoragesStatement;

        let _stmt = ShowStoragesStatement::parse(sql).map_err(KalamDbError::InvalidSql)?;

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
            if a.storage_id.as_str() == "local" {
                std::cmp::Ordering::Less
            } else if b.storage_id.as_str() == "local" {
                std::cmp::Ordering::Greater
            } else {
                a.storage_id.as_str().cmp(b.storage_id.as_str())
            }
        });

        // Convert to RecordBatch
        let mask_credentials = !exec_ctx.is_admin();
        let batch = Self::storages_to_record_batch(sorted_storages, mask_credentials)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute CREATE STORAGE
    async fn execute_create_storage(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::CreateStorageStatement;

        let stmt = CreateStorageStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("CREATE STORAGE parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Check if storage already exists
        let storage_id = StorageId::from(stmt.storage_id.as_str());
        if (kalam_sql
            .get_storage(&storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to check storage: {}", e)))?).is_some()
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
            storage_id: StorageId::from(stmt.storage_id.clone()),
            storage_name: stmt.storage_name,
            description: stmt.description,
            storage_type: stmt.storage_type.to_string(),
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
    async fn execute_alter_storage(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::AlterStorageStatement;

        let stmt = AlterStorageStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("ALTER STORAGE parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Get existing storage
        let storage_id = StorageId::from(stmt.storage_id.as_str());
        let mut storage = kalam_sql
            .get_storage(&storage_id)
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
    async fn execute_drop_storage(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::DropStorageStatement;

        let stmt = DropStorageStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("DROP STORAGE parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Check if storage exists
        let storage_id = StorageId::from(stmt.storage_id.as_str());
        match kalam_sql.get_storage(&storage_id) {
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
                    .map(|id| id.as_str() == stmt.storage_id.as_str())
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
            .delete_storage(&storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to delete storage: {}", e)))?;

        Ok(ExecutionResult::Success(format!(
            "Storage '{}' deleted successfully",
            stmt.storage_id
        )))
    }

    /// Execute FLUSH TABLE
    ///
    /// Triggers asynchronous flush for a single table, returning job_id immediately.
    /// The flush operation runs in the background via JobsManager.
    async fn execute_flush_table(
        &self,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::FlushTableStatement;

        if !matches!(exec_ctx.user_role, Role::System | Role::Dba | Role::Service) {
            return Err(KalamDbError::Unauthorized(
                "Only service, dba, or system users can flush tables".to_string(),
            ));
        }

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
                    stmt.namespace.as_str(),
                    stmt.table_name.as_str()
                ))
            })?;

        // Only user tables can be flushed (shared/stream tables not supported yet)
        if table.table_type != TableType::User {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot flush {} table '{}.{}'. Only user tables can be flushed.",
                table.table_type,
                stmt.namespace.as_str(),
                stmt.table_name.as_str()
            )));
        }

        // Get required dependencies
        let job_manager = self
            .job_manager
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("JobsManager not initialized".to_string()))?;

        let jobs_provider = self.jobs_table_provider.clone();

        // T21: Check if a flush job is already running for this table
        let table_full_name = format!("{}.{}", stmt.namespace, stmt.table_name);
        if let Some(ref provider) = jobs_provider {
            let all_jobs = provider.list_jobs()?;
            for job in all_jobs {
                if job.status == JobStatus::Running
                    && job.job_type == JobType::Flush
                    && job
                        .table_name
                        .as_ref()
                        .map(|tn| format!("{}.{}", job.namespace_id.as_str(), tn.as_str()))
                        == Some(table_full_name.clone())
                {
                    return Err(KalamDbError::InvalidOperation(format!(
                        "Flush job already running for table '{}' (job_id: {}). Please wait for it to complete.",
                        table_full_name, job.job_id
                    )));
                }
            }
        }

        // Phase 2b: Get schema from information_schema.tables (TableDefinition.schema_history)
        let namespace_id = NamespaceId::from(stmt.namespace.as_ref());
        let table_name = TableName::from(stmt.table_name.as_str());
        let table_def = kalam_sql
            .get_table_definition(&namespace_id, &table_name)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table definition not found for '{}.{}'",
                    stmt.namespace.as_str(),
                    stmt.table_name.as_str()
                ))
            })?;

        // Get the latest schema from schema_history
        if table_def.schema_history.is_empty() {
            return Err(KalamDbError::Other(format!(
                "No schema history found for table '{}.{}'",
                stmt.namespace.as_str(),
                stmt.table_name.as_str()
            )));
        }

        // Use the latest schema version (last in array)
        let latest_schema_version = &table_def.schema_history[table_def.schema_history.len() - 1];

        // Convert to Arrow schema
        let arrow_schema = crate::schema_registry::arrow_schema::ArrowSchemaWithOptions::from_json_string(
            &latest_schema_version.arrow_schema_json,
        )?;

        // Generate job_id for tracking
        let job_id = format!(
            "flush-{}-{}-{}",
            stmt.table_name,
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );
        let job_id_clone = job_id.clone();

        // Create and persist job record to system.jobs
        use kalamdb_commons::{JobId, JobType};
        let job_record = kalamdb_commons::system::Job::new(
            JobId::new(job_id.clone()),
            JobType::Flush,
            stmt.namespace.clone(),
                    //TODO: Use the nodeId from global config or context
              NodeId::from(format!("node-{}", std::process::id())),
        )
        .with_table_name(kalamdb_commons::TableName::new(format!(
            "{}.{}",
            stmt.namespace.as_str(),
            stmt.table_name.as_str()
        )));

        // Persist job to system.jobs
        if let Some(ref jobs_table_provider) = self.jobs_table_provider {
            jobs_table_provider.insert_job(job_record)?;
        }

        // Create flush job
        let namespace_id = stmt.namespace.clone();
        let table_name = stmt.table_name.clone();

        let storage_backend = self
            .storage_backend
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("Storage backend not initialized".to_string()))?;

        let table_store = Arc::new(new_user_table_store(
            storage_backend.clone(),
            &stmt.namespace,
            &stmt.table_name,
        ));

        // Phase 10: Use unified SchemaCache for dynamic path resolution
        let unified_cache = self.unified_cache.as_ref()
            .ok_or_else(|| KalamDbError::Other(
                "Unified cache not initialized - call with_storage_registry()".to_string()
            ))?
            .clone();

        // Phase 10: Create Arc<TableId> once for zero-allocation cache lookups
        let table_id = Arc::new(kalamdb_commons::models::TableId::new(
            namespace_id.clone(),
            table_name.clone(),
        ));

        let flush_job = crate::flush::UserTableFlushJob::new(
            table_id,
            table_store,
            namespace_id,
            table_name.clone(),
            arrow_schema.schema,
            unified_cache, // Phase 10 - unified cache instead of table_cache
        );

        // Clone necessary data for the async task
        let namespace_str = stmt.namespace.clone();
        let table_name_str = stmt.table_name.clone();
        let storage_id_str = table.storage_id.as_ref().map(|s| s.as_str()).unwrap_or("local").to_string();
        let jobs_provider_clone = jobs_provider.clone();

        // Spawn async flush task via JobsManager
        let job_future = Box::pin(async move {
            log::info!(
                "Executing flush job: job_id={}, table={}.{}",
                job_id_clone,
                namespace_str,
                table_name_str
            );

            let result = flush_job.execute_tracked();

            // Update job status in database based on result
            if let Some(ref provider) = jobs_provider_clone {
                let job_id_obj = kalamdb_commons::JobId::new(job_id_clone.clone());
                let mut job_record = match provider.get_job(&job_id_obj) {
                    Ok(Some(job)) => job,
                    Ok(None) => {
                        log::error!(
                            "Job {} not found in database during completion",
                            job_id_clone
                        );
                        return Err("Job record not found".to_string());
                    }
                    Err(e) => {
                        log::error!("Failed to get job {} from database: {}", job_id_clone, e);
                        return Err(format!("Failed to get job record: {}", e));
                    }
                };

                match &result {
                    Ok(flush_result) => {
                        let users_count = flush_result.metadata.users_count().unwrap_or(0);
                        job_record = job_record.complete(Some(format!(
                            "Flushed {} rows for {} users. Storage ID: {}. Parquet files: {}",
                            flush_result.rows_flushed,
                            users_count,
                            storage_id_str,
                            flush_result.parquet_files.len()
                        )));
                        log::info!(
                            "Flush job completed successfully: job_id={}, rows_flushed={}, users_count={}, parquet_files={}",
                            job_id_clone,
                            flush_result.rows_flushed,
                            users_count,
                            flush_result.parquet_files.len()
                        );
                    }
                    Err(e) => {
                        job_record = job_record.fail(e.to_string());
                        log::error!("Flush job failed: job_id={}, error={}", job_id_clone, e);
                    }
                }

                // Persist the updated job status
                if let Err(e) = provider.update_job(job_record) {
                    log::error!(
                        "Failed to update job {} status in database: {}",
                        job_id_clone,
                        e
                    );
                    return Err(format!("Failed to update job status: {}", e));
                }
            }

            match result {
                Ok(flush_result) => {
                    let users_count = flush_result.metadata.users_count().unwrap_or(0);
                    Ok(format!(
                        "Flushed {} rows for {} users. Storage ID: {}. Parquet files: {}",
                        flush_result.rows_flushed,
                        users_count,
                        storage_id_str,
                        flush_result.parquet_files.len()
                    ))
                }
                Err(e) => Err(format!("Flush failed: {}", e)),
            }
        });

        job_manager
            .start_job(job_id.clone(), "flush".to_string(), job_future)
            .await?;

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
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::FlushAllTablesStatement;

        if !matches!(exec_ctx.user_role, Role::System | Role::Dba | Role::Service) {
            return Err(KalamDbError::Unauthorized(
                "Only service, dba, or system users can flush tables".to_string(),
            ));
        }

        let stmt = FlushAllTablesStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("FLUSH ALL TABLES parse error: {}", e))
        })?;

        let kalam_sql = self
            .kalam_sql
            .as_ref()
            .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

        // Verify namespace exists
        let namespace_id = NamespaceId::from(stmt.namespace.as_str());
        match kalam_sql.get_namespace(&namespace_id) {
            Ok(Some(_)) => { /* namespace exists */ }
            Ok(None) => {
                return Err(KalamDbError::NotFound(format!(
                    "Namespace '{}' does not exist",
                    stmt.namespace.as_str()
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
            .filter(|t| t.namespace == stmt.namespace && t.table_type == TableType::User)
            .collect();

        if user_tables.is_empty() {
            return Ok(ExecutionResult::Success(format!(
                "No user tables found in namespace '{}' to flush",
                stmt.namespace.as_str()
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
            use kalamdb_commons::{JobId, JobType, TableName};
            let job_record = kalamdb_commons::system::Job::new(
                JobId::new(job_id.clone()),
                JobType::Flush,
                table.namespace.clone(),
                        //TODO: Use the nodeId from global config or context
                 NodeId::from(format!("node-{}", std::process::id())),
            )
            .with_table_name(TableName::new(format!(
                "{}.{}",
                table.namespace.as_str(),
                table.table_name.as_str()
            )));

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

        // TODO: Spawn async flush tasks via JobsManager (T250)

        Ok(ExecutionResult::Success(format!(
            "Flush started for {} table(s) in namespace '{}'. Job IDs: [{}]",
            job_ids.len(),
            stmt.namespace,
            job_ids.join(", ")
        )))
    }

    /// Execute DESCRIBE TABLE
    async fn execute_describe_table(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
        let stmt = DescribeTableStatement::parse(sql)?;

        // T314: Use unified cache for schema information
        let cache = self.unified_cache.as_ref()
            .ok_or_else(|| KalamDbError::Other("Unified cache not initialized".to_string()))?;

        // Construct TableId for cache lookup
        let namespace_str = stmt
            .namespace_id
            .as_ref()
            .map(|ns| ns.as_str())
            .unwrap_or("default");
        let namespace_id = crate::catalog::NamespaceId::new(namespace_str);
        let table_name = crate::catalog::TableName::new(stmt.table_name.as_str());
        let table_id = TableId::new(namespace_id, table_name);

        // Get table definition from cache (must exist - schema_store only used for populating cache)
        let cached_data = cache.get(&table_id)
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table '{}' not found in cache",
                    stmt.table_name.as_str()
                ))
            })?;

        if stmt.show_history {
            // Show schema version history
            let batch = Self::schema_history_to_record_batch(&cached_data.schema)?;
            Ok(ExecutionResult::RecordBatch(batch))
        } else {
            // Show column definitions (standard DESCRIBE TABLE output)
            let batch = Self::columns_to_record_batch(&cached_data.schema.columns)?;
            Ok(ExecutionResult::RecordBatch(batch))
        }
    }

    /// Execute SHOW STATS FOR TABLE
    async fn execute_show_table_stats(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
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
                let name_matches = t.table_name.as_str() == stmt.table_name.as_str();
                let namespace_matches = stmt
                    .namespace_id
                    .as_ref()
                    .map(|ns| t.namespace.as_str() == ns.as_str())
                    .unwrap_or(true);
                name_matches && namespace_matches
            })
            .ok_or_else(|| {
                KalamDbError::NotFound(format!("Table '{}' not found", stmt.table_name.as_str()))
            })?;

        // For now, return basic statistics
        // In Phase 16, this would query actual row counts from hot/cold storage
        let batch = self.table_stats_to_record_batch(&table)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute ALTER NAMESPACE
    async fn execute_alter_namespace(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
        let stmt = AlterNamespaceStatement::parse(sql)?;
        self.namespace_service
            .update_options(stmt.name.clone(), stmt.options)?;

        let message = format!("Namespace '{}' updated successfully", stmt.name.as_str());

        Ok(ExecutionResult::Success(message))
    }

    /// Execute DROP NAMESPACE
    async fn execute_drop_namespace(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
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

    /// Execute CREATE USER
    async fn execute_create_user(
        &self,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
        metadata: Option<&ExecutionMetadata>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Parse the CREATE USER statement
        let stmt =
            CreateUserStatement::parse(sql).map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        // Authorization check: Only DBA and System roles can create users
        if !exec_ctx.is_admin() {
            return Err(KalamDbError::PermissionDenied(
                "Only DBA or System users can create users".to_string(),
            ));
        }

        // Hash password if provided (for Password auth type)
        let password_hash = if stmt.auth_type == kalamdb_commons::AuthType::Password {
            if let Some(password) = &stmt.password {
                // T129: Validate password before hashing (FR-AUTH-002, FR-AUTH-003, FR-AUTH-019-022)
                // This ensures:
                // - Minimum 8 characters
                // - Maximum 72 characters (bcrypt limit)
                // - Not in common passwords list
                let policy = self.password_policy();
                password::validate_password_with_policy(password, &policy).map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Password validation failed: {}", e))
                })?;

                // Use bcrypt with cost factor 12 (good balance of security and performance)
                bcrypt::hash(password, 12)
                    .map_err(|e| KalamDbError::Other(format!("Failed to hash password: {}", e)))?
            } else {
                return Err(KalamDbError::InvalidSql(
                    "Password required for PASSWORD auth type".to_string(),
                ));
            }
        } else {
            String::new() // Empty for OAuth/Internal auth
        };

        // Prepare auth_data for OAuth (T139 - Phase 10, User Story 8)
        // For OAuth users, auth_data must contain provider and subject
        // TODO: Add a common model in kalamdb-commons for OAuth credentials and use it here and in kalamdb-auth for better type safety
        // AuthType::OAuth should have this model as associated data
        // Format: {"provider": "google", "subject": "oauth_user_id", "email": "user@example.com"}
        let auth_data = if stmt.auth_type == kalamdb_commons::AuthType::OAuth {
            // For OAuth, we expect the password field to contain JSON with provider and subject
            // Example SQL: CREATE USER bob WITH OAUTH '{"provider": "google", "subject": "12345"}'
            if let Some(oauth_json) = &stmt.password {
                // Parse OAuth JSON from password field
                let oauth_data: serde_json::Value = serde_json::from_str(oauth_json).map_err(
                    |e| {
                        KalamDbError::InvalidSql(format!(
                            "Invalid OAuth JSON format. Expected {{\"provider\": \"...\", \"subject\": \"...\"}}: {}",
                            e
                        ))
                    },
                )?;

                // Validate required fields
                if oauth_data.get("provider").is_none() || oauth_data.get("subject").is_none() {
                    return Err(KalamDbError::InvalidSql(
                        "OAuth auth_data must include 'provider' and 'subject' fields".to_string(),
                    ));
                }

                // Add email if provided
                let mut auth_json = oauth_data;
                if let Some(email) = &stmt.email {
                    auth_json["email"] = serde_json::json!(email);
                }

                Some(auth_json.to_string())
            } else {
                return Err(KalamDbError::InvalidSql(
                    "OAuth credentials required for OAUTH auth type. Format: WITH OAUTH '{\"provider\": \"...\", \"subject\": \"...\"}'".to_string(),
                ));
            }
        } else {
            None
        };

        // Create user entity
        let now = chrono::Utc::now().timestamp_millis();
        let user = kalamdb_commons::system::User {
            id: UserId::new(&stmt.username),
            username: UserName::new(&stmt.username),
            password_hash,
            role: stmt.role,
            email: stmt.email.clone(),
            auth_type: stmt.auth_type,
            auth_data,
            storage_mode: kalamdb_commons::StorageMode::Table, // Default
            storage_id: None,
            created_at: now,
            updated_at: now,
            last_seen: None,
            deleted_at: None,
        };

        // Save user via users table provider
        let users_provider = self.users_table_provider.as_ref().ok_or_else(|| {
            KalamDbError::InvalidOperation("User management not configured".to_string())
        })?;

        users_provider.create_user(user.clone())?;

        // Back-compat: also write to legacy KalamSQL store so auth/tests using
        // the adapter can find the user while we complete full migration.
        if let Some(kalam_sql) = &self.kalam_sql {
            let _ = kalam_sql.insert_user(&user);
        }

        self.log_audit_event(
            exec_ctx,
            "user.create",
            &stmt.username,
            json!({
                "role": format!("{:?}", stmt.role),
                "auth_type": format!("{:?}", stmt.auth_type),
                "email": stmt.email,
            }),
            metadata,
        );

        Ok(ExecutionResult::Success(format!(
            "User '{}' created successfully with role '{}'",
            stmt.username,
            stmt.role.as_str()
        )))
    }

    /// Execute ALTER USER
    async fn execute_alter_user(
        &self,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
        metadata: Option<&ExecutionMetadata>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Parse the ALTER USER statement
        let stmt =
            AlterUserStatement::parse(sql).map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        // Authorization check: Only DBA and System roles can alter users
        if !exec_ctx.is_admin() {
            return Err(KalamDbError::PermissionDenied(
                "Only DBA or System users can alter users".to_string(),
            ));
        }

        // Get users provider
        let users_provider = self.users_table_provider.as_ref().ok_or_else(|| {
            KalamDbError::InvalidOperation("User management not configured".to_string())
        })?;

        // Get existing user
        let target_user_id = UserId::new(&stmt.username);
        let mut user = users_provider
            .get_user_by_id(&target_user_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!("User not found: {}", stmt.username)))?;

        // Apply modifications
        use kalamdb_sql::ddl::UserModification;
        let mut change_details = serde_json::Map::new();
        match stmt.modification {
            UserModification::SetPassword(new_password) => {
                // T129: Validate password before hashing
                let policy = self.password_policy();
                password::validate_password_with_policy(&new_password, &policy).map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Password validation failed: {}", e))
                })?;

                // Hash the new password
                let password_hash = bcrypt::hash(&new_password, 12)
                    .map_err(|e| KalamDbError::Other(format!("Failed to hash password: {}", e)))?;
                user.password_hash = password_hash;
                user.auth_type = kalamdb_commons::AuthType::Password;
                change_details.insert("password_changed".to_string(), json!(true));
            }
            UserModification::SetRole(new_role) => {
                // new_role is already a Role enum from the parser
                user.role = new_role;
                change_details.insert("role".to_string(), json!(format!("{:?}", new_role)));
            }
            UserModification::SetEmail(new_email) => {
                user.email = Some(new_email);
                change_details.insert(
                    "email".to_string(),
                    json!(user.email.clone().unwrap_or_default()),
                );
            }
        }

        user.updated_at = chrono::Utc::now().timestamp_millis();

        // Update user
        users_provider.update_user(user)?;

        self.log_audit_event(
            exec_ctx,
            "user.alter",
            &stmt.username,
            json!({ "changes": change_details }),
            metadata,
        );

        Ok(ExecutionResult::Success(format!(
            "User '{}' updated successfully",
            stmt.username
        )))
    }

    /// Execute DROP USER
    async fn execute_drop_user(
        &self,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
        metadata: Option<&ExecutionMetadata>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Parse the DROP USER statement
        let stmt =
            DropUserStatement::parse(sql).map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        // Authorization check: Only DBA and System roles can drop users
        if !exec_ctx.is_admin() {
            return Err(KalamDbError::PermissionDenied(
                "Only DBA or System users can drop users".to_string(),
            ));
        }

        // Get users provider
        let users_provider = self.users_table_provider.as_ref().ok_or_else(|| {
            KalamDbError::InvalidOperation("User management not configured".to_string())
        })?;

        // Soft delete the user
        let target_user_id = UserId::new(&stmt.username);
        match users_provider.delete_user(&target_user_id) {
            Ok(()) => { /* deleted */ }
            Err(KalamDbError::NotFound(_)) if stmt.if_exists => {
                // IF EXISTS: treat missing user as success (skipped)
                return Ok(ExecutionResult::Success(format!(
                    "User '{}' does not exist (skipped)",
                    stmt.username
                )));
            }
            Err(e) => return Err(e),
        }

        self.log_audit_event(
            exec_ctx,
            "user.drop",
            &stmt.username,
            json!({ "soft_deleted": true }),
            metadata,
        );

        Ok(ExecutionResult::Success(format!(
            "User '{}' dropped successfully",
            stmt.username
        )))
    }

    /// Execute KILL JOB command
    async fn execute_kill_job(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
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
            .cancel_job_str(&job_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to cancel job: {}", e)))?;

        Ok(ExecutionResult::Success(format!(
            "Job '{}' cancelled successfully",
            job_id
        )))
    }

    async fn execute_kill_live_query(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
        let manager = self.live_query_manager.as_ref().ok_or_else(|| {
            KalamDbError::InvalidOperation("Live query manager not configured".to_string())
        })?;

        let stmt = KillLiveQueryStatement::parse(sql)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        manager
            .unregister_subscription(&stmt.live_id)
            .await
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to unregister live query {}: {}",
                    stmt.live_id,
                    e
                ))
            })?;

        Ok(ExecutionResult::Success(format!(
            "Live query {} terminated",
            stmt.live_id
        )))
    }

    /// Execute SUBSCRIBE TO command
    ///
    /// Returns metadata instructing the client to establish a WebSocket connection.
    /// This command does NOT execute a subscription directly - it returns information
    /// needed for the client to connect via WebSocket.
    async fn execute_subscribe(&self, _session: &SessionContext, sql: &str, _exec_ctx: &ExecutionContext) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::ddl::SubscribeStatement;

        // Parse the SUBSCRIBE TO command
        let stmt = SubscribeStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidSql(format!("Failed to parse SUBSCRIBE TO: {}", e))
        })?;

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
        response.insert(
            "status".to_string(),
            serde_json::json!("subscription_required"),
        );
        response.insert("ws_url".to_string(), serde_json::json!(ws_url));
        response.insert("subscription".to_string(), serde_json::json!(subscription));
        response.insert(
            "message".to_string(),
            serde_json::json!(format!(
                "Connect to WebSocket at {} and send the subscription object",
                ws_url
            )),
        );

        Ok(ExecutionResult::Subscription(serde_json::Value::Object(
            response,
        )))
    }

    /// Execute CREATE TABLE - determines table type from LOCATION and routes to appropriate service
    async fn execute_create_table(
        &self,
        session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Determine table type based on SQL keywords or LOCATION pattern
        let sql_upper = sql.to_uppercase();
        let namespace_id = crate::catalog::NamespaceId::new("default"); // TODO: Get from context
        let default_user_id = exec_ctx.user_id.clone();

        // Check for TABLE_TYPE clause to determine table type
        let has_table_type_user = sql_upper.contains("TABLE_TYPE")
            && (sql_upper.contains("TABLE_TYPE USER"));
        let has_table_type_shared = sql_upper.contains("TABLE_TYPE")
            && (sql_upper.contains("TABLE_TYPE SHARED"));
        let has_table_type_stream = sql_upper.contains("TABLE_TYPE")
            && (sql_upper.contains("TABLE_TYPE STREAM"));

        if sql_upper.contains("USER TABLE")
            || sql_upper.contains("${USER_ID}")
            || has_table_type_user
        {
            // RBAC: Only roles permitted by policy can create USER tables
            if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::User) {
                return Err(KalamDbError::Unauthorized(
                    "Insufficient privileges to create USER tables".to_string(),
                ));
            }

            // NOTE: USER tables are multi-tenant tables that store data for ALL users.
            // The user_id parameter here is ONLY for authentication/authorization (RBAC).
            // It does NOT determine table ownership - the table is accessible to all authenticated users.
            // Data isolation is handled at query time by automatically filtering rows by user_id.

            let namespace_id_for_parse =
                kalamdb_commons::models::NamespaceId::new(namespace_id.as_str());

            // Use unified parser for consistent parsing across all table types
            let stmt = CreateTableStatement::parse(sql, &namespace_id_for_parse)
                .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

            // Verify this is actually a USER table
            if stmt.table_type != kalamdb_commons::schemas::TableType::User {
                return Err(KalamDbError::InvalidSql(
                    "Expected CREATE USER TABLE statement".to_string(),
                ));
            }

            // stmt fields are already the right types from kalamdb_commons
            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone();
            self.ensure_namespace_exists(&namespace_id)?;
            let schema = stmt.schema.clone();
            let flush_policy = stmt.flush_policy.clone();
            let deleted_retention_hours = stmt.deleted_retention_hours;
            // Extract storage fields before stmt is moved
            let stmt_storage_id = stmt.storage_id.clone();
            let stmt_use_user_storage = stmt.use_user_storage;

            // Validate and resolve storage_id
            let storage_id = self.validate_storage_id(stmt_storage_id)?;

            let _metadata = self.user_table_service.create_table(stmt)?;

            // Insert into system.tables via KalamSQL
            if let Some(kalam_sql) = &self.kalam_sql {
                let table = kalamdb_sql::Table {
                    table_id: TableId::from_strings(namespace_id.as_str(), table_name.as_str()),
                    table_name: table_name.clone(),
                    namespace: namespace_id.clone(),
                    table_type: TableType::User,
                    created_at: chrono::Utc::now().timestamp_millis(),
                    storage_id: Some(storage_id.clone()),
                    use_user_storage: stmt_use_user_storage,
                    flush_policy: serde_json::to_string(&flush_policy.clone().unwrap_or_default())
                        .unwrap_or_else(|_| "{}".to_string()),
                    schema_version: 1,
                    deleted_retention_hours: deleted_retention_hours.unwrap_or(0) as i32,
                    access_level: None, // USER tables don't use access_level
                };
                kalam_sql.insert_table(&table).map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to insert table into system catalog: {}",
                        e
                    ))
                })?;
            }

            // Register with DataFusion if stores are configured
            // NOTE: For USER tables, this registration is skipped (returns Ok immediately)
            // because user tables are registered dynamically at query time per user.
            // The user_id parameter is ignored for USER table type.
            if self.user_table_store.is_some() {
                let dummy_user_id = crate::catalog::UserId::from("system");
                self.register_table_with_datafusion(
                    session,
                    &namespace_id,
                    &table_name,
                    crate::catalog::TableType::User,
                    schema.clone(),
                    dummy_user_id,
                )
                .await?;
            }

            // T311: Cache table metadata in unified SchemaCache
            self.cache_table_metadata(
                &namespace_id,
                &table_name,
                TableType::User,
                &storage_id,
                flush_policy,
                schema,
                1, // initial schema_version
                deleted_retention_hours,
            )?;

            Ok(ExecutionResult::Success(
                "User table created successfully".to_string(),
            ))
        } else if sql_upper.contains("STREAM TABLE")
            || sql_upper.contains("TTL")
            || sql_upper.contains("BUFFER_SIZE")
            || has_table_type_stream
        {
            // RBAC: Check permission for STREAM tables
            if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::Stream) {
                return Err(KalamDbError::Unauthorized(
                    "Insufficient privileges to create STREAM tables".to_string(),
                ));
            }
            // Stream table - use unified parser
            let stmt = CreateTableStatement::parse(sql, &namespace_id)
                .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

            // Verify this is actually a STREAM table
            if stmt.table_type != kalamdb_commons::schemas::TableType::Stream {
                return Err(KalamDbError::InvalidSql(
                    "Expected CREATE STREAM TABLE statement".to_string(),
                ));
            }

            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone(); // Use the namespace from the statement
            self.ensure_namespace_exists(&namespace_id)?;
            let schema = stmt.schema.clone();
            let retention_seconds = stmt.ttl_seconds.map(|t| t as u32);
            let flush_policy = stmt.flush_policy.clone().map(crate::flush::FlushPolicy::from).unwrap_or_default();

            self.stream_table_service.create_table(stmt)?;

            // TODO: In the future create the table and directly add it to the cache and read the Table from the cache itself
            // Insert into system.tables via KalamSQL
            if let Some(kalam_sql) = &self.kalam_sql {
                let table = kalamdb_sql::Table {
                    table_id: TableId::from_strings(namespace_id.as_str(), table_name.as_str()),
                    table_name: table_name.clone(),
                    namespace: namespace_id.clone(),
                    table_type: TableType::Stream,
                    created_at: chrono::Utc::now().timestamp_millis(),
                    storage_id: Some(StorageId::from("local")), // Stream tables always use local storage
                    use_user_storage: false, // Stream tables don't support user storage
                    flush_policy: serde_json::to_string(&flush_policy)
                        .unwrap_or_else(|_| "{}".to_string()),
                    schema_version: 1,
                    deleted_retention_hours: retention_seconds
                        .map(|s| (s / 3600) as i32)
                        .unwrap_or(0),
                    access_level: None, // STREAM tables don't use access_level
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
                    session,
                    &namespace_id,
                    &table_name,
                    crate::catalog::TableType::Stream,
                    schema.clone(),
                    default_user_id,
                )
                .await?;
            }

            // T311: Cache table metadata in unified SchemaCache
            let storage_id = StorageId::from("local"); // Stream tables always use local storage
            // Convert core FlushPolicy to DDL FlushPolicy for caching
            let flush_policy_ddl = match &flush_policy {
                crate::flush::FlushPolicy::RowLimit { row_limit } => {
                    Some(kalamdb_sql::ddl::FlushPolicy::RowLimit {
                        row_limit: *row_limit,
                    })
                }
                crate::flush::FlushPolicy::TimeInterval { interval_seconds } => {
                    Some(kalamdb_sql::ddl::FlushPolicy::TimeInterval {
                        interval_seconds: *interval_seconds,
                    })
                }
                crate::flush::FlushPolicy::Combined {
                    row_limit,
                    interval_seconds,
                } => Some(kalamdb_sql::ddl::FlushPolicy::Combined {
                    row_limit: *row_limit,
                    interval_seconds: *interval_seconds,
                }),
            };
            self.cache_table_metadata(
                &namespace_id,
                &table_name,
                TableType::Stream,
                &storage_id,
                flush_policy_ddl,
                schema,
                1, // initial schema_version
                retention_seconds,
            )?;

            Ok(ExecutionResult::Success(
                "Stream table created successfully".to_string(),
            ))
        } else if has_table_type_shared {
            // RBAC: Check permission for SHARED tables
            if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::Shared) {
                return Err(KalamDbError::Unauthorized(
                    "Insufficient privileges to create SHARED tables".to_string(),
                ));
            }
            // Shared table specified via TABLE_TYPE
            // Use unified parser for consistent parsing
            let stmt = CreateTableStatement::parse(sql, &namespace_id)
                .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

            // Verify this is actually a SHARED table
            if stmt.table_type != kalamdb_commons::schemas::TableType::Shared {
                return Err(KalamDbError::InvalidSql(
                    "Expected CREATE SHARED TABLE statement".to_string(),
                ));
            }

            let table_name = stmt.table_name.clone();
            let namespace_id = stmt.namespace_id.clone(); // Use the namespace from the statement
            self.ensure_namespace_exists(&namespace_id)?;
            let schema = stmt.schema.clone();
            let flush_policy = stmt.flush_policy.clone();
            let deleted_retention = stmt.deleted_retention_hours.map(|h| h as u64 * 3600);

            // Extract storage_id and access_level before stmt is moved
            let stmt_storage_id = stmt.storage_id.clone();
            let stmt_access_level = stmt.access_level;

            // Validate and resolve storage_id
            let storage_id = self.validate_storage_id(stmt_storage_id)?;

            let was_created = self.shared_table_service.create_table(stmt)?;
            log::debug!(
                "CREATE SHARED TABLE: was_created={}, table_name={}",
                was_created,
                table_name
            );

            // Only insert into system.tables and register with DataFusion if table was newly created
            if was_created {
                log::debug!(
                    "Inserting table into system.tables: {}.{}",
                    namespace_id,
                    table_name
                );
                // Insert into system.tables via KalamSQL
                if let Some(kalam_sql) = &self.kalam_sql {
                    // Serialize flush policy for storage
                    let flush_policy_json =
                        serde_json::to_string(&flush_policy.clone().unwrap_or_default())
                            .unwrap_or_else(|_| "{}".to_string());

                    let table = kalamdb_sql::Table {
                        table_id: TableId::from_strings(
                            namespace_id.as_str(),
                            table_name.as_str(),
                        ),
                        table_name: table_name.clone(),
                        namespace: namespace_id.clone(),
                        table_type: TableType::Shared,
                        created_at: chrono::Utc::now().timestamp_millis(),
                        storage_id: Some(storage_id.clone()),
                        use_user_storage: false, // Shared tables don't support user storage
                        flush_policy: flush_policy_json,
                        schema_version: 1,
                        deleted_retention_hours: deleted_retention
                            .map(|s| (s / 3600) as i32)
                            .unwrap_or(0),
                        access_level: stmt_access_level, // Already TableAccess enum
                    };
                    log::debug!("Calling insert_table for: {:?}", table);
                    kalam_sql.insert_table(&table).map_err(|e| {
                        log::error!("Failed to insert table into system catalog: {}", e);
                        KalamDbError::Other(format!(
                            "Failed to insert table into system catalog: {}",
                            e
                        ))
                    })?;
                    log::debug!("Successfully inserted table into system.tables");
                } else {
                    log::warn!("kalam_sql is None, cannot insert table into system.tables");
                }

                // Register with DataFusion if stores are configured
                if self.shared_table_store.is_some() {
                    self.register_table_with_datafusion(
                        session,
                        &namespace_id,
                        &table_name,
                        crate::catalog::TableType::Shared,
                        schema.clone(),
                        default_user_id,
                    )
                    .await?;
                }

                // T311: Cache table metadata in unified SchemaCache
                self.cache_table_metadata(
                    &namespace_id,
                    &table_name,
                    TableType::Shared,
                    &storage_id,
                    flush_policy,
                    schema,
                    1, // initial schema_version
                    deleted_retention.map(|s| (s / 3600) as u32),
                )?;

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
            self.ensure_namespace_exists(&namespace_id)?;
            let schema = stmt.schema.clone();
            let flush_policy = stmt.flush_policy.clone();
            let deleted_retention = stmt.deleted_retention_hours.map(|h| h as u64 * 3600);

            // Extract storage_id and access_level before stmt is moved
            let stmt_storage_id = stmt.storage_id.clone();
            let stmt_access_level = stmt.access_level;

            // Validate and resolve storage_id
            let storage_id = self.validate_storage_id(stmt_storage_id)?;

            let was_created = self.shared_table_service.create_table(stmt)?;

            if was_created {
                if let Some(kalam_sql) = &self.kalam_sql {
                    // Serialize flush policy for storage
                    let flush_policy_json =
                        serde_json::to_string(&flush_policy.clone().unwrap_or_default())
                            .unwrap_or_else(|_| "{}".to_string());

                    let table = kalamdb_sql::Table {
                        table_id: TableId::from_strings(
                            namespace_id.as_str(),
                            table_name.as_str(),
                        ),
                        table_name: table_name.clone(),
                        namespace: namespace_id.clone(),
                        table_type: TableType::Shared,
                        created_at: chrono::Utc::now().timestamp_millis(),
                        storage_id: Some(storage_id.clone()),
                        use_user_storage: false,
                        flush_policy: flush_policy_json,
                        schema_version: 1,
                        deleted_retention_hours: deleted_retention
                            .map(|s| (s / 3600) as i32)
                            .unwrap_or(0),
                        access_level: stmt_access_level, // Already TableAccess enum
                    };
                    kalam_sql.insert_table(&table).map_err(|e| {
                        KalamDbError::Other(format!(
                            "Failed to insert table into system catalog: {}",
                            e
                        ))
                    })?;
                } else {
                    log::warn!("kalam_sql is None for default SHARED table, cannot insert into system.tables");
                }

                if self.shared_table_store.is_some() {
                    self.register_table_with_datafusion(
                        session,
                        &namespace_id,
                        &table_name,
                        crate::catalog::TableType::Shared,
                        schema.clone(),
                        default_user_id,
                    )
                    .await?;
                }

                // T311: Cache table metadata in unified SchemaCache
                self.cache_table_metadata(
                    &namespace_id,
                    &table_name,
                    TableType::Shared,
                    &storage_id,
                    flush_policy,
                    schema,
                    1, // initial schema_version
                    deleted_retention.map(|s| (s / 3600) as u32),
                )?;

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

    /// Execute ALTER TABLE
    async fn execute_alter_table(
        &self,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
        metadata: Option<&ExecutionMetadata>,
    ) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::ddl::{AlterTableStatement, ColumnOperation};

        // Parse ALTER TABLE statement (use default namespace for now)
        let default_namespace = kalamdb_commons::NamespaceId::new("default");
        let stmt = AlterTableStatement::parse(sql, &default_namespace)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        // Handle SET ACCESS LEVEL operation
        if let ColumnOperation::SetAccessLevel { access_level } = &stmt.operation {
            // Only Service/Dba/System can modify access levels
            if !matches!(exec_ctx.user_role, Role::Service | Role::Dba | Role::System) {
                return Err(KalamDbError::Unauthorized(
                    "Only service, dba, or system users can modify table access levels".to_string(),
                ));
            }

            // Get the table to verify it exists and is a SHARED table
            let kalam_sql = self
                .kalam_sql
                .as_ref()
                .ok_or_else(|| KalamDbError::Other("KalamSql not initialized".to_string()))?;

            let table_id =
                TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
            let table_id_str = table_id.to_string();
            let mut table = kalam_sql
                .get_table(&table_id_str)
                .map_err(|e| KalamDbError::Other(format!("Failed to get table: {}", e)))?
                .ok_or_else(|| {
                    KalamDbError::table_not_found(format!(
                        "Table '{}' not found",
                        table_id
                    ))
                })?;

            // Verify table is SHARED type
            if table.table_type != TableType::Shared {
                return Err(KalamDbError::InvalidOperation(
                    "ACCESS LEVEL can only be set on SHARED tables".to_string(),
                ));
            }

            // Update the table's access_level (already TableAccess enum)
            table.access_level = Some(*access_level);

            // Persist the change
            kalam_sql
                .update_table(&table)
                .map_err(|e| KalamDbError::Other(format!("Failed to update table: {}", e)))?;

            // T312: Invalidate unified cache after ALTER TABLE
            if let Some(cache) = &self.unified_cache {
                cache.invalidate(&table_id);
            }

            self.log_audit_event(
                exec_ctx,
                "table.set_access",
                &format!(
                    "{}.{}",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                ),
                json!({ "access_level": format!("{:?}", access_level) }),
                metadata,
            );

            return Ok(ExecutionResult::Success(format!(
                "Table '{}' access level changed to '{:?}'",
                table_id, access_level
            )));
        }

        // For other ALTER TABLE operations (ADD COLUMN, DROP COLUMN, etc.),
        // return error for now (not yet implemented)
        Err(KalamDbError::InvalidOperation(
            "ALTER TABLE operations other than SET ACCESS LEVEL are not yet supported via this path. \
             Use SchemaEvolutionService directly for column modifications.".to_string(),
        ))
    }

    /// Execute DROP TABLE
    async fn execute_drop_table(
        &self,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
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

        let requested_table_type: TableType = stmt.table_type.into();
        let table_identifier =
            TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());

        let actual_table_type = if let Some(kalam_sql) = &self.kalam_sql {
            match kalam_sql.get_table(table_identifier.as_ref()) {
                Ok(Some(table)) => table.table_type,
                Ok(None) => requested_table_type,
                Err(_) => requested_table_type,
            }
        } else {
            requested_table_type
        };

        // TODO: Track table ownership in system tables to determine is_owner accurately (#US3 follow-up)
        let is_owner = false;
        if !crate::auth::rbac::can_delete_table(exec_ctx.user_role, actual_table_type, is_owner) {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop this table".to_string(),
            ));
        }

        if let Some(manager) = &self.live_query_manager {
            let table_ref = format!(
                "{}.{}",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            );
            if manager.has_active_subscriptions_for(&table_ref).await {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Cannot drop table {} while active live queries exist",
                    table_ref
                )));
            }
        }

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

        // T313: Invalidate unified cache after DROP TABLE
        if let Some(cache) = &self.unified_cache {
            cache.invalidate(&table_identifier);
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
        session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Parse UPDATE statement (basic parser for integration tests)
        let update_info = self.parse_update_statement(sql)?;

        // Check write permissions for shared tables
        let table_name = format!("{}.{}", update_info.namespace.as_str(), update_info.table.as_str());
        let mut table_refs = std::collections::HashSet::new();
        table_refs.insert(table_name.clone());
        self.check_write_permissions(Some(&exec_ctx.user_id), &Some(table_refs))?;

        // Special-case: limited UPDATE support for system.users (restore user via deleted_at = NULL)
        if update_info.namespace.as_str().eq_ignore_ascii_case("system")
            && update_info.table.as_str().eq_ignore_ascii_case("users")
        {
            // Only admins can update system tables
            if !exec_ctx.is_admin() {
                return Err(KalamDbError::Unauthorized(
                    "Only DBA or System users can update system tables".to_string(),
                ));
            }

            // Expect WHERE username = '...'
            let (where_col, where_val) = self.parse_simple_where(&update_info.where_clause)?;
            if !where_col.eq_ignore_ascii_case("username") {
                return Err(KalamDbError::InvalidSql(
                    "Only WHERE username = '...' is supported for system.users".to_string(),
                ));
            }

            // Only support SET deleted_at = NULL (restore user)
            if update_info.set_values.len() != 1
                || !update_info.set_values[0].0.eq_ignore_ascii_case("deleted_at")
                || !update_info.set_values[0].1.is_null()
            {
                return Err(KalamDbError::InvalidOperation(
                    "Only UPDATE system.users SET deleted_at = NULL WHERE username = '...' is supported"
                        .to_string(),
                ));
            }

            // Perform restore
            let users_provider = self.users_table_provider.as_ref().ok_or_else(|| {
                KalamDbError::InvalidOperation("User management not configured".to_string())
            })?;

            let mut user = users_provider
                .get_user_by_username(&where_val)?
                .ok_or_else(|| KalamDbError::NotFound(format!("User not found: {}", where_val)))?;

            user.deleted_at = None;
            user.updated_at = chrono::Utc::now().timestamp_millis();
            users_provider.update_user(user)?;

            return Ok(ExecutionResult::Success("Updated 1 user(s)".to_string()));
        }

        // Get table provider from the appropriate session
        let table_ref = datafusion::sql::TableReference::parse_str(&format!(
            "{}.{}",
            update_info.namespace.as_str(), update_info.table.as_str()
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
            // Scan only this user's rows via the provider (ensures correct partition and filtering)
            let all_rows = user_provider
                .scan_current_user_rows()
                .map_err(|e| KalamDbError::Other(format!("Scan failed: {}", e)))?;

            // Filter rows by WHERE clause (simple evaluation: col = 'value')
            let (where_col, where_val) = self.parse_simple_where(&update_info.where_clause)?;

            let mut updated_count = 0;
            for (_row_key, row_data) in all_rows {
                // Check if row matches WHERE clause
                if let Some(obj) = row_data.fields.as_object() {
                    if let Some(col_value) = obj.get(&where_col) {
                        if Self::value_matches(col_value, &where_val) {
                            // Build update JSON from SET clause
                            let mut updates = serde_json::Map::new();
                            for (col, val) in &update_info.set_values {
                                updates.insert(col.clone(), val.clone());
                            }

                            // Call update method using the row's ID
                            user_provider
                                .update_row(&row_data.row_id, serde_json::Value::Object(updates))
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

        // Execute the update on shared table
        let store = self
            .shared_table_store
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("Store not configured".to_string()))?;
        let all_rows =
            SharedTableStoreExt::scan(store.as_ref(), update_info.namespace.as_str(), update_info.table.as_str())
                .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // Filter rows by WHERE clause (simple evaluation: col = 'value')
        let (where_col, where_val) = self.parse_simple_where(&update_info.where_clause)?;

        let mut updated_count = 0;
        for (row_id, row_data) in all_rows {
            // Check if row matches WHERE clause
            if let Some(obj) = row_data.fields.as_object() {
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
        session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Parse DELETE statement
        let delete_info = self.parse_delete_statement(sql)?;

        // Check write permissions for shared tables
        let table_name = format!("{}.{}", delete_info.namespace.as_str(), delete_info.table.as_str());
        let mut table_refs = std::collections::HashSet::new();
        table_refs.insert(table_name.clone());
        self.check_write_permissions(Some(&exec_ctx.user_id), &Some(table_refs))?;

        // Get table provider from the appropriate session
        let table_ref = datafusion::sql::TableReference::parse_str(&format!(
            "{}.{}",
            delete_info.namespace.as_str(), delete_info.table.as_str()
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
            let all_rows = user_provider
                .scan_current_user_rows()
                .map_err(|e| KalamDbError::Other(format!("Scan failed: {}", e)))?;

            // Filter rows by WHERE clause - support both '=' and 'IN'
            let mut deleted_count = 0;

            // Try parsing as IN clause first
            if delete_info.where_clause.to_uppercase().contains(" IN ") {
                let (where_col, where_vals) = self.parse_where_in(&delete_info.where_clause)?;

                for (_row_key, row_data) in all_rows {
                    // Skip rows already soft-deleted
                    if row_data._deleted {
                        continue;
                    }
                    // Check if row matches WHERE IN clause
                    if let Some(obj) = row_data.fields.as_object() {
                        if let Some(col_value) = obj.get(&where_col) {
                            let val_str = match col_value {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Number(n) => n.to_string(),
                                _ => continue,
                            };

                            if where_vals.contains(&val_str) {
                                // Call delete method (soft delete for user tables)
                                user_provider.delete_row(&row_data.row_id).map_err(|e| {
                                    KalamDbError::Other(format!("Delete failed: {}", e))
                                })?;

                                deleted_count += 1;
                            }
                        }
                    }
                }
            } else {
                // Simple equality WHERE clause
                let (where_col, where_val) = self.parse_simple_where(&delete_info.where_clause)?;

                for (_row_key, row_data) in all_rows {
                    // Skip rows already soft-deleted
                    if row_data._deleted {
                        continue;
                    }
                    // Check if row matches WHERE clause
                    if let Some(obj) = row_data.fields.as_object() {
                        if let Some(col_value) = obj.get(&where_col) {
                            if Self::value_matches(col_value, &where_val) {
                                // Call delete method (soft delete for user tables)
                                user_provider.delete_row(&row_data.row_id).map_err(|e| {
                                    KalamDbError::Other(format!("Delete failed: {}", e))
                                })?;

                                deleted_count += 1;
                            }
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
            .scan(delete_info.namespace.as_str(), delete_info.table.as_str())
            .map_err(|e| KalamDbError::Other(format!("Scan failed: {}", e)))?;

        // Filter rows by WHERE clause - support both '=' and 'IN'
        let mut deleted_count = 0;

        // Try parsing as IN clause first
        if delete_info.where_clause.to_uppercase().contains(" IN ") {
            let (where_col, where_vals) = self.parse_where_in(&delete_info.where_clause)?;

            for (row_id, row_data) in all_rows {
                // Skip rows already soft-deleted
                if row_data._deleted {
                    continue;
                }
                // Check if row matches WHERE IN clause
                if let Some(obj) = row_data.fields.as_object() {
                    if let Some(col_value) = obj.get(&where_col) {
                        let val_str = match col_value {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            _ => continue,
                        };

                        if where_vals.contains(&val_str) {
                            // Call soft delete method
                            shared_provider.delete_soft(&row_id).map_err(|e| {
                                KalamDbError::Other(format!("Delete failed: {}", e))
                            })?;

                            deleted_count += 1;
                        }
                    }
                }
            }
        } else {
            // Simple equality WHERE clause
            let (where_col, where_val) = self.parse_simple_where(&delete_info.where_clause)?;

            for (row_id, row_data) in all_rows {
                // Skip rows already soft-deleted
                if row_data._deleted {
                    continue;
                }
                // Check if row matches WHERE clause
                if let Some(obj) = row_data.fields.as_object() {
                    if let Some(col_value) = obj.get(&where_col) {
                        if Self::value_matches(col_value, &where_val) {
                            // Call soft delete method
                            shared_provider.delete_soft(&row_id).map_err(|e| {
                                KalamDbError::Other(format!("Delete failed: {}", e))
                            })?;

                            deleted_count += 1;
                        }
                    }
                }
            }
        }

        Ok(ExecutionResult::Success(format!(
            "Deleted {} row(s)",
            deleted_count
        )))
    }

    /// Parse simple WHERE clause: col = 'value' or col IN ('val1', 'val2', ...)
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

    /// Parse WHERE clause with IN support: col IN ('val1', 'val2', ...)
    /// Returns (column_name, Vec<values>)
    fn parse_where_in(&self, where_clause: &str) -> Result<(String, Vec<String>), KalamDbError> {
        // Pattern: col IN ('val1', 'val2', ...)
        let re = regex::Regex::new(r"(?i)(\w+)\s+IN\s*\((.+)\)").unwrap();

        if let Some(captures) = re.captures(where_clause.trim()) {
            let col_name = captures[1].to_string();
            let values_str = &captures[2];

            // Parse comma-separated quoted values
            let values: Vec<String> = values_str
                .split(',')
                .map(|v| v.trim().trim_matches('\'').trim_matches('"').to_string())
                .collect();

            return Ok((col_name, values));
        }

        Err(KalamDbError::InvalidSql(format!(
            "Invalid IN clause syntax: {}",
            where_clause
        )))
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
            let raw_val = parts[1].trim();
            // Treat NULL as JSON null; otherwise strip quotes and keep as string literal
            let value = if raw_val.eq_ignore_ascii_case("NULL") {
                serde_json::Value::Null
            } else {
                let col_value = raw_val.trim_matches('\'').trim_matches('"').to_string();
                serde_json::json!(col_value)
            };
            set_values.push((col_name, value));
        }

        Ok(UpdateInfo {
            namespace: NamespaceId::new(namespace),
            table: TableName::new(table),
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
            namespace: NamespaceId::new(captures[1].to_string()),
            table: TableName::new(captures[2].to_string()),
            where_clause: captures[3].to_string(),
        })
    }

    /// Convert namespaces list to Arrow RecordBatch
    fn namespaces_to_record_batch(namespaces: Vec<Namespace>) -> Result<RecordBatch, KalamDbError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("table_count", DataType::Int32, false),
            Field::new("created_at", DataType::Utf8, false),
        ]));

        let names: ArrayRef = Arc::new(StringArray::from(
            namespaces
                .iter()
                .map(|ns| ns.name.as_str())
                .collect::<Vec<_>>(),
        ));

        let table_counts: ArrayRef = Arc::new(datafusion::arrow::array::Int32Array::from(
            namespaces
                .iter()
                .map(|ns| ns.table_count)
                .collect::<Vec<_>>(),
        ));

        let created_at: ArrayRef = Arc::new(StringArray::from(
            namespaces
                .iter()
                .map(|ns| {
                    chrono::DateTime::from_timestamp_millis(ns.created_at)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| "Invalid timestamp".to_string())
                })
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
                let id_str = id.as_str().to_lowercase();
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
        let is_stream_table = table.table_type == TableType::Stream;

        let schema_history_hint =
            format!("system.table_schemas WHERE table_id = '{}'", table.table_id);

        let (properties, values): (Vec<&str>, Vec<String>) = if is_stream_table {
            // Stream table properties (NO system columns, show stream-specific config)
            (
                vec![
                    "table_id",
                    "table_name",
                    "namespace",
                    "table_type",
                    "current_schema_version",
                    "created_at",
                    "schema_history_reference",
                    "note",
                ],
                vec![
                    table.table_id.to_string(),
                    table.table_name.to_string(),
                    table.namespace.to_string(),
                    format!("{:?}", table.table_type),
                    schema_version_str,
                    created_at_str,
                    schema_history_hint,
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
                    "current_schema_version",
                    "deleted_retention_hours",
                    "created_at",
                    "schema_history_reference",
                ],
                vec![
                    table.table_id.to_string(),
                    table.table_name.to_string(),
                    table.namespace.to_string(),
                    format!("{:?}", table.table_type),
                    table.storage_id.as_ref().map(|s| s.to_string()).unwrap_or_else(|| "local".to_string()),
                    table.flush_policy.to_string(),
                    schema_version_str,
                    retention_hours_str,
                    created_at_str,
                    schema_history_hint,
                ],
            )
        };

        let property_array: ArrayRef = Arc::new(StringArray::from(properties));
        let value_array: ArrayRef = Arc::new(StringArray::from(values));

        RecordBatch::try_new(schema, vec![property_array, value_array])
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    /// Phase 15 (008-schema-consolidation): Convert column definitions to RecordBatch for DESCRIBE TABLE
    fn columns_to_record_batch(
        columns: &[kalamdb_commons::schemas::ColumnDefinition],
    ) -> Result<RecordBatch, KalamDbError> {
        use datafusion::arrow::array::{BooleanArray, Int32Array, StringArray};

        let schema = Arc::new(Schema::new(vec![
            Field::new("column_name", DataType::Utf8, false),
            Field::new("ordinal_position", DataType::Int32, false),
            Field::new("data_type", DataType::Utf8, false),
            Field::new("is_nullable", DataType::Boolean, false),
            Field::new("is_primary_key", DataType::Boolean, false),
            Field::new("is_partition_key", DataType::Boolean, false),
            Field::new("default_value", DataType::Utf8, true),
            Field::new("column_comment", DataType::Utf8, true),
        ]));

        let column_names: ArrayRef = Arc::new(StringArray::from(
            columns
                .iter()
                .map(|c| c.column_name.as_str())
                .collect::<Vec<_>>(),
        ));

        let ordinal_positions: ArrayRef = Arc::new(Int32Array::from(
            columns
                .iter()
                .map(|c| c.ordinal_position as i32)
                .collect::<Vec<_>>(),
        ));

        let data_types: ArrayRef = Arc::new(StringArray::from(
            columns
                .iter()
                .map(|c| format!("{:?}", c.data_type))
                .collect::<Vec<_>>(),
        ));

        let is_nullable: ArrayRef = Arc::new(BooleanArray::from(
            columns.iter().map(|c| c.is_nullable).collect::<Vec<_>>(),
        ));

        let is_primary_key: ArrayRef = Arc::new(BooleanArray::from(
            columns.iter().map(|c| c.is_primary_key).collect::<Vec<_>>(),
        ));

        let is_partition_key: ArrayRef = Arc::new(BooleanArray::from(
            columns
                .iter()
                .map(|c| c.is_partition_key)
                .collect::<Vec<_>>(),
        ));

        let default_values: ArrayRef = Arc::new(StringArray::from(
            columns
                .iter()
                .map(|c| {
                    if c.default_value.is_none() {
                        None
                    } else {
                        Some(format!("{:?}", c.default_value))
                    }
                })
                .collect::<Vec<_>>(),
        ));

        let column_comments: ArrayRef = Arc::new(StringArray::from(
            columns
                .iter()
                .map(|c| c.column_comment.as_deref())
                .collect::<Vec<_>>(),
        ));

        RecordBatch::try_new(
            schema,
            vec![
                column_names,
                ordinal_positions,
                data_types,
                is_nullable,
                is_primary_key,
                is_partition_key,
                default_values,
                column_comments,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    /// Phase 15 (008-schema-consolidation): Convert schema history to RecordBatch for DESCRIBE TABLE HISTORY
    fn schema_history_to_record_batch(
        table_def: &kalamdb_commons::schemas::TableDefinition,
    ) -> Result<RecordBatch, KalamDbError> {
        use datafusion::arrow::array::{Int32Array, Int64Array, StringArray};

        let schema = Arc::new(Schema::new(vec![
            Field::new("version", DataType::Int32, false),
            Field::new("created_at", DataType::Int64, false),
            Field::new("changes", DataType::Utf8, true),
            Field::new("column_count", DataType::Int32, false),
        ]));

        let versions: ArrayRef = Arc::new(Int32Array::from(
            table_def
                .schema_history
                .iter()
                .map(|v| v.version as i32)
                .collect::<Vec<_>>(),
        ));

        let created_ats: ArrayRef = Arc::new(Int64Array::from(
            table_def
                .schema_history
                .iter()
                .map(|v| v.created_at.timestamp_millis())
                .collect::<Vec<_>>(),
        ));

        let changes: ArrayRef = Arc::new(StringArray::from(
            table_def
                .schema_history
                .iter()
                .map(|v| v.changes.as_str())
                .collect::<Vec<_>>(),
        ));

        // Count columns for each version by parsing arrow_schema_json
        let column_counts: ArrayRef = Arc::new(Int32Array::from(
            table_def
                .schema_history
                .iter()
                .map(|v| {
                    // Try to parse arrow_schema_json to count fields
                    match serde_json::from_str::<serde_json::Value>(&v.arrow_schema_json) {
                        Ok(val) => val
                            .get("fields")
                            .and_then(|f| f.as_array())
                            .map(|arr| arr.len() as i32)
                            .unwrap_or(table_def.columns.len() as i32),
                        Err(_) => table_def.columns.len() as i32,
                    }
                })
                .collect::<Vec<_>>(),
        ));

        RecordBatch::try_new(schema, vec![versions, created_ats, changes, column_counts])
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    /// Convert table statistics to RecordBatch for SHOW STATS
    fn table_stats_to_record_batch(
        &self,
        table: &kalamdb_sql::Table,
    ) -> Result<RecordBatch, KalamDbError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("statistic", DataType::Utf8, false),
            Field::new("value", DataType::Utf8, false),
        ]));

        let buffered_rows = self.count_buffered_rows(table)?;

        let full_table_name = format!("{}.{}", table.namespace, table.table_name);
        let (last_flush_rows, last_flush_ts, storage_bytes) =
            self.fetch_flush_metrics(&full_table_name)?;

        let created_at_str = chrono::DateTime::from_timestamp_millis(table.created_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "unknown".to_string());

        let metrics = vec![
            ("table_name", table.table_name.as_str().to_string()),
            ("namespace", table.namespace.as_str().to_string()),
            ("table_type", format!("{:?}", table.table_type)),
            ("schema_version", table.schema_version.to_string()),
            ("storage_id", table.storage_id.as_ref().map(|s| s.to_string()).unwrap_or_else(|| "local".to_string())),
            ("created_at", created_at_str),
            ("buffered_rows", buffered_rows.to_string()),
            ("last_flush_rows", last_flush_rows.to_string()),
            ("storage_bytes", storage_bytes.to_string()),
            ("last_flush_timestamp", last_flush_ts),
        ];

        let (stat_names, stat_values): (Vec<&str>, Vec<String>) = metrics
            .into_iter()
            .unzip();

        let statistic_array: ArrayRef = Arc::new(StringArray::from(stat_names));
        let value_array: ArrayRef = Arc::new(StringArray::from(stat_values));

        RecordBatch::try_new(schema, vec![statistic_array, value_array])
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    fn count_buffered_rows(&self, table: &kalamdb_sql::Table) -> Result<usize, KalamDbError> {
        match table.table_type {
            TableType::User => {
                let store = self.user_table_store.as_ref().ok_or_else(|| {
                    KalamDbError::InvalidOperation(
                        "User table store not configured for SHOW TABLE STATS".to_string(),
                    )
                })?;
                let rows = UserTableStoreExt::scan_all(
                    store.as_ref(),
                    table.namespace.as_str(),
                    table.table_name.as_str(),
                )
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to scan user table {}.{}: {}",
                        table.namespace.as_str(),
                        table.table_name.as_str(),
                        e
                    ))
                })?;
                Ok(rows.len())
            }
            TableType::Shared => {
                let store = self.shared_table_store.as_ref().ok_or_else(|| {
                    KalamDbError::InvalidOperation(
                        "Shared table store not configured for SHOW TABLE STATS".to_string(),
                    )
                })?;
                let rows = store
                    .scan(table.namespace.as_str(), table.table_name.as_str())
                    .map_err(|e| {
                        KalamDbError::Other(format!(
                            "Failed to scan shared table {}.{}: {}",
                            table.namespace.as_str(),
                            table.table_name.as_str(),
                            e
                        ))
                    })?;
                Ok(rows.len())
            }
            TableType::Stream => {
                let store = self.stream_table_store.as_ref().ok_or_else(|| {
                    KalamDbError::InvalidOperation(
                        "Stream table store not configured for SHOW TABLE STATS".to_string(),
                    )
                })?;
                let rows = store
                    .scan(table.namespace.as_str(), table.table_name.as_str())
                    .map_err(|e| {
                        KalamDbError::Other(format!(
                            "Failed to scan stream table {}.{}: {}",
                            table.namespace.as_str(),
                            table.table_name.as_str(),
                            e
                        ))
                    })?;
                Ok(rows.len())
            }
            TableType::System => Ok(0),
        }
    }

    fn fetch_flush_metrics(
        &self,
        full_table_name: &str,
    ) -> Result<(usize, String, u64), KalamDbError> {
        let provider = match &self.jobs_table_provider {
            Some(provider) => provider,
            None => return Ok((0, "never".to_string(), 0)),
        };

        let jobs = provider.list_jobs().map_err(|e| {
            KalamDbError::Other(format!("Failed to list jobs for SHOW TABLE STATS: {}", e))
        })?;

        let mut latest: Option<kalamdb_commons::system::Job> = None;
        for job in jobs {
            use kalamdb_commons::JobType;
            if job.job_type == JobType::Flush
                && job.table_name.as_ref().map(|tn| tn.as_str()) == Some(full_table_name)
                && job.completed_at.is_some()
            {
                let candidate_ts = job.completed_at.unwrap_or_default();
                let replace = latest
                    .as_ref()
                    .map(|existing| existing.completed_at.unwrap_or_default() < candidate_ts)
                    .unwrap_or(true);

                if replace {
                    latest = Some(job);
                }
            }
        }

        if let Some(job) = latest {
            let completed_at = job.completed_at.unwrap_or_default();
            let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(completed_at)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| completed_at.to_string());

            let mut rows_flushed = 0usize;
            let mut storage_bytes = 0u64;

            if let Some(result_str) = job.result.as_ref() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(result_str) {
                    if let Some(rows) = json.get("rows_flushed").and_then(|v| v.as_u64()) {
                        rows_flushed = rows as usize;
                    }

                    if let Some(files) = json.get("parquet_files").and_then(|v| v.as_array()) {
                        for file in files.iter().filter_map(|v| v.as_str()) {
                            if let Ok(meta) = std::fs::metadata(file) {
                                if meta.is_file() {
                                    storage_bytes = storage_bytes.saturating_add(meta.len());
                                }
                            }
                        }
                    }
                }
            }

            Ok((rows_flushed, timestamp, storage_bytes))
        } else {
            Ok((0, "never".to_string(), 0))
        }
    }

    /// Record an audit log entry for privileged actions.
    fn log_audit_event(
        &self,
        ctx: &ExecutionContext,
        action: &str,
        target: &str,
        details: serde_json::Value,
        metadata: Option<&ExecutionMetadata>,
    ) {
        let kalam_sql = match &self.kalam_sql {
            Some(k) => k,
            None => {
                log::debug!(
                    "Audit logging skipped (KalamSql not configured) for action '{}'",
                    action
                );
                return;
            }
        };

        let actor_username = match kalam_sql.get_user_by_id(&ctx.user_id) {
            Ok(Some(user)) => user.username.clone(),
            Ok(None) => UserName::new(ctx.user_id.as_str()),
            Err(err) => {
                log::warn!(
                    "Failed to resolve username for {} during audit logging: {}",
                    ctx.user_id,
                    err
                );
                UserName::new(ctx.user_id.as_str())
            }
        };

        let entry = AuditLogEntry {
            audit_id: AuditLogId::new(uuid::Uuid::new_v4().to_string()),
            timestamp: chrono::Utc::now().timestamp_millis(),
            actor_user_id: ctx.user_id.clone(),
            actor_username,
            action: action.to_string(),
            target: target.to_string(),
            details: if details.is_null() {
                None
            } else {
                Some(details.to_string())
            },
            ip_address: metadata
                .and_then(|m| m.ip_address.as_ref())
                .map(|s| s.to_string()),
        };

        if let Err(err) = kalam_sql.insert_audit_log(&entry) {
            log::warn!(
                "Failed to persist audit log entry for action '{}': {}",
                action,
                err
            );
        }
    }

    fn password_policy(&self) -> PasswordPolicy {
        PasswordPolicy::default().with_enforced_complexity(self.enforce_password_complexity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::prelude::SessionContext;
    use kalamdb_sql::KalamSql;
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::{RocksDBBackend, StorageBackend};

    fn setup_test_executor() -> SqlExecutor {
        let test_db =
            TestDb::new(&["system_namespaces", "system_tables", "system_table_schemas"]).unwrap();

        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend.clone()).unwrap());
        let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
        let session_context = Arc::new(SessionContext::new());

        // Initialize table services for tests (now zero-sized structs)
        let user_table_service = Arc::new(crate::services::UserTableService::new());
        let shared_table_service = Arc::new(crate::services::SharedTableService::new());
        let stream_table_service = Arc::new(crate::services::StreamTableService::new());

        SqlExecutor::new(
            namespace_service,
            user_table_service,
            shared_table_service,
            stream_table_service,
        )
        .with_storage_backend(backend)
    }

    fn create_test_session() -> SessionContext {
        SessionContext::new()
    }

    fn create_test_exec_ctx() -> ExecutionContext {
        ExecutionContext::new(
            UserId::from("test_user"),
            Role::Dba,  // Use Dba role for tests to bypass permission checks
        )
    }

    #[tokio::test]
    async fn test_execute_create_namespace() {
        let executor = setup_test_executor();
        let session = create_test_session();
        let exec_ctx = create_test_exec_ctx();

        let result = executor
            .execute(&session, "CREATE NAMESPACE app", &exec_ctx)
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
        let session = create_test_session();
        let exec_ctx = create_test_exec_ctx();

        // Create some namespaces
        executor
            .execute(&session, "CREATE NAMESPACE app1", &exec_ctx)
            .await
            .unwrap();
        executor
            .execute(&session, "CREATE NAMESPACE app2", &exec_ctx)
            .await
            .unwrap();

        let result = executor.execute(&session, "SHOW NAMESPACES", &exec_ctx).await.unwrap();

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
        let session = create_test_session();
        let exec_ctx = create_test_exec_ctx();

        executor
            .execute(&session, "CREATE NAMESPACE app", &exec_ctx)
            .await
            .unwrap();

        let result = executor
            .execute(&session, "ALTER NAMESPACE app SET OPTIONS (enabled = true)", &exec_ctx)
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
            TestDb::new(&["system_namespaces", "system_tables", "system_table_schemas", "information_schema_tables"]).unwrap();
        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend.clone()).unwrap());
        let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
        let session_context = Arc::new(SessionContext::new());

        // Initialize table services for tests (now zero-sized structs)
        let user_table_service = Arc::new(crate::services::UserTableService::new());
        let shared_table_service = Arc::new(crate::services::SharedTableService::new());
        let stream_table_service = Arc::new(crate::services::StreamTableService::new());

        let storage_registry = Arc::new(crate::storage::StorageRegistry::new(
            kalam_sql.clone(),
            "./data/storage".to_string(),
        ));

        let mut executor = SqlExecutor::new(
            namespace_service,
            user_table_service,
            shared_table_service,
            stream_table_service,
        )
        .with_storage_backend(backend.clone())
        .with_storage_registry(storage_registry.clone());

        // Create and populate unified cache for DESCRIBE TABLE
        let unified_cache = Arc::new(crate::catalog::SchemaCache::new(100, Some(storage_registry)));
        
        // Create a test stream table definition using TableDefinition
        use kalamdb_commons::schemas::{TableDefinition, TableOptions};
        use crate::catalog::CachedTableData;
        
        let table_def = TableDefinition::new(
            NamespaceId::new("app"),
            TableName::new("events"),
            kalamdb_commons::schemas::TableType::Stream,
            vec![], // Stream tables have no user-defined columns
            TableOptions::stream(3600), // Default 1 hour TTL
            None, // table_comment
        ).unwrap();

        kalam_sql.upsert_table_definition(&table_def).unwrap();

        // Populate cache with the table definition
        let table_id = TableId::new(NamespaceId::new("app"), TableName::new("events"));
        let cached_data = CachedTableData::new(
            table_id.clone(),
            kalamdb_commons::schemas::TableType::Stream,
            chrono::Utc::now(),
            Some(StorageId::new("local")),
            crate::flush::FlushPolicy::default(),
            "./data/storage/stream".to_string(), // storage_path_template
            1, // schema_version
            None, // deleted_retention_hours
            Arc::new(table_def),
        );
        unified_cache.insert(table_id, Arc::new(cached_data));

        // Set cache in executor
        executor.unified_cache = Some(unified_cache);

        let session = create_test_session();
        let exec_ctx = create_test_exec_ctx();

        // Execute DESCRIBE TABLE
        let result = executor
            .execute(&session, "DESCRIBE TABLE app.events", &exec_ctx)
            .await
            .unwrap();

        // Verify stream table shows no user-defined columns
        match result {
            ExecutionResult::RecordBatch(batch) => {
                // Should have 8 columns (standard DESCRIBE TABLE schema)
                assert_eq!(batch.num_columns(), 8);
                
                // Should have 0 rows (stream tables have no user-defined columns)
                assert_eq!(batch.num_rows(), 0, "Stream tables should have no user-defined columns");
            }
            _ => panic!("Expected RecordBatch result"),
        }
    }

    #[tokio::test]
    async fn test_execute_drop_namespace() {
        let executor = setup_test_executor();
        let session = create_test_session();
        let exec_ctx = create_test_exec_ctx();

        executor
            .execute(&session, "CREATE NAMESPACE app", &exec_ctx)
            .await
            .unwrap();

        let result = executor.execute(&session, "DROP NAMESPACE app", &exec_ctx).await.unwrap();

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
        let session = create_test_session();
        let exec_ctx = create_test_exec_ctx();

        executor
            .execute(&session, "CREATE NAMESPACE app", &exec_ctx)
            .await
            .unwrap();

        // Simulate adding a table
        executor
            .namespace_service
            .increment_table_count("app")
            .unwrap();

        let result = executor.execute(&session, "DROP NAMESPACE app", &exec_ctx).await;
        assert!(result.is_err());
    }
}
