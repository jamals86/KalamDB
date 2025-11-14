//! CREATE TABLE helper functions
//!
//! Provides unified logic for creating all table types (USER/SHARED/STREAM)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::models::ExecutionContext;
use kalamdb_commons::models::{NamespaceId, StorageId, TableId, UserId};
use kalamdb_commons::models::StorageType;
use kalamdb_commons::schemas::TableType;
use kalamdb_sql::ddl::CreateTableStatement;
use std::sync::Arc;

// Import shared registration helpers
use super::table_registration::{
    register_user_table_provider,
    register_shared_table_provider,
    register_stream_table_provider,
};

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

    // Log success here; detailed error logging is handled in type-specific helpers
    if let Ok(msg) = &result {
        log::info!("✅ CREATE TABLE succeeded: {}", msg);
    }

    result
}

/// Create USER table (multi-tenant with automatic user_id filtering)
pub fn create_user_table(
    app_context: Arc<AppContext>,
    stmt: CreateTableStatement,
    exec_ctx: &ExecutionContext,
) -> Result<String, KalamDbError> {
    use super::tables::{inject_auto_increment_field, save_table_definition, validate_table_name};
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

    // T051: Validate PRIMARY KEY is specified (MVCC requirement)
    if stmt.primary_key_column.is_none() {
        log::error!(
            "❌ CREATE USER TABLE failed: {}.{} - PRIMARY KEY column is required",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str()
        );
        return Err(KalamDbError::InvalidOperation(
            "CREATE TABLE requires a PRIMARY KEY column (MVCC architecture requirement)".to_string(),
        ));
    }

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
            log::warn!(
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

    // Resolve storage and ensure it exists
    let (storage_id, storage_type) = resolve_storage_info(&app_context, stmt.storage_id.as_ref())?;

    log::debug!(
        "✓ Validation passed for USER TABLE {}.{} (storage: {})",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str()
    );

    // Auto-increment field injection ONLY if no explicit PRIMARY KEY
    // T060: When user specifies PRIMARY KEY, don't add auto-increment 'id' column
    let schema = if stmt.primary_key_column.is_some() {
        // User specified explicit PK - use schema as-is
        stmt.schema.clone()
    } else {
        // No explicit PK - add auto-increment 'id' column for backward compatibility
        inject_auto_increment_field(stmt.schema.clone())?
    };

    // REMOVED: inject_system_columns() call
    // System columns (_id, _updated, _deleted) are now added by SystemColumnsService
    // in save_table_definition() after TableDefinition creation (Phase 12, US5)
    
    // Inject DEFAULT SNOWFLAKE_ID() for auto-injected id column
    let mut modified_stmt = stmt.clone();
    if !modified_stmt.column_defaults.contains_key("id") {
        modified_stmt.column_defaults.insert(
            "id".to_string(),
            ColumnDefault::function("SNOWFLAKE_ID", vec![]),
        );
    }

    // Save complete table definition to information_schema.tables (produces full Arrow schema with system columns)
    save_table_definition(&modified_stmt, &schema)?;

    // Insert into system.tables
    let table_def = schema_registry
        .get_table_definition(&table_id)?
        .ok_or_else(|| KalamDbError::Other(format!("Failed to retrieve table definition for {}.{} after save", stmt.namespace_id.as_str(), stmt.table_name.as_str())))?;

    let tables_provider = app_context.system_tables().tables();
    tables_provider.create_table(&table_id, &table_def).map_err(|e| {
        KalamDbError::Other(format!("Failed to insert table into system catalog: {}", e))
    })?;

    // Prime cache entry with storage path template + storage id (needed for flush path resolution)
    {
        use crate::schema_registry::CachedTableData;
        use kalamdb_commons::schemas::TableType;
        let template = schema_registry
            .resolve_storage_path_template(&table_id.namespace_id(), &table_id.table_name(), TableType::User, &storage_id)?;
        let mut data = CachedTableData::new(Arc::clone(&table_def));
        data.storage_id = Some(storage_id.clone());
        data.storage_path_template = template.clone();
        schema_registry.insert(table_id.clone(), Arc::new(data));
        log::debug!(
            "Primed cache for user table {}.{} with template: {}",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            template
        );
    }

    // Register UserTableProvider for INSERT/UPDATE/DELETE/SELECT operations
    // Use authoritative Arrow schema rebuilt from TableDefinition (includes system columns)
    let provider_arrow_schema = table_def
        .to_arrow_schema()
        .map_err(|e| KalamDbError::SchemaError(format!("Failed to build provider Arrow schema: {}", e)))?;
    register_user_table_provider(&app_context, &table_id, provider_arrow_schema)?;

    // Ensure filesystem table directories exist for the creator
    if storage_type == StorageType::Filesystem {
        ensure_table_directory_exists(&schema_registry, &table_id, Some(exec_ctx.user_id()))?;
    }

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
    use super::tables::{inject_auto_increment_field, save_table_definition, validate_table_name};
    use kalamdb_commons::schemas::ColumnDefault;

    // T051: Validate PRIMARY KEY is specified (MVCC requirement)
    if stmt.primary_key_column.is_none() {
        log::error!(
            "❌ CREATE SHARED TABLE failed: {}.{} - PRIMARY KEY column is required",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str()
        );
        return Err(KalamDbError::InvalidOperation(
            "CREATE TABLE requires a PRIMARY KEY column (MVCC architecture requirement)".to_string(),
        ));
    }

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
            log::warn!(
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

    // Resolve storage and ensure it exists
    let (storage_id, storage_type) = resolve_storage_info(&app_context, stmt.storage_id.as_ref())?;

    log::debug!(
        "✓ Validation passed for SHARED TABLE {}.{} (storage: {})",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str()
    );

    // Auto-increment field injection ONLY if no explicit PRIMARY KEY
    // T060: When user specifies PRIMARY KEY, don't add auto-increment 'id' column
    let schema = if stmt.primary_key_column.is_some() {
        // User specified explicit PK - use schema as-is
        stmt.schema.clone()
    } else {
        // No explicit PK - add auto-increment 'id' column for backward compatibility
        inject_auto_increment_field(stmt.schema.clone())?
    };

    // REMOVED: inject_system_columns() call
    // System columns (_id, _updated, _deleted) are now added by SystemColumnsService
    // in save_table_definition() after TableDefinition creation (Phase 12, US5)
    
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

    // Register SharedTableProvider for CRUD/query access
    register_shared_table_provider(&app_context, &table_id, schema.clone())?;

    // Prime cache entry with storage path template + storage id (needed for flush path resolution)
    {
        use crate::schema_registry::CachedTableData;
        let template = schema_registry
            .resolve_storage_path_template(&table_id.namespace_id(), &table_id.table_name(), TableType::Shared, &storage_id)?;
        let mut data = CachedTableData::new(Arc::clone(&table_def));
        data.storage_id = Some(storage_id.clone());
        data.storage_path_template = template.clone();
        schema_registry.insert(table_id.clone(), Arc::new(data));
        log::debug!(
            "Primed cache for shared table {}.{} with template: {}",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            template
        );
    }

    // Log detailed success with table options
    log::info!(
        "✅ SHARED TABLE created: {}.{} | storage: {} | columns: {} | access_level: {:?} | auto_increment: true | system_columns: [_updated, _deleted]",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str(),
        schema.fields().len(),
        stmt.access_level
    );

    if storage_type == StorageType::Filesystem {
        ensure_table_directory_exists(&schema_registry, &table_id, None)?;
    }

    Ok(format!(
        "Shared table {}.{} created successfully",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str()
    ))
}

fn resolve_storage_info(
    app_context: &Arc<AppContext>,
    requested: Option<&StorageId>,
) -> Result<(StorageId, StorageType), KalamDbError> {
    let storages_provider = app_context.system_tables().storages();
    let storage_id = requested
        .cloned()
        .unwrap_or_else(|| StorageId::from("local"));

    let storage = storages_provider
        .get_storage_by_id(&storage_id)?
        .ok_or_else(|| {
            log::error!(
                "❌ CREATE TABLE failed: Storage '{}' does not exist",
                storage_id.as_str()
            );
            KalamDbError::InvalidOperation(format!(
                "Storage '{}' does not exist",
                storage_id.as_str()
            ))
        })?;

    let storage_type = StorageType::from(storage.storage_type.as_str());
    Ok((storage_id, storage_type))
}

fn ensure_table_directory_exists(
    schema_registry: &Arc<crate::schema_registry::SchemaRegistry>,
    table_id: &TableId,
    user_id: Option<&UserId>,
) -> Result<(), KalamDbError> {
    let path = schema_registry.get_storage_path(table_id, user_id, None)?;
    if path.trim().is_empty() {
        return Ok(());
    }

    let dir = std::path::Path::new(&path);
    std::fs::create_dir_all(dir).map_err(|e| {
        KalamDbError::IoError(format!(
            "Failed to create table directory '{}': {}",
            dir.display(),
            e
        ))
    })
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
            log::warn!(
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

    // Register StreamTableProvider for event operations (distinct from SHARED)
    register_stream_table_provider(&app_context, &table_id, schema.clone(), Some(ttl_seconds))?;

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
