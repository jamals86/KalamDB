//! Table provider registration helpers (shared between CREATE TABLE and load_existing_tables)
//!
//! Consolidates provider creation/registration logic to eliminate duplication

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::TableType;
use arrow::datatypes::SchemaRef;
use datafusion::datasource::TableProvider;
use kalamdb_commons::models::TableId;
use kalamdb_sharding::ShardRouter;
use std::sync::Arc;

/// Register USER table provider with SchemaRegistry
///
/// Creates UserTableShared + UserTableProvider and registers in provider cache.
/// SchemaRegistry automatically registers with DataFusion's catalog.
///
/// # Arguments
/// * `app_context` - Application context
/// * `table_id` - Table identifier
/// * `arrow_schema` - Arrow schema for the table
///
/// # Returns
/// Ok on success, error if registration fails
pub fn register_user_table_provider(
    app_context: &Arc<AppContext>,
    table_id: &TableId,
    _arrow_schema: SchemaRef,
) -> Result<(), KalamDbError> {
    use crate::providers::{TableProviderCore, UserTableProvider};
    use kalamdb_tables::new_indexed_user_table_store;

    log::debug!(
        "📋 Registering USER table provider: {}",
        table_id
    );

    // Determine primary key field name from TableDefinition first
    // (needed for creating indexed store with PK index)
    let table_def = app_context
        .schema_registry()
        .get_table_if_exists(app_context.as_ref(), table_id)?
        .ok_or_else(|| {
            KalamDbError::InvalidOperation(format!(
                "Table definition not found for {}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            ))
        })?;

    let pk_field = table_def
        .columns
        .iter()
        .find(|c| c.is_primary_key)
        .map(|c| c.column_name.clone())
        .unwrap_or_else(|| "id".to_string());

    // Create indexed user table store with PK index (partition is automatically created)
    let user_table_store = Arc::new(new_indexed_user_table_store(
        app_context.storage_backend(),
        table_id,
        &pk_field,
    ));

    log::debug!(
        "Created indexed user table store for {} with pk_field='{}'",
        table_id,
        pk_field
    );

    // Create TableProviderCore and provider (wire LiveQueryManager for notifications)
    let core = Arc::new(
        TableProviderCore::from_app_context(app_context, table_id.clone(), TableType::User)
            .with_live_query_manager(app_context.live_query_manager()),
    );

    let provider = UserTableProvider::try_new(core, user_table_store, pk_field)?;
    let provider_arc: Arc<dyn TableProvider> = Arc::new(provider);

    app_context
        .schema_registry()
        .insert_provider(app_context.as_ref(), table_id.clone(), provider_arc)?;

    log::debug!(
        "✅ USER table provider registered: {}",
        table_id
    );

    Ok(())
}

/// Register SHARED table provider with SchemaRegistry
///
/// Creates SharedTableProvider and ensures RocksDB partition exists.
/// SchemaRegistry automatically registers with DataFusion's catalog.
///
/// # Arguments
/// * `app_context` - Application context
/// * `table_id` - Table identifier
/// * `arrow_schema` - Arrow schema for the table
///
/// # Returns
/// Ok on success, error if registration fails
pub fn register_shared_table_provider(
    app_context: &Arc<AppContext>,
    table_id: &TableId,
    _arrow_schema: SchemaRef,
) -> Result<(), KalamDbError> {
    use crate::providers::{SharedTableProvider, TableProviderCore};
    use kalamdb_tables::new_indexed_shared_table_store;

    log::debug!(
        "📋 Registering SHARED table provider: {}",
        table_id
    );

    // Determine primary key field name first (needed for indexed store)
    let table_def = app_context
        .schema_registry()
        .get_table_if_exists(app_context.as_ref(), table_id)?
        .ok_or_else(|| {
            KalamDbError::InvalidOperation(format!(
                "Table definition not found for {}",
                table_id
            ))
        })?;
    let pk_field = table_def
        .columns
        .iter()
        .find(|c| c.is_primary_key)
        .map(|c| c.column_name.clone())
        .unwrap_or_else(|| "id".to_string());

    // Create indexed shared table store with PK index for efficient lookups
    let shared_store = Arc::new(new_indexed_shared_table_store(
        app_context.storage_backend(),
        table_id,
        &pk_field,
    ));

    log::debug!(
        "Created indexed shared table store for {} with pk_field='{}'",
        table_id,
        pk_field
    );

    // Create and register new providers::SharedTableProvider
    let core = Arc::new(
        TableProviderCore::from_app_context(app_context, table_id.clone(), TableType::Shared)
            .with_live_query_manager(app_context.live_query_manager()),
    );

    let provider = SharedTableProvider::new(core, shared_store, pk_field);

    app_context
        .schema_registry()
        .insert_provider(app_context.as_ref(), table_id.clone(), Arc::new(provider))?;

    log::debug!(
        "✅ SHARED table provider registered: {}",
        table_id
    );

    Ok(())
}

/// Register STREAM table provider with SchemaRegistry
///
/// Creates StreamTableProvider with TTL settings.
/// SchemaRegistry automatically registers with DataFusion's catalog.
///
/// # Arguments
/// * `app_context` - Application context
/// * `table_id` - Table identifier
/// * `arrow_schema` - Arrow schema for the table (unused but kept for consistency)
/// * `ttl_seconds` - TTL in seconds (None = no TTL)
///
/// # Returns
/// Ok on success, error if registration fails
pub fn register_stream_table_provider(
    app_context: &Arc<AppContext>,
    table_id: &TableId,
    _arrow_schema: SchemaRef, // Unused but kept for API consistency
    ttl_seconds: Option<u64>,
) -> Result<(), KalamDbError> {
    use crate::providers::{StreamTableProvider, TableProviderCore};
    use kalamdb_tables::{new_stream_table_store, StreamTableStoreConfig};

    log::debug!(
        "📋 Registering STREAM table provider: {} (TTL: {:?}s)",
        table_id,
        ttl_seconds
    );

    // Create stream table store
    let streams_root = app_context.config().storage.streams_dir();
    let base_dir = streams_root
        .join(table_id.namespace_id().as_str())
        .join(table_id.table_name().as_str());
    let stream_store = Arc::new(new_stream_table_store(
        table_id,
        StreamTableStoreConfig {
            base_dir,
            shard_router: ShardRouter::default_config(),
            ttl_seconds,
        },
    ));

    log::debug!(
        "Created stream table store for {}",
        table_id
    );

    // Create and register provider (new providers::streams implementation)
    let core = Arc::new(
        TableProviderCore::from_app_context(app_context, table_id.clone(), TableType::Stream)
            .with_live_query_manager(app_context.live_query_manager()),
    );
    // Determine primary key field name from TableDefinition
    let table_def = app_context
        .schema_registry()
        .get_table_if_exists(app_context.as_ref(), table_id)?
        .ok_or_else(|| {
            KalamDbError::InvalidOperation(format!(
                "Table definition not found for {}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            ))
        })?;

    let pk_field = table_def
        .columns
        .iter()
        .find(|c| c.is_primary_key)
        .map(|c| c.column_name.clone())
        .unwrap_or_else(|| "id".to_string());

    let provider = StreamTableProvider::new(core, stream_store, ttl_seconds, pk_field);

    app_context
        .schema_registry()
        .insert_provider(app_context.as_ref(), table_id.clone(), Arc::new(provider))?;

    log::debug!(
        "✅ STREAM table provider registered: {} (TTL: {:?}s)",
        table_id,
        ttl_seconds
    );

    Ok(())
}

/// Unregister table provider from SchemaRegistry
///
/// Removes provider from cache and unregisters from DataFusion's catalog.
/// Used during DROP TABLE operations.
///
/// # Arguments
/// * `app_context` - Application context
/// * `table_id` - Table identifier
///
/// # Returns
/// Ok on success, error if unregistration fails
pub fn unregister_table_provider(
    app_context: &Arc<AppContext>,
    table_id: &TableId,
) -> Result<(), KalamDbError> {
    log::debug!(
        "📋 Unregistering table provider: {}",
        table_id
    );

    // Remove from SchemaRegistry (automatically unregisters from DataFusion)
    app_context.schema_registry().remove_provider(table_id)?;

    log::debug!(
        "✅ Table provider unregistered: {}",
        table_id
    );

    Ok(())
}
