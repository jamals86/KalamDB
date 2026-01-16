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

/// Build a TableDefinition by validating inputs and constructing the definition
/// WITHOUT persisting or registering providers.
///
/// This is used in cluster mode where the Raft applier handles the actual
/// persistence and provider registration on ALL nodes (including the leader).
///
/// # Arguments
/// * `app_context` - Application context
/// * `stmt` - Parsed CREATE TABLE statement
/// * `user_id` - User ID from execution context
/// * `user_role` - User role from execution context
///
/// # Returns
/// Ok with TableDefinition, or error
pub fn build_table_definition(
    app_context: Arc<AppContext>,
    stmt: &CreateTableStatement,
    user_id: &UserId,
    user_role: Role,
) -> Result<kalamdb_commons::models::schemas::TableDefinition, KalamDbError> {
    use super::tables::validate_table_name;
    use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition, TableOptions};
    use kalamdb_commons::datatypes::{FromArrowType, KalamDataType};
    use kalamdb_commons::schemas::ColumnDefault;

    let table_id_str = format!(
        "{}.{}",
        stmt.namespace_id.as_str(),
        stmt.table_name.as_str()
    );

    log::debug!(
        "🔨 BUILD TABLE DEFINITION: {} (type: {:?}, user: {}, role: {:?})",
        table_id_str,
        stmt.table_type,
        user_id.as_str(),
        user_role
    );

    // Block CREATE on system namespaces
    super::guards::block_system_namespace_modification(
        &stmt.namespace_id,
        "CREATE",
        "TABLE",
        Some(stmt.table_name.as_str()),
    )?;

    // RBAC check
    if !crate::auth::rbac::can_create_table(user_role, stmt.table_type) {
        log::error!(
            "❌ CREATE TABLE {:?} {}: Insufficient privileges (user: {}, role: {:?})",
            stmt.table_type,
            table_id_str,
            user_id.as_str(),
            user_role
        );
        return Err(KalamDbError::Unauthorized(format!(
            "Insufficient privileges to create {:?} tables",
            stmt.table_type
        )));
    }

    // Validate table name
    validate_table_name(stmt.table_name.as_str()).map_err(KalamDbError::InvalidOperation)?;

    // Validate namespace exists
    let namespaces_provider = app_context.system_tables().namespaces();
    let namespace_id = NamespaceId::new(stmt.namespace_id.as_str());
    if namespaces_provider.get_namespace(&namespace_id)?.is_none() {
        log::error!(
            "❌ CREATE TABLE failed: Namespace '{}' does not exist",
            stmt.namespace_id.as_str()
        );
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace '{}' does not exist. Create it first with CREATE NAMESPACE {}",
            stmt.namespace_id.as_str(),
            stmt.namespace_id.as_str()
        )));
    }

    // Type-specific validation
    match stmt.table_type {
        TableType::User | TableType::Shared => {
            // PRIMARY KEY validation
            if stmt.primary_key_column.is_none() {
                log::error!(
                    "❌ CREATE TABLE {:?} {}: PRIMARY KEY is required",
                    stmt.table_type,
                    table_id_str
                );
                return Err(KalamDbError::InvalidOperation(format!(
                    "{:?} tables require a PRIMARY KEY column",
                    stmt.table_type
                )));
            }
        }
        TableType::Stream => {
            // TTL validation
            if stmt.ttl_seconds.is_none() {
                log::error!(
                    "❌ CREATE TABLE STREAM {}: TTL clause is required",
                    table_id_str
                );
                return Err(KalamDbError::InvalidOperation(
                    "STREAM tables require TTL clause (e.g., TTL 3600)".to_string(),
                ));
            }
        }
        TableType::System => {
            return Err(KalamDbError::InvalidOperation(
                "Cannot create SYSTEM tables via SQL".to_string(),
            ));
        }
    }

    // Check if table already exists
    let schema_registry = app_context.schema_registry();
    let table_id = TableId::from_strings(stmt.namespace_id.as_str(), stmt.table_name.as_str());
    let existing_def = schema_registry
        .get_table_if_exists(app_context.as_ref(), &table_id)
        .into_kalamdb_error("Failed to check table existence")?;

    if existing_def.is_some() {
        if stmt.if_not_exists {
            log::info!(
                "ℹ️  TABLE {} already exists (IF NOT EXISTS)",
                table_id_str
            );
            // Return the existing definition
            return Ok((*existing_def.unwrap()).clone());
        } else {
            log::warn!(
                "❌ CREATE TABLE failed: {} already exists",
                table_id_str
            );
            return Err(KalamDbError::AlreadyExists(format!(
                "Table {} already exists",
                table_id_str
            )));
        }
    }

    // Resolve storage
    let (storage_id, _storage_type) = resolve_storage_info(&app_context, stmt.storage_id.as_ref())?;

    // Build columns from Arrow schema
    let columns: Vec<ColumnDefinition> = stmt.schema
        .fields()
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            kalamdb_commons::validation::validate_column_name(field.name()).map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Invalid column name '{}': {}",
                    field.name(),
                    e
                ))
            })?;

            let kalam_type =
                KalamDataType::from_arrow_type(field.data_type()).unwrap_or(KalamDataType::Text);

            let is_pk = stmt
                .primary_key_column
                .as_ref()
                .map(|pk| pk == field.name())
                .unwrap_or(false);

            let default_val = stmt
                .column_defaults
                .get(field.name())
                .cloned()
                .unwrap_or(ColumnDefault::None);

            Ok(ColumnDefinition::new(
                (idx + 1) as u64,
                field.name().clone(),
                (idx + 1) as u32,
                kalam_type,
                field.is_nullable(),
                is_pk,
                false,
                default_val,
                None,
            ))
        })
        .collect::<Result<Vec<_>, KalamDbError>>()?;

    // Build table options
    let table_options = match stmt.table_type {
        TableType::User => TableOptions::user(),
        TableType::Shared => TableOptions::shared(),
        TableType::Stream => TableOptions::stream(stmt.ttl_seconds.unwrap_or(3600)),
        TableType::System => TableOptions::system(),
    };

    // Create TableDefinition
    let mut table_def = TableDefinition::new(
        stmt.namespace_id.clone(),
        stmt.table_name.clone(),
        stmt.table_type,
        columns,
        table_options.clone(),
        None,
    )
    .map_err(KalamDbError::SchemaError)?;

    // Apply table-level options from DDL
    match (&mut table_def.table_options, stmt.table_type) {
        (TableOptions::User(opts), TableType::User) => {
            opts.storage_id = storage_id.clone();
            opts.use_user_storage = stmt.use_user_storage;
            opts.flush_policy = stmt.flush_policy.clone();
        }
        (TableOptions::Shared(opts), TableType::Shared) => {
            opts.storage_id = storage_id.clone();
            // Only override access_level if explicitly specified in SQL; otherwise keep the default (Private)
            if let Some(access) = stmt.access_level {
                opts.access_level = Some(access);
            }
            opts.flush_policy = stmt.flush_policy.clone();
        }
        (TableOptions::Stream(opts), TableType::Stream) => {
            if let Some(ttl) = stmt.ttl_seconds {
                opts.ttl_seconds = ttl;
            }
        }
        _ => {}
    }

    // Inject system columns (_seq, _deleted)
    let sys_cols = app_context.system_columns_service();
    sys_cols.add_system_columns(&mut table_def)?;

    log::info!(
        "✅ Built TableDefinition for {} (type: {:?}, columns: {}, version: {})",
        table_id_str,
        stmt.table_type,
        table_def.columns.len(),
        table_def.schema_version
    );

    Ok(table_def)
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
    save_table_definition(app_context.as_ref(), stmt, schema)?;

    // Retrieve the saved table definition
    let table_def = schema_registry
        .get_table_if_exists(app_context.as_ref(), table_id)?
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
    use crate::schema_registry::PathResolver;
    let template = PathResolver::resolve_storage_path_template(
        app_context.as_ref(),
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
        .get_table_if_exists(app_context.as_ref(), &table_id)
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
                let arrow_schema = schema_registry.get_arrow_schema(app_context.as_ref(), &table_id)?;
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
    let provider_arrow_schema = schema_registry.get_arrow_schema(app_context.as_ref(), &table_id)?;
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
        .get_table_if_exists(app_context.as_ref(), &table_id)
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
                let arrow_schema = schema_registry.get_arrow_schema(app_context.as_ref(), &table_id)?;
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
    let provider_arrow_schema = schema_registry.get_arrow_schema(app_context.as_ref(), &table_id)?;
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
        .get_table_if_exists(app_context.as_ref(), &table_id)
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
                let arrow_schema = schema_registry.get_arrow_schema(app_context.as_ref(), &table_id)?;

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
    const MAX_STREAM_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;
    if ttl_seconds == 0 {
        log::error!(
            "❌ CREATE TABLE (TYPE='STREAM') failed: {}.{} - TTL must be greater than 0",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str()
        );
        return Err(KalamDbError::InvalidOperation(
            "STREAM table TTL must be greater than 0".to_string(),
        ));
    }

    if ttl_seconds > MAX_STREAM_TTL_SECONDS {
        log::error!(
            "❌ CREATE TABLE (TYPE='STREAM') failed: {}.{} - TTL exceeds 30 days ({}s)",
            stmt.namespace_id.as_str(),
            stmt.table_name.as_str(),
            ttl_seconds
        );
        return Err(KalamDbError::InvalidOperation(
            "STREAM table TTL must be 30 days or less".to_string(),
        ));
    }
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
