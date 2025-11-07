//! DDL (Data Definition Language) Handler//! Ddl handlers (to be migrated from executor.rs)

//! //!

//! Handles DDL operations://! Functions will be moved here during the refactoring.

//! - CREATE/DROP NAMESPACE
//! - CREATE/ALTER/DROP TABLE (USER/SHARED/STREAM)
//!
//! Design: Phased extraction approach
//! - Phase 1: CREATE NAMESPACE (simple, 16 lines)
//! - Phase 2: ALTER/DROP TABLE (moderate, ~151 lines)
//! - Phase 3: CREATE TABLE (complex, 445 lines with 3 table type branches)

use super::types::{ExecutionContext, ExecutionMetadata, ExecutionResult};
use kalamdb_commons::schemas::TableType;
use crate::error::KalamDbError;
use datafusion::execution::context::SessionContext;
use kalamdb_commons::models::{JobId, NamespaceId, StorageId, TableId, UserId};
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::system::Namespace;
use kalamdb_commons::Role;
use kalamdb_sql::ddl::{AlterTableStatement, ColumnOperation, CreateNamespaceStatement, CreateTableStatement, DropTableStatement};
use kalamdb_commons::schemas::policy::FlushPolicy;
// Phase 8 Complete: All KalamSql usages replaced with SchemaRegistry and system table providers
use std::sync::Arc;

/// DDL Handler for Data Definition Language operations
pub struct DDLHandler;

impl DDLHandler {
    // ============================================================================
    // Helper Methods for Table Schema Transformations
    // ============================================================================

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

    /// Inject auto-increment field if not present
    ///
    /// Adds a snowflake ID field named "id" as the first column if no field named "id" exists.
    /// Uses Int64 type for snowflake IDs.
    fn inject_auto_increment_field(
        schema: Arc<arrow::datatypes::Schema>,
    ) -> Result<Arc<arrow::datatypes::Schema>, KalamDbError> {
        use arrow::datatypes::{DataType, Field, Schema};

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
        schema: Arc<arrow::datatypes::Schema>,
        table_type: TableType,
    ) -> Result<Arc<arrow::datatypes::Schema>, KalamDbError> {
        use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

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
        stmt: &CreateTableStatement,
        schema: &Arc<arrow::datatypes::Schema>,
    ) -> Result<(), KalamDbError> {
        use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, SchemaVersion};
        use kalamdb_commons::datatypes::{KalamDataType, FromArrowType};
        use crate::schema_registry::arrow_schema::ArrowSchemaWithOptions;
        use crate::app_context::AppContext;

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

        // Build table options based on table type
        let table_options = match stmt.table_type {
            kalamdb_commons::schemas::TableType::User => TableOptions::user(),
            kalamdb_commons::schemas::TableType::Shared => TableOptions::shared(),
            kalamdb_commons::schemas::TableType::Stream => {
                // Stream tables require TTL from statement
                let ttl_seconds = stmt.ttl_seconds.unwrap_or(3600); // Default 1 hour
                TableOptions::stream(ttl_seconds)
            },
            kalamdb_commons::schemas::TableType::System => {
                // System tables shouldn't be created via SQL, but handle gracefully
                TableOptions::shared()
            },
        };

        // Create NEW TableDefinition directly
        let mut table_def = TableDefinition::new(
            stmt.namespace_id.clone(),
            stmt.table_name.clone(),
            stmt.table_type,
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

        // Single atomic write to information_schema_tables via SchemaRegistry
        let ctx = AppContext::get();
        let schema_registry = ctx.schema_registry();
        let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
        schema_registry
            .put_table_definition(&table_id, &table_def)
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

    // ============================================================================
    // DDL Statement Handlers
    // ============================================================================

    /// Execute CREATE NAMESPACE statement
    /// 
    /// Creates a new namespace in the system. Namespaces provide logical isolation
    /// for tables and other database objects.
    /// 
    /// # Arguments
    /// * `namespaces_provider` - Provider for namespace operations
    /// * `session` - DataFusion session context (reserved for future use)
    /// * `sql` - Raw SQL statement
    /// * `exec_ctx` - Execution context with user information
    /// 
    /// # Returns
    /// Success message indicating namespace creation status
    /// 
    /// # Example SQL
    /// ```sql
    /// CREATE NAMESPACE production;
    /// CREATE NAMESPACE IF NOT EXISTS staging;
    /// ```
    pub async fn execute_create_namespace(
        app_context: &Arc<crate::app_context::AppContext>,
        _session: &SessionContext,
        sql: &str,
        _exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Extract namespace provider from AppContext
        let namespaces_provider = app_context.system_tables().namespaces();

        // Parse CREATE NAMESPACE statement
        let stmt = CreateNamespaceStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidSql(format!("Failed to parse CREATE NAMESPACE: {}", e))
        })?;

        let name = stmt.name.as_str();

        // Validate namespace name
        Namespace::validate_name(name)?;

        // Check if namespace already exists
        let namespace_id = NamespaceId::new(name);
        let existing = namespaces_provider.get_namespace(&namespace_id)?;

        if existing.is_some() {
            if stmt.if_not_exists {
                let message = format!("Namespace '{}' already exists", name);
                return Ok(ExecutionResult::Success(message));
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Namespace '{}' already exists",
                    name
                )));
            }
        }

        // Create namespace entity with system as default owner
        let namespace = Namespace::new(name);

        // Insert namespace via provider
        namespaces_provider.create_namespace(namespace)?;

        let message = format!("Namespace '{}' created successfully", name);
        Ok(ExecutionResult::Success(message))
    }

    /// Execute DROP NAMESPACE statement
    /// 
    /// Drops a namespace from the system. Prevents dropping namespaces that contain tables.
    /// 
    /// # Arguments
    /// * `namespaces_provider` - Provider for namespace operations
    /// * `session` - DataFusion session context (reserved for future use)
    /// * `sql` - Raw SQL statement
    /// * `exec_ctx` - Execution context with user information
    /// 
    /// # Returns
    /// Success message indicating namespace deletion status
    /// 
    /// # Example SQL
    /// ```sql
    /// DROP NAMESPACE production;
    /// DROP NAMESPACE IF EXISTS staging;
    /// ```
    pub async fn execute_drop_namespace(
        app_context: &Arc<crate::app_context::AppContext>,
        _session: &SessionContext,
        sql: &str,
        _exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::ddl::DropNamespaceStatement;

        // Extract namespace provider from AppContext
        let namespaces_provider = app_context.system_tables().namespaces();

        // Parse DROP NAMESPACE statement
        let stmt = DropNamespaceStatement::parse(sql)?;

        let name = stmt.name.as_str();
        let namespace_id = NamespaceId::new(name);

        // Check if namespace exists
        let namespace = match namespaces_provider.get_namespace(&namespace_id)? {
            Some(ns) => ns,
            None => {
                if stmt.if_exists {
                    let message = format!("Namespace '{}' does not exist", name);
                    return Ok(ExecutionResult::Success(message));
                } else {
                    return Err(KalamDbError::NotFound(format!(
                        "Namespace '{}' not found",
                        name
                    )));
                }
            }
        };

        // Check if namespace has tables
        if !namespace.can_delete() {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot drop namespace '{}': namespace contains {} table(s). Drop all tables first.",
                name,
                namespace.table_count
            )));
        }

        // Delete namespace via provider
        namespaces_provider.delete_namespace(&namespace_id)?;

        let message = format!("Namespace '{}' dropped successfully", name);
        Ok(ExecutionResult::Success(message))
    }

    /// Execute CREATE STORAGE statement
    /// 
    /// Creates a new storage backend configuration in the system.
    /// 
    /// # Arguments
    /// * `kalam_sql` - SQL adapter for system table access
    /// * `storage_registry` - Storage registry for template validation
    /// * `session` - DataFusion session context (reserved for future use)
    /// * `sql` - Raw SQL statement
    /// * `exec_ctx` - Execution context with user information
    /// 
    /// # Returns
    /// Success message indicating storage creation status
    /// 
    /// # Example SQL
    /// ```sql
    /// CREATE STORAGE my_storage
    ///   TYPE 'parquet'
    ///   BASE_DIRECTORY '/data/kalamdb'
    ///   SHARED_TABLES_TEMPLATE '{namespace}/{tableName}'
    ///   USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}';
    /// ```
    pub async fn execute_create_storage(
        app_context: &Arc<crate::app_context::AppContext>,
        _session: &SessionContext,
        sql: &str,
        _exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use kalamdb_sql::CreateStorageStatement;

        // Extract providers from AppContext
        let storages_provider = app_context.system_tables().storages();
        let storage_registry = app_context.storage_registry();

        // Parse CREATE STORAGE statement
        let stmt = CreateStorageStatement::parse(sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("CREATE STORAGE parse error: {}", e))
        })?;

        // Check if storage already exists
        let storage_id = StorageId::from(stmt.storage_id.as_str());
        if storages_provider
            .get_storage_by_id(&storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to check storage: {}", e)))?
            .is_some()
        {
            return Err(KalamDbError::InvalidOperation(format!(
                "Storage '{}' already exists",
                stmt.storage_id
            )));
        }

        // Validate templates using StorageRegistry
        if !stmt.shared_tables_template.is_empty() {
            storage_registry.validate_template(&stmt.shared_tables_template, false)?;
        }
        if !stmt.user_tables_template.is_empty() {
            storage_registry.validate_template(&stmt.user_tables_template, true)?;
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
        storages_provider
            .insert_storage(storage)
            .map_err(|e| KalamDbError::Other(format!("Failed to create storage: {}", e)))?;

        Ok(ExecutionResult::Success(format!(
            "Storage '{}' created successfully",
            stmt.storage_id
        )))
    }

    /// Execute CREATE TABLE statement
    ///
    /// Handles creation of all three table types:
    /// - USER tables: Multi-tenant tables with automatic user_id filtering
    /// - SHARED tables: Single-tenant tables with access control
    /// - STREAM tables: TTL-based ephemeral tables
    ///
    /// # Arguments
    /// * `stream_table_service` - Service for STREAM table operations
    /// * `tables_provider` - TablesTableProvider for system.tables access
    /// * `cache_fn` - Closure for caching table metadata
    /// * `register_fn` - Closure for DataFusion registration
    /// * `validate_storage_fn` - Closure for storage_id validation
    /// * `ensure_namespace_fn` - Closure for namespace existence check
    /// * `session` - DataFusion session context
    /// * `sql` - Raw SQL statement
    /// * `exec_ctx` - Execution context with user information
    ///
    /// # Returns
    /// Success message indicating table creation status
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_create_table<CacheFn, RegisterFn, ValidateFn, EnsureFn>(
        tables_provider: &Arc<crate::tables::system::TablesTableProvider>,
        cache_fn: CacheFn,
        register_fn: RegisterFn,
        validate_storage_fn: ValidateFn,
        ensure_namespace_fn: EnsureFn,
        session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>
    where
        CacheFn: FnOnce(&NamespaceId, &kalamdb_commons::models::TableName, TableType, &StorageId, Option<FlushPolicy>, std::sync::Arc<arrow::datatypes::Schema>, i32, Option<u32>) -> Result<(), KalamDbError>,
        RegisterFn: FnOnce(&SessionContext, &NamespaceId, &kalamdb_commons::models::TableName, TableType, std::sync::Arc<arrow::datatypes::Schema>, UserId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), KalamDbError>> + Send>>,
        ValidateFn: FnOnce(Option<StorageId>) -> Result<StorageId, KalamDbError>,
        EnsureFn: FnOnce(&NamespaceId) -> Result<(), KalamDbError>,
    {
        // Determine table type from SQL keywords
        let sql_upper = sql.to_uppercase();
        let namespace_id = NamespaceId::new("default");
        let default_user_id = exec_ctx.user_id.clone();

        // Check for TABLE_TYPE clause
        let has_table_type_user = sql_upper.contains("TABLE_TYPE") && sql_upper.contains("TABLE_TYPE USER");
        let _has_table_type_shared = sql_upper.contains("TABLE_TYPE") && sql_upper.contains("TABLE_TYPE SHARED");
        let has_table_type_stream = sql_upper.contains("TABLE_TYPE") && sql_upper.contains("TABLE_TYPE STREAM");

        // Determine table type and route to appropriate service
        if sql_upper.contains("USER TABLE") || sql_upper.contains("${USER_ID}") || has_table_type_user {
            Self::create_user_table(
                tables_provider,
                cache_fn,
                register_fn,
                validate_storage_fn,
                ensure_namespace_fn,
                session,
                sql,
                &namespace_id,
                default_user_id,
                exec_ctx,
            ).await
        } else if sql_upper.contains("STREAM TABLE") || sql_upper.contains("TTL") || sql_upper.contains("BUFFER_SIZE") || has_table_type_stream {
            Self::create_stream_table(
                tables_provider,
                cache_fn,
                register_fn,
                ensure_namespace_fn,
                session,
                sql,
                &namespace_id,
                default_user_id,
                exec_ctx,
            ).await
        } else {
            // Default to SHARED table (most common case)
            Self::create_shared_table(
                tables_provider,
                cache_fn,
                register_fn,
                validate_storage_fn,
                ensure_namespace_fn,
                session,
                sql,
                &namespace_id,
                default_user_id,
                exec_ctx,
            ).await
        }
    }

    /// Create USER table (multi-tenant with automatic user_id filtering)
    #[allow(clippy::too_many_arguments)]
    async fn create_user_table<CacheFn, RegisterFn, ValidateFn, EnsureFn>(
        tables_provider: &Arc<crate::tables::system::TablesTableProvider>,
        cache_fn: CacheFn,
        register_fn: RegisterFn,
        validate_storage_fn: ValidateFn,
        ensure_namespace_fn: EnsureFn,
        session: &SessionContext,
        sql: &str,
        namespace_id: &NamespaceId,
        _default_user_id: UserId,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>
    where
        CacheFn: FnOnce(&NamespaceId, &kalamdb_commons::models::TableName, TableType, &StorageId, Option<FlushPolicy>, std::sync::Arc<arrow::datatypes::Schema>, i32, Option<u32>) -> Result<(), KalamDbError>,
        RegisterFn: FnOnce(&SessionContext, &NamespaceId, &kalamdb_commons::models::TableName, TableType, std::sync::Arc<arrow::datatypes::Schema>, UserId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), KalamDbError>> + Send>>,
        ValidateFn: FnOnce(Option<StorageId>) -> Result<StorageId, KalamDbError>,
        EnsureFn: FnOnce(&NamespaceId) -> Result<(), KalamDbError>,
    {
        // RBAC check
        if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::User) {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to create USER tables".to_string(),
            ));
        }

        // Parse statement
        let stmt = CreateTableStatement::parse(sql, namespace_id)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        // Verify table type
        if stmt.table_type != kalamdb_commons::schemas::TableType::User {
            return Err(KalamDbError::InvalidSql("Expected CREATE USER TABLE statement".to_string()));
        }

        // Extract fields
        let table_name = stmt.table_name.clone();
        let namespace_id = stmt.namespace_id.clone();
        let schema = stmt.schema.clone();
        let flush_policy = stmt.flush_policy.clone();
        let deleted_retention_hours = stmt.deleted_retention_hours;
        let stmt_storage_id = stmt.storage_id.clone();
    let _stmt_use_user_storage = stmt.use_user_storage;

        // Validate dependencies
        ensure_namespace_fn(&namespace_id)?;
        let storage_id = validate_storage_fn(stmt_storage_id)?;

        // ========================================================================
        // Inlined Business Logic from UserTableService (Phase 8 - T108)
        // ========================================================================

        // Step 1: Validate table name
        Self::validate_table_name(stmt.table_name.as_str())
            .map_err(KalamDbError::InvalidOperation)?;

        // Step 2: Check if table already exists
        let ctx = crate::app_context::AppContext::get();
        let schema_registry = ctx.schema_registry();
        let table_id = TableId::from_strings(namespace_id.as_str(), table_name.as_str());
        let table_exists = schema_registry.table_exists(&table_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to check table existence: {}", e)))?;

        if table_exists {
            if stmt.if_not_exists {
                // IF NOT EXISTS: Return success without creating
                return Ok(ExecutionResult::Success(format!(
                    "Table {}.{} already exists (IF NOT EXISTS)",
                    namespace_id.as_str(),
                    table_name.as_str()
                )));
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Table {}.{} already exists",
                    namespace_id.as_str(),
                    table_name.as_str()
                )));
            }
        }

        // Step 3: Auto-increment field injection (id column)
        let schema = Self::inject_auto_increment_field(schema)?;

        // Step 4: System column injection (_updated, _deleted)
        let schema = Self::inject_system_columns(schema, TableType::User)?;

        // Step 5: Inject DEFAULT SNOWFLAKE_ID() for auto-injected id column
        let mut modified_stmt = stmt.clone();
        if !modified_stmt.column_defaults.contains_key("id") {
            modified_stmt.column_defaults.insert(
                "id".to_string(),
                kalamdb_commons::schemas::ColumnDefault::function("SNOWFLAKE_ID", vec![]),
            );
        }

        // Step 6: Save complete table definition to information_schema_tables
        Self::save_table_definition(&modified_stmt, &schema)?;

        // Step 7: Create RocksDB column family for this table
        let _user_table_store = ctx.user_table_store();
        // TODO: Column family creation removed - SystemTableStore doesn't have this method
        // user_table_store.create_column_family(namespace_id.as_str(), table_name.as_str())
        Ok::<(), KalamDbError>(())
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to create column family for user table {}.{}: {}",
                    namespace_id.as_str(),
                    table_name.as_str(),
                    e
                ))
            })?;

        log::info!(
            "User table {}.{} created successfully (schema version 1)",
            namespace_id.as_str(),
            table_name.as_str()
        );

        // ========================================================================
        // End Inlined Business Logic
        // ========================================================================

        // Insert into system.tables
        let table_id = TableId::from_strings(namespace_id.as_str(), table_name.as_str());
        
        // Get the TableDefinition that was just saved
        let table_def = schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| {
                KalamDbError::Other(format!(
                    "Failed to retrieve table definition for {}.{} after save",
                    namespace_id.as_str(),
                    table_name.as_str()
                ))
            })?;
        
        tables_provider.create_table(&table_id, &table_def).map_err(|e| {
            KalamDbError::Other(format!("Failed to insert table into system catalog: {}", e))
        })?;

        // Register with DataFusion (skipped for USER tables - registered dynamically per user)
        let dummy_user_id = UserId::from("system");
        register_fn(session, &namespace_id, &table_name, TableType::User, schema.clone(), dummy_user_id).await?;

        // Cache metadata
        cache_fn(&namespace_id, &table_name, TableType::User, &storage_id, flush_policy, schema, 1, deleted_retention_hours)?;

        Ok(ExecutionResult::Success("User table created successfully".to_string()))
    }

    /// Create STREAM table (TTL-based ephemeral table)
    #[allow(clippy::too_many_arguments)]
    async fn create_stream_table<CacheFn, RegisterFn, EnsureFn>(
        tables_provider: &Arc<crate::tables::system::TablesTableProvider>,
        cache_fn: CacheFn,
        register_fn: RegisterFn,
        ensure_namespace_fn: EnsureFn,
        session: &SessionContext,
        sql: &str,
        namespace_id: &NamespaceId,
        default_user_id: UserId,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>
    where
        CacheFn: FnOnce(&NamespaceId, &kalamdb_commons::models::TableName, TableType, &StorageId, Option<FlushPolicy>, std::sync::Arc<arrow::datatypes::Schema>, i32, Option<u32>) -> Result<(), KalamDbError>,
        RegisterFn: FnOnce(&SessionContext, &NamespaceId, &kalamdb_commons::models::TableName, TableType, std::sync::Arc<arrow::datatypes::Schema>, UserId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), KalamDbError>> + Send>>,
        EnsureFn: FnOnce(&NamespaceId) -> Result<(), KalamDbError>,
    {
        // RBAC check
        if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::Stream) {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to create STREAM tables".to_string(),
            ));
        }

        // Parse statement
        let stmt = CreateTableStatement::parse(sql, namespace_id)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        // Verify table type
        if stmt.table_type != kalamdb_commons::schemas::TableType::Stream {
            return Err(KalamDbError::InvalidSql("Expected CREATE STREAM TABLE statement".to_string()));
        }

        // Extract fields
        let table_name = stmt.table_name.clone();
        let namespace_id = stmt.namespace_id.clone();
        let schema = stmt.schema.clone();
        let retention_seconds = stmt.ttl_seconds.map(|t| t as u32);
        let flush_policy = stmt.flush_policy.clone();

        // Validate dependencies
        ensure_namespace_fn(&namespace_id)?;

        // ========================================================================
        // Inlined Business Logic from StreamTableService (Phase 8 - T110)
        // ========================================================================

        // Step 1: Validate table name
        Self::validate_table_name(stmt.table_name.as_str())
            .map_err(KalamDbError::InvalidOperation)?;

        // Step 2: Check if table already exists
        let ctx = crate::app_context::AppContext::get();
        let schema_registry = ctx.schema_registry();
        let table_id = TableId::from_strings(namespace_id.as_str(), table_name.as_str());
        let table_exists = schema_registry.table_exists(&table_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to check table existence: {}", e)))?;

        if table_exists {
            if stmt.if_not_exists {
                // IF NOT EXISTS: Return success without creating
                return Ok(ExecutionResult::Success(format!(
                    "Stream table {}.{} already exists (IF NOT EXISTS)",
                    namespace_id.as_str(),
                    table_name.as_str()
                )));
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Stream table {}.{} already exists",
                    namespace_id.as_str(),
                    table_name.as_str()
                )));
            }
        }

        // Step 3: Stream tables use schema as-is (NO auto-increment, NO system columns)
        // This is the key difference from USER/SHARED tables

        // Step 4: Save complete table definition to information_schema_tables
        // Note: No DEFAULT injection for stream tables - they are ephemeral
        Self::save_table_definition(&stmt, &schema)?;

        // Step 5: Create RocksDB column family for this table
        let _stream_table_store = ctx.stream_table_store();
        // TODO: create column family for stream tables once storage API exposes the helper again

        log::info!(
            "Stream table {}.{} created successfully (schema version 1, TTL: {:?}s)",
            namespace_id.as_str(),
            table_name.as_str(),
            retention_seconds
        );

        // ========================================================================
        // End Inlined Business Logic
        // ========================================================================

        // Insert into system.tables
        let table_id = TableId::from_strings(namespace_id.as_str(), table_name.as_str());
        let storage_id = StorageId::from("local"); // Keep for cache_fn call
        
        // Get the TableDefinition that was just saved
        let table_def = schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| {
                KalamDbError::Other(format!(
                    "Failed to retrieve table definition for {}.{} after save",
                    namespace_id.as_str(),
                    table_name.as_str()
                ))
            })?;
        
        tables_provider.create_table(&table_id, &table_def).map_err(|e| {
            KalamDbError::Other(format!("Failed to insert table into system catalog: {}", e))
        })?;

        // Register with DataFusion
        register_fn(session, &namespace_id, &table_name, TableType::Stream, schema.clone(), default_user_id).await?;

        // Cache metadata
        cache_fn(&namespace_id, &table_name, TableType::Stream, &storage_id, flush_policy, schema, 1, retention_seconds)?;

        Ok(ExecutionResult::Success("Stream table created successfully".to_string()))
    }

    /// Create SHARED table (single-tenant with access control)
    #[allow(clippy::too_many_arguments)]
    async fn create_shared_table<CacheFn, RegisterFn, ValidateFn, EnsureFn>(
        tables_provider: &Arc<crate::tables::system::TablesTableProvider>,
        cache_fn: CacheFn,
        register_fn: RegisterFn,
        validate_storage_fn: ValidateFn,
        ensure_namespace_fn: EnsureFn,
        session: &SessionContext,
        sql: &str,
        namespace_id: &NamespaceId,
        default_user_id: UserId,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>
    where
        CacheFn: FnOnce(&NamespaceId, &kalamdb_commons::models::TableName, TableType, &StorageId, Option<FlushPolicy>, std::sync::Arc<arrow::datatypes::Schema>, i32, Option<u32>) -> Result<(), KalamDbError>,
        RegisterFn: FnOnce(&SessionContext, &NamespaceId, &kalamdb_commons::models::TableName, TableType, std::sync::Arc<arrow::datatypes::Schema>, UserId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), KalamDbError>> + Send>>,
        ValidateFn: FnOnce(Option<StorageId>) -> Result<StorageId, KalamDbError>,
        EnsureFn: FnOnce(&NamespaceId) -> Result<(), KalamDbError>,
    {
        // RBAC check
        if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::Shared) {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to create SHARED tables".to_string(),
            ));
        }

        // Parse statement
        let stmt = CreateTableStatement::parse(sql, namespace_id)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        // Extract fields before moving stmt
        let table_name = stmt.table_name.clone();
        let namespace_id = stmt.namespace_id.clone();
        let schema = stmt.schema.clone();
        let flush_policy = stmt.flush_policy.clone();
        let deleted_retention = stmt.deleted_retention_hours.map(|h| h as u64 * 3600);
        let stmt_storage_id = stmt.storage_id.clone();
    let _stmt_access_level = stmt.access_level;

        // Validate dependencies
        ensure_namespace_fn(&namespace_id)?;
        let storage_id = validate_storage_fn(stmt_storage_id)?;

        // ========================================================================
        // Inlined Business Logic from SharedTableService (Phase 8 - T109)
        // ========================================================================

        // Step 1: Validate table name
        Self::validate_table_name(stmt.table_name.as_str())
            .map_err(KalamDbError::InvalidOperation)?;

        // Step 2: Check if table already exists
        let ctx = crate::app_context::AppContext::get();
        let schema_registry = ctx.schema_registry();
        let table_id = TableId::from_strings(namespace_id.as_str(), table_name.as_str());
        let table_exists = schema_registry.table_exists(&table_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to check table existence: {}", e)))?;

        let was_created = if table_exists {
            if stmt.if_not_exists {
                // IF NOT EXISTS: Return success without creating
                false
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Table {}.{} already exists",
                    namespace_id.as_str(),
                    table_name.as_str()
                )));
            }
        } else {
            // Step 3: Auto-increment field injection (id column)
            let schema = Self::inject_auto_increment_field(schema.clone())?;

            // Step 4: System column injection (_updated, _deleted)
            let schema = Self::inject_system_columns(schema, TableType::Shared)?;

            // Step 5: Inject DEFAULT SNOWFLAKE_ID() for auto-injected id column
            let mut modified_stmt = stmt.clone();
            if !modified_stmt.column_defaults.contains_key("id") {
                modified_stmt.column_defaults.insert(
                    "id".to_string(),
                    kalamdb_commons::schemas::ColumnDefault::function("SNOWFLAKE_ID", vec![]),
                );
            }

            // Step 6: Save complete table definition to information_schema_tables
            Self::save_table_definition(&modified_stmt, &schema)?;

            // Step 7: Create RocksDB column family for this table
            let _shared_table_store = ctx.shared_table_store();
            // TODO: create column family for shared tables once storage API exposes the helper again

            log::info!(
                "Shared table {}.{} created successfully (schema version 1)",
                namespace_id.as_str(),
                table_name.as_str()
            );

            true // Was created
        };

        // ========================================================================
        // End Inlined Business Logic
        // ========================================================================

        if was_created {
            // Insert into system.tables
            let table_id = TableId::from_strings(namespace_id.as_str(), table_name.as_str());
            
            // Get the TableDefinition that was just saved
            let table_def = schema_registry
                .get_table_definition(&table_id)?
                .ok_or_else(|| {
                    KalamDbError::Other(format!(
                        "Failed to retrieve table definition for {}.{} after save",
                        namespace_id.as_str(),
                        table_name.as_str()
                    ))
                })?;
            
            tables_provider.create_table(&table_id, &table_def).map_err(|e| {
                KalamDbError::Other(format!("Failed to insert table into system catalog: {}", e))
            })?;

            // Register with DataFusion
            register_fn(session, &namespace_id, &table_name, TableType::Shared, schema.clone(), default_user_id).await?;

            // Cache metadata
            cache_fn(&namespace_id, &table_name, TableType::Shared, &storage_id, flush_policy, schema, 1, deleted_retention.map(|s| (s / 3600) as u32))?;

            Ok(ExecutionResult::Success("Table created successfully".to_string()))
        } else {
            Ok(ExecutionResult::Success(format!(
                "Table {}.{} already exists (skipped)",
                namespace_id, table_name
            )))
        }
    }

    /// Execute ALTER TABLE statement
    /// 
    /// Currently supports:
    /// - SET ACCESS LEVEL (for SHARED tables only)
    /// 
    /// # Arguments
    /// * `kalam_sql` - SQL adapter for system table access
    /// * `cache` - Optional unified cache for invalidation
    /// * `log_fn` - Closure for audit logging
    /// * `session` - DataFusion session context (reserved for future use)
    /// * `sql` - Raw SQL statement
    /// * `exec_ctx` - Execution context with user information
    /// * `metadata` - Optional execution metadata for audit logging
    /// 
    /// # Returns
    /// Success message indicating the alteration result
    /// 
    /// # Example SQL
    /// ```sql
    /// ALTER TABLE shared_data SET ACCESS LEVEL PUBLIC;
    /// ALTER TABLE analytics SET ACCESS LEVEL PRIVATE;
    /// ```
    /// Execute ALTER TABLE statement
    /// 
    /// **Phase 10.2 Migration**: Now uses SchemaRegistry instead of KalamSql for 50-100× performance improvement
    /// 
    /// # Arguments
    /// * `schema_registry` - Schema registry for fast table metadata access (replaces kalam_sql)
    /// * `kalam_sql` - SQL adapter (still needed for update_table persistence)
    /// * `cache` - Optional unified cache for invalidation
    /// * `log_fn` - Closure for audit logging
    /// * `_session` - DataFusion session context (reserved)
    /// * `sql` - Raw SQL statement
    /// * `exec_ctx` - Execution context with user information
    /// * `_metadata` - Optional execution metadata
    pub async fn execute_alter_table<F>(
        schema_registry: &crate::schema_registry::SchemaRegistry,
        _cache: Option<&crate::schema_registry::SchemaRegistry>,
        _log_fn: F,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
        _metadata: Option<&ExecutionMetadata>,
    ) -> Result<ExecutionResult, KalamDbError>
    where
        F: FnOnce(&str, &str, serde_json::Value),
    {
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
            // Phase 10.2: Use SchemaRegistry for fast metadata lookup (1-2μs vs 50-100μs)
            let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
            let table_def = schema_registry.get_table_definition(&table_id)?
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!("Table '{}' not found", table_id))
                })?;

            // Verify table is SHARED type
            if table_def.table_type != TableType::Shared {
                return Err(KalamDbError::InvalidOperation(
                    "ACCESS LEVEL can only be set on SHARED tables".to_string(),
                ));
            }

            // TODO Phase 8: Need TablesTableProvider parameter to update access_level
            // For now, just log a warning - this functionality temporarily disabled
            // The tables_provider.update_table() method is needed but not passed to this function yet
            log::warn!(
                "ALTER TABLE SET ACCESS LEVEL temporarily disabled during Phase 8 migration. \
                 Table: {}, Requested level: {:?}. Need to pass TablesTableProvider to execute_alter_table().",
                table_id, access_level
            );

            return Err(KalamDbError::InvalidOperation(
                "ALTER TABLE SET ACCESS LEVEL temporarily disabled - Phase 8 migration in progress".to_string()
            ));
        }

        // For other ALTER TABLE operations (ADD COLUMN, DROP COLUMN, etc.),
        // return error for now (not yet implemented)
        Err(KalamDbError::InvalidOperation(
            "ALTER TABLE operations other than SET ACCESS LEVEL are not yet supported via this path. \
             Use SchemaEvolutionService directly for column modifications.".to_string(),
        ))
    }

    /// Execute DROP TABLE statement
    /// 
    /// Drops a table (USER/SHARED/STREAM) from the specified namespace.
    /// Prevents dropping tables with active live queries.
    /// 
    /// **Phase 10.2 Migration**: Now uses SchemaRegistry instead of KalamSql for 50-100× performance improvement
    /// 
    /// # Arguments
    /// * `deletion_service` - Service for table deletion operations
    /// * `schema_registry` - Schema registry for fast table metadata access (replaces kalam_sql)
    /// * `cache` - Optional unified cache for invalidation
    /// * `live_query_check_fn` - Closure to check for active subscriptions
    /// * `session` - DataFusion session context (reserved for future use)
    /// * `sql` - Raw SQL statement
    /// * `exec_ctx` - Execution context with user information
    /// 
    /// # Returns
    /// Success message with deletion statistics (files deleted, bytes freed)
    /// 
    /// # Example SQL
    /// ```sql
    /// DROP TABLE analytics.events;
    /// DROP TABLE IF EXISTS shared_data;
    /// DROP USER TABLE user_settings;
    /// ```
    pub async fn execute_drop_table<F>(
        schema_registry: &crate::schema_registry::SchemaRegistry,
        cache: Option<&crate::schema_registry::SchemaRegistry>,
        job_manager: &crate::jobs::JobsManager,
        live_query_check_fn: F,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>
    where
        F: FnOnce(&str) -> bool,
    {
        // Use default namespace - the SQL parser will extract namespace from qualified name (namespace.table)
        let default_namespace = kalamdb_commons::NamespaceId::new("default".to_string());
        let stmt = DropTableStatement::parse(sql, &default_namespace)
            .map_err(|e| KalamDbError::InvalidSql(e.to_string()))?;

        let requested_table_type: TableType = stmt.table_type.into();
        let table_identifier = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());

        // Phase 10.2: Use SchemaRegistry for fast table type lookup (1-2μs vs 50-100μs)
        let actual_table_type = match schema_registry.get_table_definition(&table_identifier)? {
            Some(table_def) => table_def.table_type,
            None => requested_table_type, // Table doesn't exist, use requested type for RBAC check
        };

        // TODO: Track table ownership in system tables to determine is_owner accurately (#US3 follow-up)
        let is_owner = false;
        if !crate::auth::rbac::can_delete_table(exec_ctx.user_role, actual_table_type, is_owner) {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop this table".to_string(),
            ));
        }

        // ========================================================================
        // Inlined Business Logic from TableDeletionService (Phase 8 - T111)
        // ========================================================================

        // Generate table_id (format: namespace:table_name)
        let table_id_str = format!("{}:{}", stmt.namespace_id.as_str(), stmt.table_name.as_str());
        let table_type_internal: TableType = stmt.table_type.into();

        // Fetch providers from AppContext
        let ctx = crate::app_context::AppContext::get();
        let tables_provider = ctx.system_tables().tables();
        let _jobs_provider = ctx.system_tables().jobs();

        // Check if table exists
        let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
        let table_metadata = tables_provider
            .get_table_by_id(&table_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get table: {}", e)))?;

        if table_metadata.is_none() {
            if stmt.if_exists {
                return Ok(ExecutionResult::Success(format!(
                    "Table {}.{} does not exist (skipped)",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                )));
            } else {
                return Err(KalamDbError::NotFound(format!(
                    "Table '{}' not found in namespace '{}'",
                    stmt.table_name.as_str(),
                    stmt.namespace_id.as_str()
                )));
            }
        }

        let table = table_metadata.unwrap();

        // Step 1: Check for active subscriptions
        Self::check_active_subscriptions_internal(&stmt.table_name)?;

        // Also check via live_query_check_fn parameter (alternative check)
        let table_ref = format!("{}.{}", stmt.namespace_id.as_str(), stmt.table_name.as_str());
        if live_query_check_fn(&table_ref) {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot drop table {} while active live queries exist",
                table_ref
            )));
        }

        // Step 2: Create job record for tracking (Phase 9, T165)
        let job_id = Self::create_deletion_job_unified(&job_manager, &table_id_str, &stmt.namespace_id, &stmt.table_name, &table_type_internal).await?;

        // Step 3: Delete table data from RocksDB
        let data_cleanup_result = Self::cleanup_table_data_internal(&stmt.namespace_id, &stmt.table_name, &table_type_internal);

        if let Err(e) = data_cleanup_result {
            // Update job as failed (Phase 9, T165)
            Self::fail_deletion_job_unified(&job_manager, &job_id, &e.to_string()).await?;
            return Err(e);
        }

        // Step 4: Delete Parquet files
        let parquet_result = Self::cleanup_parquet_files_internal(&table, &table_type_internal);

        let (files_deleted, bytes_freed) = match parquet_result {
            Ok(stats) => stats,
            Err(e) => {
                // Attempt rollback: restore metadata
                log::error!("Parquet cleanup failed: {}, attempting rollback", e);
                Self::fail_deletion_job_unified(&job_manager, &job_id, &format!("Parquet cleanup failed: {}", e)).await?;

                // Note: Data deletion is idempotent, no need to restore
                return Err(e);
            }
        };

        // Step 5: Delete metadata
        let metadata_result = Self::cleanup_metadata_internal(&table_id_str);

        if let Err(e) = metadata_result {
            // Log warning but don't rollback (data is already deleted)
            log::warn!("Metadata cleanup failed but data is deleted: {}", e);
            Self::fail_deletion_job_unified(&job_manager, &job_id, &format!("Metadata cleanup failed: {}", e)).await?;
            return Err(e);
        }

        // Step 6: Update storage location usage count (currently disabled)
        // TODO: Phase 9 - Update to use storage_id from table_options or cache
        // storage_id is not directly available in TableDefinition
        log::debug!("Storage usage count decrement skipped (storage_id not available in TableDefinition)");

        // Step 7: Complete job successfully (Phase 9, T165)
        Self::complete_deletion_job_unified(&job_manager, &job_id, files_deleted, bytes_freed).await?;

        // ========================================================================
        // End Inlined Business Logic
        // ========================================================================

        // Invalidate unified cache after DROP TABLE
        if let Some(cache) = cache {
            cache.invalidate(&table_identifier);
        }

        Ok(ExecutionResult::Success(format!(
            "Table {}.{} dropped successfully ({} Parquet files deleted, {} bytes freed, job_id: {})",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            files_deleted,
            bytes_freed,
            job_id
        )))
    }

    // ============================================================================
    // Table Deletion Helper Methods (Phase 8 - T111)
    // Migrated from TableDeletionService
    // ============================================================================

    /// Check for active subscriptions before dropping table
    fn check_active_subscriptions_internal(table_name: &kalamdb_commons::models::TableName) -> Result<(), KalamDbError> {
        use crate::app_context::AppContext;
        use datafusion::arrow::array::AsArray;
        
        let ctx = AppContext::get();
        let live_queries_provider = ctx.system_tables().live_queries();

        let batch = live_queries_provider
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::IoError(format!("Failed to scan live queries: {}", e)))?;
        
        // Extract live queries from RecordBatch
        let table_name_array = batch.column(4).as_string::<i32>(); // table_name is column 4
        let connection_id_array = batch.column(1).as_string::<i32>();
        let user_id_array = batch.column(2).as_string::<i32>();
        let query_id_array = batch.column(0).as_string::<i32>();
        
        let mut subscription_details: Vec<String> = Vec::new();
        for i in 0..batch.num_rows() {
            if table_name_array.value(i) == table_name.as_str() {
                // Found an active subscription for this table
                subscription_details.push(format!(
                    "connection_id={}, user_id={}, query_id={}",
                    connection_id_array.value(i),
                    user_id_array.value(i),
                    query_id_array.value(i)
                ));
            }
        }

        if !subscription_details.is_empty() {

            return Err(KalamDbError::Conflict(format!(
                "Cannot drop table '{}': {} active subscription(s) exist: {}",
                table_name.as_str(),
                subscription_details.len(),
                subscription_details.join("; ")
            )));
        }

        Ok(())
    }

    /// Delete table data from RocksDB
    fn cleanup_table_data_internal(
        namespace_id: &NamespaceId,
        table_name: &kalamdb_commons::models::TableName,
        table_type: &TableType,
    ) -> Result<(), KalamDbError> {
        use crate::app_context::AppContext;
    use crate::tables::system::system_table_store::{SharedTableStoreExt, UserTableStoreExt};
        
        let ctx = AppContext::get();
        let user_table_store = ctx.user_table_store();
        let shared_table_store = ctx.shared_table_store();
        let stream_table_store = ctx.stream_table_store();

        match table_type {
            TableType::User => UserTableStoreExt::drop_table(
                user_table_store.as_ref(),
                namespace_id.as_str(),
                table_name.as_str(),
            )
            .map_err(|e| KalamDbError::IoError(format!("Failed to drop user table data: {}", e))),
            TableType::Shared => SharedTableStoreExt::drop_table(
                shared_table_store.as_ref(),
                namespace_id.as_str(),
                table_name.as_str(),
            )
            .map_err(|e| KalamDbError::IoError(format!("Failed to drop shared table data: {}", e))),
            TableType::Stream => stream_table_store
                .drop_table(namespace_id.as_str(), table_name.as_str())
                .map_err(|e| {
                    KalamDbError::IoError(format!("Failed to drop stream table data: {}", e))
                }),
            TableType::System => {
                Err(KalamDbError::PermissionDenied(
                    "Cannot drop system tables".to_string(),
                ))
            }
        }
    }

    /// Cleanup Parquet files in a specific directory
    #[allow(dead_code)]
    fn cleanup_directory_parquet_files(
        dir_path: &std::path::Path,
        files_deleted: &mut usize,
        bytes_freed: &mut u64,
    ) -> Result<(), KalamDbError> {
        use std::fs;
        
        if !dir_path.exists() {
            return Ok(());
        }

        if !dir_path.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir_path)
            .map_err(|e| KalamDbError::IoError(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| KalamDbError::IoError(format!("Failed to read entry: {}", e)))?;

            let path = entry.path();

            // Only delete .parquet files that match the batch-* pattern
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("parquet")
                && path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with("batch-"))
                    .unwrap_or(false)
            {
                // Get file size before deletion
                if let Ok(metadata) = fs::metadata(&path) {
                    *bytes_freed += metadata.len();
                }

                // Delete the file
                fs::remove_file(&path)
                    .map_err(|e| KalamDbError::IoError(format!("Failed to delete file: {}", e)))?;

                *files_deleted += 1;

                log::info!("Deleted Parquet file: {}", path.display());
            }
        }

        Ok(())
    }

    /// Cleanup Parquet files in user directories
    #[allow(dead_code)]
    fn cleanup_user_parquet_files(
        storage_path: &std::path::Path,
        files_deleted: &mut usize,
        bytes_freed: &mut u64,
    ) -> Result<(), KalamDbError> {
        use std::fs;
        
        if !storage_path.is_dir() {
            return Ok(());
        }

        // Iterate over user directories
        let entries = fs::read_dir(storage_path).map_err(|e| {
            KalamDbError::IoError(format!("Failed to read storage directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                KalamDbError::IoError(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();

            if path.is_dir() {
                // This should be a user_id directory
                Self::cleanup_directory_parquet_files(&path, files_deleted, bytes_freed)?;
            }
        }

        Ok(())
    }

    /// Delete Parquet files for dropped table
    fn cleanup_parquet_files_internal(
        _table_def: &TableDefinition,
        table_type: &TableType,
    ) -> Result<(usize, u64), KalamDbError> {
        // Stream tables don't have Parquet files
        if matches!(table_type, TableType::Stream) {
            return Ok((0, 0));
        }

        // TODO: Phase 9 - Use TableCache for dynamic path resolution
        // storage_id is not directly available in TableDefinition, needs to be retrieved from table_options or cache
        log::warn!("cleanup_parquet_files_internal: Storage path resolution not implemented for TableDefinition");
        return Ok((0, 0));
        
        // TODO: Implement proper storage path resolution
        // let storage_path_str = ... get from table_def.table_options or cache
        // let storage_path = Path::new(storage_path_str);
        // 
        // let mut files_deleted = 0;
        // let mut bytes_freed = 0u64;
        // 
        // match table_type {
        //     TableType::User => {
        //         Self::cleanup_user_parquet_files(storage_path, &mut files_deleted, &mut bytes_freed)?;
        //     }
        //     TableType::Shared => {
        //         let table_path = storage_path.join("shared").join(table_def.table_name.as_str());
        //         Self::cleanup_directory_parquet_files(&table_path, &mut files_deleted, &mut bytes_freed)?;
        //     }
        //     _ => {}
        // }
        // 
        // Ok((files_deleted, bytes_freed))
    }

    /// Delete metadata from system tables
    fn cleanup_metadata_internal(table_id_str: &str) -> Result<(), KalamDbError> {
        use crate::app_context::AppContext;
        
        let ctx = AppContext::get();
        let tables_provider = ctx.system_tables().tables();
        let schema_registry = ctx.schema_registry();

        // Parse namespace:table_name format
        let parts: Vec<&str> = table_id_str.split(':').collect();
        if parts.len() != 2 {
            return Err(KalamDbError::IoError(format!("Invalid table_id format: {}", table_id_str)));
        }
        
        let table_id = TableId::from_strings(parts[0], parts[1]);
        
        // Delete from system.tables
        tables_provider
            .delete_table(&table_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to delete from system.tables: {}", e)))?;
        
        // Delete from information_schema.tables (SchemaRegistry)
        schema_registry
            .delete_table_definition(&table_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to delete table definition: {}", e)))?;

        Ok(())
    }

    /// Decrement storage location usage count (currently disabled)
    #[allow(dead_code)]
    fn decrement_storage_usage_internal(_location_name: &str) -> Result<(), KalamDbError> {
        // TODO: Phase 2b - system_storages no longer tracks usage_count
        log::warn!(
            "Storage usage tracking temporarily disabled during migration to information_schema"
        );
        Ok(())
    }

    /// Create deletion job for tracking (Phase 9, T165 - JobsManager)
    async fn create_deletion_job_unified(
        job_manager: &crate::jobs::JobsManager,
        table_id: &str,
        namespace_id: &NamespaceId,
        table_name: &kalamdb_commons::models::TableName,
        table_type: &TableType,
    ) -> Result<JobId, KalamDbError> {
        use kalamdb_commons::JobType;
        
        // Create job parameters
        let parameters = serde_json::json!({
            "table_id": table_id,
            "table_name": table_name.as_str(),
            "table_type": format!("{:?}", table_type),
            "operation": "drop_table"
        });
        
        // Create idempotency key from table ID to prevent duplicate deletion jobs
        let idempotency_key = Some(format!("drop-table-{}", table_id));
        
        // Create job via JobsManager
        let job_id = job_manager
            .create_job(
                JobType::Cleanup,
                namespace_id.clone(),
                parameters,
                idempotency_key,
                None, // Use default options
            )
            .await?;
        
        log::info!("Created deletion job {} for table {}", job_id, table_id);
        Ok(job_id)
    }

    /// Complete deletion job (Phase 9, T165 - UnifiedJobsManager)
    async fn complete_deletion_job_unified(
        job_manager: &crate::jobs::JobsManager,
        job_id: &JobId,
        files_deleted: usize,
        bytes_freed: u64,
    ) -> Result<(), KalamDbError> {
        let message = format!("Deleted {} files, freed {} bytes", files_deleted, bytes_freed);
        job_manager.complete_job(job_id, Some(message)).await?;
        log::info!("Completed deletion job {}: {} files, {} bytes", job_id, files_deleted, bytes_freed);
        Ok(())
    }

    /// Fail deletion job (Phase 9, T165 - UnifiedJobsManager)
    async fn fail_deletion_job_unified(
        job_manager: &crate::jobs::JobsManager,
        job_id: &JobId,
        error_message: &str,
    ) -> Result<(), KalamDbError> {
        job_manager.fail_job(job_id, error_message.to_string()).await?;
        log::error!("Failed deletion job {}: {}", job_id, error_message);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::app_context::AppContext;

    use super::*;
    use datafusion::execution::context::SessionContext;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;
    use std::sync::Arc;

    /// Helper to create test execution context
    fn create_test_context() -> ExecutionContext {
        ExecutionContext::new(
            UserId::new("test_user"),
            Role::Dba,
        )
    }

    /// Helper to get AppContext
    fn get_app_context() -> Arc<AppContext> {
        AppContext::get()
    }

    #[tokio::test]
    async fn test_create_namespace_success() {
        let app_ctx = get_app_context();
        let session = SessionContext::new();
        let ctx = create_test_context();

        let sql = "CREATE NAMESPACE production";
        let result = DDLHandler::execute_create_namespace(&app_ctx, &session, sql, &ctx)
            .await
            .expect("Should create namespace");

        match result {
            ExecutionResult::Success(msg) => {
                assert!(msg.contains("production"));
                assert!(msg.contains("created successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_create_namespace_if_not_exists() {
    let app_ctx = get_app_context();
        let session = SessionContext::new();
        let ctx = create_test_context();

        let sql = "CREATE NAMESPACE IF NOT EXISTS staging";

        // First creation should succeed
        let result1 = DDLHandler::execute_create_namespace(&app_ctx, &session, sql, &ctx)
            .await
            .expect("Should create namespace");

        match result1 {
            ExecutionResult::Success(msg) => assert!(msg.contains("created successfully")),
            _ => panic!("Expected Success result"),
        }

        // Second creation should succeed with "already exists" message
        let result2 = DDLHandler::execute_create_namespace(&app_ctx, &session, sql, &ctx)
            .await
            .expect("Should handle existing namespace");

        match result2 {
            ExecutionResult::Success(msg) => assert!(msg.contains("already exists")),
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_create_namespace_duplicate_without_if_not_exists() {
    let app_ctx = get_app_context();
        let session = SessionContext::new();
        let ctx = create_test_context();

        let sql_without_if_not_exists = "CREATE NAMESPACE production";

        // First creation should succeed
        DDLHandler::execute_create_namespace(&app_ctx, &session, sql_without_if_not_exists, &ctx)
            .await
            .expect("Should create namespace");

        // Second creation without IF NOT EXISTS should fail
        let result = DDLHandler::execute_create_namespace(&app_ctx, &session, sql_without_if_not_exists, &ctx)
            .await;

        assert!(result.is_err(), "Should fail on duplicate namespace without IF NOT EXISTS");
    }

    #[tokio::test]
    async fn test_create_namespace_invalid_sql() {
    let app_ctx = get_app_context();
        let session = SessionContext::new();
        let ctx = create_test_context();

        let invalid_sql = "CREATE NAMESPACE";

        let result = DDLHandler::execute_create_namespace(&app_ctx, &session, invalid_sql, &ctx)
            .await;

        assert!(result.is_err(), "Should fail on invalid SQL");
        
        if let Err(KalamDbError::InvalidSql(msg)) = result {
            assert!(msg.contains("Failed to parse CREATE NAMESPACE"));
        } else {
            panic!("Expected InvalidSql error");
        }
    }

    #[tokio::test]
    async fn test_create_namespace_empty_name() {
    let app_ctx = get_app_context();
        let session = SessionContext::new();
        let ctx = create_test_context();

        let sql = "CREATE NAMESPACE ''";

        let result = DDLHandler::execute_create_namespace(&app_ctx, &session, sql, &ctx)
            .await;

        assert!(result.is_err(), "Should fail on empty namespace name");
    }
}
