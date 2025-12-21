//! CREATE TABLE helper functions
//!
//! Provides unified logic for creating all table types (USER/SHARED/STREAM)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use kalamdb_commons::models::StorageType;
use kalamdb_commons::models::{NamespaceId, StorageId, TableAccess, TableId, UserId};
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::Role;
use kalamdb_sql::ddl::CreateTableStatement;
use std::sync::Arc;

// Import shared registration helpers
use super::table_registration::{
    register_shared_table_provider, register_stream_table_provider, register_user_table_provider,
};

/// Route CREATE TABLE to type-specific handler
///
/// # Arguments
/// * `app_context` - Application context
/// * `stmt` - Parsed CREATE TABLE statement
/// * `user_id` - User ID from execution context
/// * `user_role` - User role from execution context
///
/// # Returns
/// Ok with success message, or error
pub fn create_table(
    app_context: Arc<AppContext>,
    stmt: CreateTableStatement,
    user_id: &UserId,
    user_role: Role,
) -> Result<String, KalamDbError> {
    let table_id_str = format!(
        "{}.{}",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str()
    );
    let table_type = stmt.table_type;
    let user_id_str = user_id.as_str().to_string();

    log::info!(
        "🔨 CREATE TABLE request: {} (type: {:?}, user: {}, role: {:?})",
        table_id_str,
        table_type,
        user_id_str,
        user_role
    );

    // Block CREATE on system namespaces - they are managed internally
    super::guards::block_system_namespace_modification(
        &stmt.namespace_id,
        "CREATE",
        "TABLE",
        Some(stmt.table_name.as_str()),
    )?;

    // Route to type-specific handler
    let result = match stmt.table_type {
        TableType::User => create_user_table(app_context, stmt, user_id, user_role),
        TableType::Shared => create_shared_table(app_context, stmt, user_id, user_role),
        TableType::Stream => create_stream_table(app_context, stmt, user_id, user_role),
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

/// Persist table to system catalog and prime the schema cache
///
/// Common logic extracted from create_user/shared/stream_table functions.
/// Handles:
/// 1. Saving table definition to information_schema.tables
/// 2. Inserting into system.tables catalog
/// 3. Storing initial versioned schema
/// 4. Priming the schema cache with storage configuration
///
/// # Arguments
/// * `app_context` - Application context
/// * `stmt` - The CREATE TABLE statement
/// * `schema` - Arrow schema for the table
/// * `table_id` - Unique table identifier
/// * `table_type` - Type of table (User/Shared/Stream)
/// * `storage_id` - Storage location for the table
///
/// # Returns
/// The persisted TableDefinition wrapped in Arc
fn persist_table_and_prime_cache(
    app_context: &Arc<AppContext>,
    stmt: &CreateTableStatement,
    schema: &arrow::datatypes::SchemaRef,
    table_id: &TableId,
    table_type: TableType,
    storage_id: &StorageId,
) -> Result<std::sync::Arc<kalamdb_commons::models::schemas::TableDefinition>, KalamDbError> {
    use crate::schema_registry::CachedTableData;
    use super::tables::save_table_definition;

    let schema_registry = app_context.schema_registry();

    // Save complete table definition to information_schema.tables
    save_table_definition(stmt, schema)?;

    // Retrieve the saved table definition
    let table_def = schema_registry
        .get_table_definition(table_id)?
        .ok_or_else(|| {
            KalamDbError::Other(format!(
                "Failed to retrieve table definition for {} after save",
                table_id
            ))
        })?;

    // Insert into system.tables catalog
    let tables_provider = app_context.system_tables().tables();
    tables_provider
        .create_table(table_id, &table_def)
        .into_kalamdb_error("Failed to insert table into system catalog")?;

    // Store initial version (version 1) in versioned schema store
    tables_provider
        .put_versioned_schema(table_id, &table_def)
        .into_kalamdb_error("Failed to store initial schema version")?;

    // Prime cache entry with storage path template + storage id
    let template = schema_registry.resolve_storage_path_template(
        table_id,
        table_type,
        storage_id,
    )?;

    let data = CachedTableData::with_storage_config(
        std::sync::Arc::clone(&table_def),
        Some(storage_id.clone()),
        template.clone(),
    );
    schema_registry.insert(table_id.clone(), std::sync::Arc::new(data));
    
    log::debug!(
        "Primed cache for {:?} table {} with template: {}",
        table_type,
        table_id,
        template
    );

    Ok(table_def)
}

/// Create USER table (multi-tenant with automatic user_id filtering)
pub fn create_user_table(
    app_context: Arc<AppContext>,
    stmt: CreateTableStatement,
    user_id: &UserId,
    user_role: Role,
) -> Result<String, KalamDbError> {
    use super::tables::validate_table_name;

    // RBAC check
    if !crate::auth::rbac::can_create_table(user_role, TableType::User) {
        log::error!(
            "❌ CREATE TABLE (TYPE='USER') {}.{}: Insufficient privileges (user: {}, role: {:?})",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            user_id.as_str(),
            user_role
        );
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create USER tables".to_string(),
        ));
    }

    log::debug!(
        "📋 CREATE TABLE (TYPE='USER') {}.{}: columns={}, storage={:?}, ttl={:?}",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        stmt.schema.fields().len(),
        stmt.storage_id,
        stmt.ttl_seconds
    );

    // Validate table name
    validate_table_name(stmt.table_name.as_str()).map_err(KalamDbError::InvalidOperation)?;

    // Validate namespace exists BEFORE other validations to surface actionable guidance
    let namespaces_provider = app_context.system_tables().namespaces();
    let namespace_id = NamespaceId::new(stmt.namespace_id.as_str());
    if namespaces_provider.get_namespace(&namespace_id)?.is_none() {
        log::error!(
            "❌ CREATE TABLE (TYPE='USER') failed: Namespace '{}' does not exist. Create it first with CREATE NAMESPACE {}",
            stmt.namespace_id.as_str(),
            stmt.namespace_id.as_str()
        );
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace '{}' does not exist. Create it first with CREATE NAMESPACE {}",
            stmt.namespace_id.as_str(),
            stmt.namespace_id.as_str()
        )));
    }

    // PRIMARY KEY validation - USER tables MUST have PRIMARY KEY
    if stmt.primary_key_column.is_none() {
        log::error!(
            "❌ CREATE TABLE (TYPE='USER') {}.{}: PRIMARY KEY is required",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str()
        );
        return Err(KalamDbError::InvalidOperation(
            "USER tables require a PRIMARY KEY column".to_string(),
        ));
    }

    // Check if table already exists (single optimized lookup)
    let schema_registry = app_context.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
    let existing_def = schema_registry
        .get_table_if_exists(&table_id)
        .into_kalamdb_error("Failed to check table existence")?;

    if existing_def.is_some() {
        if stmt.if_not_exists {
            // Ensure provider is registered even if table exists
            // This handles cases where table exists in storage but provider is missing from cache
            // (e.g. after restart or in test environments with shared storage)
            if schema_registry.get_provider(&table_id).is_none() {
                log::info!(
                    "Table {}.{} exists but provider missing - registering now",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                );

                // Use cached Arrow schema (memoized in CachedTableData) instead of to_arrow_schema()
                let arrow_schema = schema_registry.get_arrow_schema(&table_id)?;
                register_user_table_provider(&app_context, &table_id, arrow_schema)?;
            }

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
                "❌ CREATE TABLE (TYPE='USER') failed: {}.{} already exists",
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

    // Resolve storage and ensure it exists
    let (storage_id, _storage_type) = resolve_storage_info(&app_context, stmt.storage_id.as_ref())?;

    log::debug!(
        "✓ Validation passed for USER TABLE {}.{} (storage: {})",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str()
    );

    // Use schema as-is (PRIMARY KEY already validated)
    let schema = stmt.schema.clone();

    // Persist table definition, insert into system catalog, and prime cache
    persist_table_and_prime_cache(
        &app_context,
        &stmt,
        &schema,
        &table_id,
        TableType::User,
        &storage_id,
    )?;

    // Register UserTableProvider for INSERT/UPDATE/DELETE/SELECT operations
    // Use cached Arrow schema from SchemaRegistry (memoized in CachedTableData)
    let provider_arrow_schema = schema_registry.get_arrow_schema(&table_id)?;
    register_user_table_provider(&app_context, &table_id, provider_arrow_schema)?;

    // Log detailed success with table options
    log::info!(
        "✅ USER TABLE created: {}.{} | storage: {} | columns: {} | pk: {} | system_columns: [_seq, _deleted]",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str(),
        schema.fields().len(),
        stmt.primary_key_column.as_ref().unwrap()
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
    mut stmt: CreateTableStatement,
    user_id: &UserId,
    user_role: Role,
) -> Result<String, KalamDbError> {
    use super::tables::validate_table_name;

    // Set default access level if not provided
    if stmt.access_level.is_none() {
        stmt.access_level = Some(TableAccess::Private);
    }

    // RBAC check
    if !crate::auth::rbac::can_create_table(user_role, TableType::Shared) {
        log::error!(
            "❌ CREATE TABLE (TYPE='SHARED') {}.{}: Insufficient privileges (user: {}, role: {:?})",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            user_id.as_str(),
            user_role
        );
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create SHARED tables".to_string(),
        ));
    }

    log::debug!(
        "📋 CREATE TABLE (TYPE='SHARED') {}.{}: columns={}, storage={:?}, access_level={:?}",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        stmt.schema.fields().len(),
        stmt.storage_id,
        stmt.access_level
    );

    // Validate table name
    validate_table_name(stmt.table_name.as_str()).map_err(KalamDbError::InvalidOperation)?;

    // Validate namespace exists BEFORE other validations to surface actionable guidance
    let namespaces_provider = app_context.system_tables().namespaces();
    let namespace_id = NamespaceId::new(stmt.namespace_id.as_str());
    if namespaces_provider.get_namespace(&namespace_id)?.is_none() {
        log::error!(
            "❌ CREATE TABLE (TYPE='SHARED') failed: Namespace '{}' does not exist. Create it first with CREATE NAMESPACE {}",
            stmt.namespace_id.as_str(),
            stmt.namespace_id.as_str()
        );
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace '{}' does not exist. Create it first with CREATE NAMESPACE {}",
            stmt.namespace_id.as_str(),
            stmt.namespace_id.as_str()
        )));
    }

    // PRIMARY KEY validation - SHARED tables MUST have PRIMARY KEY
    if stmt.primary_key_column.is_none() {
        log::error!(
            "❌ CREATE TABLE (TYPE='SHARED') {}.{}: PRIMARY KEY is required",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str()
        );
        return Err(KalamDbError::InvalidOperation(
            "SHARED tables require a PRIMARY KEY column".to_string(),
        ));
    }

    // Check if table already exists (single optimized lookup)
    let schema_registry = app_context.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
    let existing_def = schema_registry
        .get_table_if_exists(&table_id)
        .into_kalamdb_error("Failed to check table existence")?;

    if existing_def.is_some() {
        if stmt.if_not_exists {
            // Ensure provider is registered even if table exists
            if schema_registry.get_provider(&table_id).is_none() {
                log::info!(
                    "Table {}.{} exists but provider missing - registering now",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                );

                // Use cached Arrow schema (memoized in CachedTableData) instead of to_arrow_schema()
                let arrow_schema = schema_registry.get_arrow_schema(&table_id)?;
                register_shared_table_provider(&app_context, &table_id, arrow_schema)?;
            }

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
                "❌ CREATE TABLE (TYPE='SHARED') failed: {}.{} already exists",
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

    // (namespace existence validated above)

    // Resolve storage and ensure it exists
    let (storage_id, _storage_type) = resolve_storage_info(&app_context, stmt.storage_id.as_ref())?;

    log::debug!(
        "✓ Validation passed for SHARED TABLE {}.{} (storage: {})",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str()
    );

    // Use schema as-is (PRIMARY KEY already validated)
    let schema = stmt.schema.clone();

    // Persist table definition, insert into system catalog, and prime cache
    persist_table_and_prime_cache(
        &app_context,
        &stmt,
        &schema,
        &table_id,
        TableType::Shared,
        &storage_id,
    )?;

    // Register SharedTableProvider for CRUD/query access
    let provider_arrow_schema = schema_registry.get_arrow_schema(&table_id)?;
    register_shared_table_provider(&app_context, &table_id, provider_arrow_schema)?;

    // Log detailed success with table options
    log::info!(
        "✅ SHARED TABLE created: {}.{} | storage: {} | columns: {} | pk: {} | access_level: {:?} | system_columns: [_seq, _deleted]",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        storage_id.as_str(),
        schema.fields().len(),
        stmt.primary_key_column.as_ref().unwrap(),
        stmt.access_level
    );

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

    let storage_type = storage.storage_type;
    Ok((storage_id, storage_type))
}

/// Create STREAM table (TTL-based ephemeral table)
pub fn create_stream_table(
    app_context: Arc<AppContext>,
    stmt: CreateTableStatement,
    user_id: &UserId,
    user_role: Role,
) -> Result<String, KalamDbError> {
    use super::tables::validate_table_name;

    // RBAC check
    if !crate::auth::rbac::can_create_table(user_role, TableType::Stream) {
        log::error!(
            "❌ CREATE TABLE (TYPE='STREAM') {}.{}: Insufficient privileges (user: {}, role: {:?})",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            user_id.as_str(),
            user_role
        );
        return Err(KalamDbError::Unauthorized(
            "Insufficient privileges to create STREAM tables".to_string(),
        ));
    }

    log::debug!(
        "📋 CREATE TABLE (TYPE='STREAM') {}.{}: columns={}, ttl={:?}s",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str(),
        stmt.schema.fields().len(),
        stmt.ttl_seconds
    );

    // Validate table name
    validate_table_name(stmt.table_name.as_str()).map_err(KalamDbError::InvalidOperation)?;

    // Check if table already exists (single optimized lookup)
    let schema_registry = app_context.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
    let existing_def = schema_registry
        .get_table_if_exists(&table_id)
        .into_kalamdb_error("Failed to check table existence")?;

    if let Some(def) = existing_def {
        if stmt.if_not_exists {
            // Ensure provider is registered even if table exists
            if schema_registry.get_provider(&table_id).is_none() {
                log::info!(
                    "Table {}.{} exists but provider missing - registering now",
                    stmt.namespace_id.as_str(),
                    stmt.table_name.as_str()
                );

                // Use cached Arrow schema (memoized in CachedTableData) instead of to_arrow_schema()
                let arrow_schema = schema_registry.get_arrow_schema(&table_id)?;

                // Extract TTL from table options if available, otherwise use default
                let ttl_seconds = if let kalamdb_commons::schemas::TableOptions::Stream(opts) =
                    &def.table_options
                {
                    Some(opts.ttl_seconds)
                } else {
                    stmt.ttl_seconds
                };

                register_stream_table_provider(
                    &app_context,
                    &table_id,
                    arrow_schema,
                    ttl_seconds,
                )?;
            }

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
                "❌ CREATE TABLE (TYPE='STREAM') failed: {}.{} already exists",
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

    // Validate namespace exists BEFORE other validations to surface actionable guidance
    let namespaces_provider = app_context.system_tables().namespaces();
    let namespace_id = NamespaceId::new(stmt.namespace_id.as_str());
    if namespaces_provider.get_namespace(&namespace_id)?.is_none() {
        log::error!(
            "❌ CREATE TABLE (TYPE='STREAM') failed: Namespace '{}' does not exist. Create it first with CREATE NAMESPACE {}",
            stmt.namespace_id.as_str(),
            stmt.namespace_id.as_str()
        );
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace '{}' does not exist. Create it first with CREATE NAMESPACE {}",
            stmt.namespace_id.as_str(),
            stmt.namespace_id.as_str()
        )));
    }

    // Stream tables use schema as-is (NO auto-increment, NO system columns)
    let schema = stmt.schema.clone();

    // Validate TTL is specified
    if stmt.ttl_seconds.is_none() {
        log::error!(
            "❌ CREATE TABLE (TYPE='STREAM') failed: {}.{} - TTL clause is required",
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

    // Stream tables use default storage
    let storage_id = StorageId::from("local");

    // Persist table definition, insert into system catalog, and prime cache
    persist_table_and_prime_cache(
        &app_context,
        &stmt,
        &schema,
        &table_id,
        TableType::Stream,
        &storage_id,
    )?;

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
