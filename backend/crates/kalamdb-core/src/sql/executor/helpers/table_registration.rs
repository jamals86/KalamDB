//! Table provider registration helpers (shared between CREATE TABLE and load_existing_tables)
//!
//! Consolidates provider creation/registration logic to eliminate duplication

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use arrow::datatypes::SchemaRef;
use datafusion::datasource::TableProvider;
use kalamdb_commons::models::TableId;
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
    arrow_schema: SchemaRef,
) -> Result<(), KalamDbError> {
    use crate::tables::base_table_provider::UserTableShared;
    use crate::tables::user_tables::{new_user_table_store, UserTableProvider};

    log::debug!(
        "📋 Registering USER table provider: {}.{}",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
    );

    // Create user table store
    let user_table_store = Arc::new(new_user_table_store(
        app_context.storage_backend(),
        table_id.namespace_id(),
        table_id.table_name(),
    ));

    // Create shared state (returns Arc<UserTableShared>)
    let mut shared = UserTableShared::new(
        Arc::new(table_id.clone()),
        app_context.schema_registry(),
        arrow_schema.clone(),
        user_table_store,
    );

    // Attach LiveQueryManager for INSERT/UPDATE/DELETE notifications
    if let Some(shared_ref) = Arc::get_mut(&mut shared) {
        shared_ref.attach_live_query_manager(app_context.live_query_manager());
    } else {
        log::warn!(
            "Could not attach LiveQueryManager to UserTableShared at registration time for {}.{}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );
    }

    // Create UserTableProvider and register
    let provider = UserTableProvider::new(shared);
    let provider_arc: Arc<dyn TableProvider> = Arc::new(provider);
    
    app_context
        .schema_registry()
        .insert_provider(table_id.clone(), provider_arc)?;

    log::debug!(
        "✅ USER table provider registered: {}.{}",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
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
    arrow_schema: SchemaRef,
) -> Result<(), KalamDbError> {
    use crate::tables::shared_tables::{shared_table_store::new_shared_table_store, SharedTableProvider};
    use crate::tables::system::system_table_store::SharedTableStoreExt;

    log::debug!(
        "📋 Registering SHARED table provider: {}.{}",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
    );

    // Create shared table store
    let shared_store = Arc::new(new_shared_table_store(
        app_context.storage_backend(),
        table_id.namespace_id(),
        table_id.table_name(),
    ));

    // Ensure RocksDB partition exists
    let _ = shared_store.create_column_family(
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str(),
    );

    // Create and register provider
    let provider = SharedTableProvider::new(
        Arc::new(table_id.clone()),
        app_context.schema_registry(),
        arrow_schema,
        shared_store,
    );

    app_context
        .schema_registry()
        .insert_provider(table_id.clone(), Arc::new(provider))?;

    log::debug!(
        "✅ SHARED table provider registered: {}.{}",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
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
    use crate::tables::stream_tables::{stream_table_store::new_stream_table_store, StreamTableProvider};

    log::debug!(
        "📋 Registering STREAM table provider: {}.{} (TTL: {:?}s)",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str(),
        ttl_seconds
    );

    // Create stream table store
    let stream_store = Arc::new(new_stream_table_store(
        table_id.namespace_id(),
        table_id.table_name(),
    ));

    // Create and register provider
    let provider = StreamTableProvider::new(
        Arc::new(table_id.clone()),
        app_context.schema_registry(),
        stream_store,
        ttl_seconds.map(|t| t as u32),
        false, // ephemeral default
        None,  // max_buffer default
    );

    app_context
        .schema_registry()
        .insert_provider(table_id.clone(), Arc::new(provider))?;

    log::debug!(
        "✅ STREAM table provider registered: {}.{} (TTL: {:?}s)",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str(),
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
        "📋 Unregistering table provider: {}.{}",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
    );

    // Remove from SchemaRegistry (automatically unregisters from DataFusion)
    app_context
        .schema_registry()
        .remove_provider(table_id)?;

    log::debug!(
        "✅ Table provider unregistered: {}.{}",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
    );

    Ok(())
}
