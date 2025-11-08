//! CREATE TABLE helper functions
//!
//! Provides unified logic for creating all table types (USER/SHARED/STREAM)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::models::ExecutionContext;
use kalamdb_commons::models::{NamespaceId, StorageId, TableId};
use kalamdb_commons::schemas::TableType;
use kalamdb_sql::ddl::CreateTableStatement;
use std::sync::Arc;

/// Route CREATE TABLE to type-specific handler
///
/// # Arguments
/// * `app_context` - Application context
/// * `stmt` - Parsed CREATE TABLE statement
/// * `exec_ctx` - Execution context (user, role)
///
/// # Returns
/// Ok with success message, or error
pub fn create_table(
    app_context: Arc<AppContext>,
    stmt: CreateTableStatement,
    exec_ctx: &ExecutionContext,
) -> Result<String, KalamDbError> {
    let table_id_str = format!("{}.{}", stmt.namespace_id.as_str(), stmt.table_name.as_str());
    let table_type = stmt.table_type;
    let user_id_str = exec_ctx.user_id.as_str().to_string();
    let user_role = exec_ctx.user_role;

    log::info!(
        "🔨 CREATE TABLE request: {} (type: {:?}, user: {}, role: {:?})",
        table_id_str,
        table_type,
        user_id_str,
        user_role
    );

    // Route to type-specific handler
    let result = match stmt.table_type {
        TableType::User => create_user_table(app_context, stmt, exec_ctx),
        TableType::Shared => create_shared_table(app_context, stmt, exec_ctx),
        TableType::Stream => create_stream_table(app_context, stmt, exec_ctx),
        TableType::System => {
            log::error!(
                "❌ CREATE TABLE failed: Cannot create SYSTEM tables via SQL ({})",
                table_id_str
            );
            Err(KalamDbError::InvalidOperation(
                "Cannot create SYSTEM tables via SQL".to_string(),
            ))
        }
    };

    // Log result
    match &result {
        Ok(msg) => log::info!("✅ CREATE TABLE succeeded: {}", msg),
        Err(e) => log::error!(
            "❌ CREATE TABLE failed for {}: {}",
            table_id_str,
            e
        ),
    }

    result
}

/// Create USER table (multi-tenant with automatic user_id filtering)
pub fn create_user_table(
    app_context: Arc<AppContext>,
    stmt: CreateTableStatement,
    exec_ctx: &ExecutionContext,
) -> Result<String, KalamDbError> {
    use super::tables::{inject_auto_increment_field, inject_system_columns, save_table_definition, validate_table_name};
    use kalamdb_commons::schemas::ColumnDefault;

    // RBAC check
    if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::User) {
        log::error!(
            "❌ CREATE USER TABLE {}.{}: Insufficient privileges (user: {}, role: {:?})",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            exec_ctx.user_id.as_str(),
            exec_ctx.user_role
        );
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create USER tables".to_string(),
        ));
    }

    log::debug!(
        "📋 CREATE USER TABLE {}.{}: columns={}, storage={:?}, ttl={:?}",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        stmt.schema.fields().len(),
        stmt.storage_id,
        stmt.ttl_seconds
    );

    // Validate table name
    validate_table_name(stmt.table_name.as_str())
        .map_err(KalamDbError::InvalidOperation)?;

    // Check if table already exists
    let schema_registry = app_context.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
    let table_exists = schema_registry
        .table_exists(&table_id)
        .map_err(|e| KalamDbError::Other(format!("Failed to check table existence: {}", e)))?;

    if table_exists {
        if stmt.if_not_exists {
            log::info!(
                "ℹ️  USER TABLE {}.{} already exists (IF NOT EXISTS - skipping)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            );
            return Ok(format!(
                "Table {}.{} already exists (IF NOT EXISTS)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ));
        } else {
            log::error!(
                "❌ CREATE USER TABLE failed: {}.{} already exists",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            );
            return Err(KalamDbError::AlreadyExists(format!(
                "Table {}.{} already exists",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            )));
        }
    }

    // Validate namespace exists
    let namespaces_provider = app_context.system_tables().namespaces();
    let namespace_id = NamespaceId::new(stmt.namespace_id.as_str());
    if namespaces_provider.get_namespace(&namespace_id)?.is_none() {
        log::error!(
            "❌ CREATE USER TABLE failed: Namespace '{}' does not exist",
            stmt.namespace_id.as_str()
        );
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace '{}' does not exist",
            stmt.namespace_id.as_str()
        )));
    }

    // Validate storage exists (if specified)
    let storage_id = if let Some(ref sid) = stmt.storage_id {
        let storages_provider = app_context.system_tables().storages();
        let storage_id = StorageId::from(sid.as_str());
        if storages_provider.get_storage_by_id(&storage_id)?.is_none() {
            log::error!(
                "❌ CREATE USER TABLE failed: Storage '{}' does not exist",
                sid
            );
            return Err(KalamDbError::InvalidOperation(format!(
                "Storage '{}' does not exist",
                sid
            )));
        }
        storage_id
    } else {
        StorageId::from("local") // Default storage
    };

    log::debug!(
        "✓ Validation passed for USER TABLE {}.{} (storage: {})",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str()
    );

    // Auto-increment field injection (id column)
    let schema = inject_auto_increment_field(stmt.schema.clone())?;

    // System column injection (_updated, _deleted)
    let schema = inject_system_columns(schema, TableType::User)?;

    // Inject DEFAULT SNOWFLAKE_ID() for auto-injected id column
    let mut modified_stmt = stmt.clone();
    if !modified_stmt.column_defaults.contains_key("id") {
        modified_stmt.column_defaults.insert(
            "id".to_string(),
            ColumnDefault::function("SNOWFLAKE_ID", vec![]),
        );
    }

    // Save complete table definition to information_schema.tables
    save_table_definition(&modified_stmt, &schema)?;

    // Insert into system.tables
    let table_def = schema_registry
        .get_table_definition(&table_id)?
        .ok_or_else(|| {
            KalamDbError::Other(format!(
                "Failed to retrieve table definition for {}.{} after save",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ))
        })?;

    let tables_provider = app_context.system_tables().tables();
    tables_provider.create_table(&table_id, &table_def).map_err(|e| {
        KalamDbError::Other(format!("Failed to insert table into system catalog: {}", e))
    })?;

    // Register SharedTableProvider for CRUD access (mirrors user table shared registration)
    use crate::tables::shared_tables::{shared_table_store::new_shared_table_store, SharedTableProvider};
    let shared_store = Arc::new(new_shared_table_store(
        app_context.storage_backend(),
        &table_id.namespace_id(),
        &table_id.table_name(),
    ));
    let provider = SharedTableProvider::new(
        Arc::new(table_id.clone()),
        app_context.schema_registry(),
        schema.clone(),
        shared_store,
    );
    // Cache provider for DataFusion / execution path
    schema_registry.insert_provider(table_id.clone(), Arc::new(provider));

    // Create and register UserTableShared instance for INSERT/UPDATE/DELETE operations
    use crate::tables::base_table_provider::UserTableShared;
    use crate::tables::user_tables::new_user_table_store;
    
    // Create user table store for this table
    let user_table_store = Arc::new(new_user_table_store(
        app_context.storage_backend(),
        &table_id.namespace_id(),
        &table_id.table_name(),
    ));
    
    // Create shared state (returns Arc<UserTableShared>)
    let shared = UserTableShared::new(
        Arc::new(table_id.clone()),
        app_context.schema_registry(),
        schema.clone(),
        user_table_store,
    );
    
    schema_registry.insert_user_table_shared(table_id.clone(), shared);

    // Log detailed success with table options
    log::info!(
        "✅ USER TABLE created: {}.{} | storage: {} | columns: {} | auto_increment: true | system_columns: [_updated, _deleted]",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str(),
        schema.fields().len()
    );

    Ok(format!(
        "User table {}.{} created successfully",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str()
    ))
}

/// Create SHARED table (single-tenant with access control)
pub fn create_shared_table(
    app_context: Arc<AppContext>,
    stmt: CreateTableStatement,
    exec_ctx: &ExecutionContext,
) -> Result<String, KalamDbError> {
    use super::tables::{inject_auto_increment_field, inject_system_columns, save_table_definition, validate_table_name};
    use kalamdb_commons::schemas::ColumnDefault;

    // RBAC check
    if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::Shared) {
        log::error!(
            "❌ CREATE SHARED TABLE {}.{}: Insufficient privileges (user: {}, role: {:?})",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            exec_ctx.user_id.as_str(),
            exec_ctx.user_role
        );
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create SHARED tables".to_string(),
        ));
    }

    log::debug!(
        "📋 CREATE SHARED TABLE {}.{}: columns={}, storage={:?}, access_level={:?}",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        stmt.schema.fields().len(),
        stmt.storage_id,
        stmt.access_level
    );

    // Validate table name
    validate_table_name(stmt.table_name.as_str())
        .map_err(KalamDbError::InvalidOperation)?;

    // Check if table already exists
    let schema_registry = app_context.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
    let table_exists = schema_registry
        .table_exists(&table_id)
        .map_err(|e| KalamDbError::Other(format!("Failed to check table existence: {}", e)))?;

    if table_exists {
        if stmt.if_not_exists {
            log::info!(
                "ℹ️  SHARED TABLE {}.{} already exists (IF NOT EXISTS - skipping)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            );
            return Ok(format!(
                "Table {}.{} already exists (IF NOT EXISTS)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ));
        } else {
            log::error!(
                "❌ CREATE SHARED TABLE failed: {}.{} already exists",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            );
            return Err(KalamDbError::AlreadyExists(format!(
                "Table {}.{} already exists",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            )));
        }
    }

    // Validate namespace exists
    let namespaces_provider = app_context.system_tables().namespaces();
    let namespace_id = NamespaceId::new(stmt.namespace_id.as_str());
    if namespaces_provider.get_namespace(&namespace_id)?.is_none() {
        log::error!(
            "❌ CREATE SHARED TABLE failed: Namespace '{}' does not exist",
            stmt.namespace_id.as_str()
        );
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace '{}' does not exist",
            stmt.namespace_id.as_str()
        )));
    }

    // Validate storage exists (if specified)
    let storage_id = if let Some(ref sid) = stmt.storage_id {
        let storages_provider = app_context.system_tables().storages();
        let storage_id = StorageId::from(sid.as_str());
        if storages_provider.get_storage_by_id(&storage_id)?.is_none() {
            log::error!(
                "❌ CREATE SHARED TABLE failed: Storage '{}' does not exist",
                sid
            );
            return Err(KalamDbError::InvalidOperation(format!(
                "Storage '{}' does not exist",
                sid
            )));
        }
        storage_id
    } else {
        StorageId::from("local") // Default storage
    };

    log::debug!(
        "✓ Validation passed for SHARED TABLE {}.{} (storage: {})",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str()
    );

    // Auto-increment field injection (id column)
    let schema = inject_auto_increment_field(stmt.schema.clone())?;

    // System column injection (_updated, _deleted)
    let schema = inject_system_columns(schema, TableType::Shared)?;

    // Inject DEFAULT SNOWFLAKE_ID() for auto-injected id column
    let mut modified_stmt = stmt.clone();
    if !modified_stmt.column_defaults.contains_key("id") {
        modified_stmt.column_defaults.insert(
            "id".to_string(),
            ColumnDefault::function("SNOWFLAKE_ID", vec![]),
        );
    }

    // Save complete table definition to information_schema.tables
    save_table_definition(&modified_stmt, &schema)?;

    // Insert into system.tables
    let table_def = schema_registry
        .get_table_definition(&table_id)?
        .ok_or_else(|| {
            KalamDbError::Other(format!(
                "Failed to retrieve table definition for {}.{} after save",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ))
        })?;

    let tables_provider = app_context.system_tables().tables();
    tables_provider.create_table(&table_id, &table_def).map_err(|e| {
        KalamDbError::Other(format!("Failed to insert table into system catalog: {}", e))
    })?;

    // Register StreamTableProvider for event operations
    use crate::tables::stream_tables::{stream_table_store::new_stream_table_store, StreamTableProvider};
    let stream_store = Arc::new(new_stream_table_store(
        &table_id.namespace_id(),
        &table_id.table_name(),
    ));
    let provider = StreamTableProvider::new(
        Arc::new(table_id.clone()),
        app_context.schema_registry(),
        stream_store,
        stmt.ttl_seconds.map(|v| v as u32),
        false, // ephemeral default
        None,  // max_buffer default
    );
    schema_registry.insert_provider(table_id.clone(), Arc::new(provider));

    // Log detailed success with table options
    log::info!(
        "✅ SHARED TABLE created: {}.{} | storage: {} | columns: {} | access_level: {:?} | auto_increment: true | system_columns: [_updated, _deleted]",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str(),
        schema.fields().len(),
        stmt.access_level
    );

    Ok(format!(
        "Shared table {}.{} created successfully",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str()
    ))
}

/// Create STREAM table (TTL-based ephemeral table)
pub fn create_stream_table(
    app_context: Arc<AppContext>,
    stmt: CreateTableStatement,
    exec_ctx: &ExecutionContext,
) -> Result<String, KalamDbError> {
    use super::tables::{save_table_definition, validate_table_name};

    // RBAC check
    if !crate::auth::rbac::can_create_table(exec_ctx.user_role, TableType::Stream) {
        log::error!(
            "❌ CREATE STREAM TABLE {}.{}: Insufficient privileges (user: {}, role: {:?})",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            exec_ctx.user_id.as_str(),
            exec_ctx.user_role
        );
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create STREAM tables".to_string(),
        ));
    }

    log::debug!(
        "📋 CREATE STREAM TABLE {}.{}: columns={}, ttl={:?}s",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        stmt.schema.fields().len(),
        stmt.ttl_seconds
    );

    // Validate table name
    validate_table_name(stmt.table_name.as_str())
        .map_err(KalamDbError::InvalidOperation)?;

    // Check if table already exists
    let schema_registry = app_context.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
    let table_exists = schema_registry
        .table_exists(&table_id)
        .map_err(|e| KalamDbError::Other(format!("Failed to check table existence: {}", e)))?;

    if table_exists {
        if stmt.if_not_exists {
            log::info!(
                "ℹ️  STREAM TABLE {}.{} already exists (IF NOT EXISTS - skipping)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            );
            return Ok(format!(
                "Stream table {}.{} already exists (IF NOT EXISTS)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ));
        } else {
            log::error!(
                "❌ CREATE STREAM TABLE failed: {}.{} already exists",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            );
            return Err(KalamDbError::AlreadyExists(format!(
                "Stream table {}.{} already exists",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            )));
        }
    }

    // Validate namespace exists
    let namespaces_provider = app_context.system_tables().namespaces();
    let namespace_id = NamespaceId::new(stmt.namespace_id.as_str());
    if namespaces_provider.get_namespace(&namespace_id)?.is_none() {
        log::error!(
            "❌ CREATE STREAM TABLE failed: Namespace '{}' does not exist",
            stmt.namespace_id.as_str()
        );
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace '{}' does not exist",
            stmt.namespace_id.as_str()
        )));
    }

    // Stream tables use schema as-is (NO auto-increment, NO system columns)
    let schema = stmt.schema.clone();

    // Validate TTL is specified
    if stmt.ttl_seconds.is_none() {
        log::error!(
            "❌ CREATE STREAM TABLE failed: {}.{} - TTL clause is required",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str()
        );
        return Err(KalamDbError::InvalidOperation(
            "STREAM tables require TTL clause (e.g., TTL 3600)".to_string(),
        ));
    }

    let ttl_seconds = stmt.ttl_seconds.unwrap();
    log::debug!(
        "✓ Validation passed for STREAM TABLE {}.{} (TTL: {}s)",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        ttl_seconds
    );

    // Save complete table definition to information_schema.tables
    save_table_definition(&stmt, &schema)?;

    // Insert into system.tables
    let table_def = schema_registry
        .get_table_definition(&table_id)?
        .ok_or_else(|| {
            KalamDbError::Other(format!(
                "Failed to retrieve table definition for {}.{} after save",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ))
        })?;

    let tables_provider = app_context.system_tables().tables();
    tables_provider.create_table(&table_id, &table_def).map_err(|e| {
        KalamDbError::Other(format!("Failed to insert table into system catalog: {}", e))
    })?;

    // Log detailed success with table options
    log::info!(
        "✅ STREAM TABLE created: {}.{} | columns: {} | TTL: {}s | auto_increment: false | system_columns: none",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        schema.fields().len(),
        ttl_seconds
    );

    Ok(format!(
        "Stream table {}.{} created successfully",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str()
    ))
}
