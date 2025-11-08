//! CREATE TABLE helper functions
//!
//! Provides unified logic for creating all table types (USER/SHARED/STREAM)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::models::ExecutionContext;
use kalamdb_commons::models::{NamespaceId, StorageId, TableId};
use kalamdb_commons::schemas::{TableDefinition, TableType};
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
    // Route to type-specific handler
    match stmt.table_type {
        TableType::User => create_user_table(app_context, stmt, exec_ctx),
        TableType::Shared => create_shared_table(app_context, stmt, exec_ctx),
        TableType::Stream => create_stream_table(app_context, stmt, exec_ctx),
        TableType::System => Err(KalamDbError::InvalidOperation(
            "Cannot create SYSTEM tables via SQL".to_string(),
        )),
    }
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
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create USER tables".to_string(),
        ));
    }

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
            return Ok(format!(
                "Table {}.{} already exists (IF NOT EXISTS)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ));
        } else {
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
            return Err(KalamDbError::InvalidOperation(format!(
                "Storage '{}' does not exist",
                sid
            )));
        }
        storage_id
    } else {
        StorageId::from("local") // Default storage
    };

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

    log::info!(
        "User table {}.{} created successfully (storage: {})",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str()
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
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create SHARED tables".to_string(),
        ));
    }

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
            return Ok(format!(
                "Table {}.{} already exists (IF NOT EXISTS)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ));
        } else {
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
            return Err(KalamDbError::InvalidOperation(format!(
                "Storage '{}' does not exist",
                sid
            )));
        }
        storage_id
    } else {
        StorageId::from("local") // Default storage
    };

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

    log::info!(
        "Shared table {}.{} created successfully (storage: {})",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str()
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
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create STREAM tables".to_string(),
        ));
    }

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
            return Ok(format!(
                "Stream table {}.{} already exists (IF NOT EXISTS)",
                stmt.namespace_id.as_str(),
                stmt.table_name.as_str()
            ));
        } else {
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
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace '{}' does not exist",
            stmt.namespace_id.as_str()
        )));
    }

    // Stream tables use schema as-is (NO auto-increment, NO system columns)
    let schema = stmt.schema.clone();

    // Validate TTL is specified
    if stmt.ttl_seconds.is_none() {
        return Err(KalamDbError::InvalidOperation(
            "STREAM tables require TTL clause (e.g., TTL 3600)".to_string(),
        ));
    }

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

    log::info!(
        "Stream table {}.{} created successfully (TTL: {:?}s)",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        stmt.ttl_seconds
    );

    Ok(format!(
        "Stream table {}.{} created successfully",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str()
    ))
}
