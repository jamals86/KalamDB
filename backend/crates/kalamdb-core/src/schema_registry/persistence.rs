use crate::error::KalamDbError;
use crate::schema_registry::cached_table_data::CachedTableData;
use crate::schema_registry::path_resolver::PathResolver;
use crate::schema_registry::table_cache::TableCache;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::TableId;
use std::sync::Arc;

/// Persistence operations for schema registry
pub struct SchemaPersistence;

impl SchemaPersistence {
    /// Get table definition from persistence layer (read-through pattern)
    pub fn get_table_definition(
        cache: &TableCache,
        table_id: &TableId,
    ) -> Result<Option<Arc<TableDefinition>>, KalamDbError> {
        // Fast path: check cache
        if let Some(cached) = cache.get(table_id) {
            return Ok(Some(Arc::clone(&cached.table)));
        }

        // Slow path: query persistence layer via AppContext
        let app_ctx = crate::app_context::AppContext::get();
        let tables_provider = app_ctx.system_tables().tables();

        match tables_provider.get_table_by_id(table_id)? {
            Some(table_def) => Ok(Some(Arc::new(table_def))),
            None => Ok(None),
        }
    }

    /// Store table definition to persistence layer (write-through pattern)
    pub fn put_table_definition(
        cache: &TableCache,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), KalamDbError> {
        // Get tables provider via AppContext
        let app_ctx = crate::app_context::AppContext::get();
        let tables_provider = app_ctx.system_tables().tables();

        // Persist to storage
        tables_provider.create_table(table_id, table_def)?;

        // Invalidate cache to force reload on next access
        cache.invalidate(table_id);

        Ok(())
    }

    /// Delete table definition from persistence layer (delete-through pattern)
    pub fn delete_table_definition(
        cache: &TableCache,
        table_id: &TableId,
    ) -> Result<(), KalamDbError> {
        // Get tables provider via AppContext
        let app_ctx = crate::app_context::AppContext::get();
        let tables_provider = app_ctx.system_tables().tables();

        // Delete from storage
        tables_provider.delete_table(table_id)?;

        // Invalidate cache
        cache.invalidate(table_id);

        Ok(())
    }

    /// Scan all table definitions from persistence layer
    pub fn scan_all_table_definitions() -> Result<Vec<TableDefinition>, KalamDbError> {
        // Get tables provider via AppContext
        let app_ctx = crate::app_context::AppContext::get();
        let tables_provider = app_ctx.system_tables().tables();

        // Scan all tables from storage
        tables_provider
            .scan_all()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))
    }

    /// Check if table exists in persistence layer
    pub fn table_exists(cache: &TableCache, table_id: &TableId) -> Result<bool, KalamDbError> {
        // Fast path: check cache
        if cache.get(table_id).is_some() {
            return Ok(true);
        }

        // Slow path: query persistence via AppContext
        let app_ctx = crate::app_context::AppContext::get();
        let tables_provider = app_ctx.system_tables().tables();

        Ok(tables_provider.get_table_by_id(table_id)?.is_some())
    }

    /// Get Arrow schema for a table (Phase 10: Arrow Schema Memoization)
    pub fn get_arrow_schema(
        cache: &TableCache,
        table_id: &TableId,
    ) -> Result<Arc<arrow::datatypes::Schema>, KalamDbError> {
        // Fast path: check cache
        if let Some(cached) = cache.get(table_id) {
            return cached.arrow_schema();
        }

        // Slow path: try to load from persistence (lazy loading)
        if let Some(table_def) = Self::get_table_definition(cache, table_id)? {
            log::debug!("Lazy loading table definition for {}", table_id);

            // Reconstruct CachedTableData
            // Extract storage_id from options
            let storage_id = match &table_def.table_options {
                kalamdb_commons::models::schemas::TableOptions::User(opts) => {
                    Some(opts.storage_id.clone())
                }
                kalamdb_commons::models::schemas::TableOptions::Shared(opts) => {
                    Some(opts.storage_id.clone())
                }
                kalamdb_commons::models::schemas::TableOptions::Stream(_) => {
                    Some(kalamdb_commons::models::StorageId::from("local"))
                } // Default for streams
                kalamdb_commons::models::schemas::TableOptions::System(_) => None,
            };

            let mut data = CachedTableData::new(table_def.clone());

            if let Some(sid) = storage_id {
                data.storage_id = Some(sid.clone());
                // Resolve template
                match PathResolver::resolve_storage_path_template(
                    &table_id.namespace_id(),
                    &table_id.table_name(),
                    table_def.table_type,
                    &sid,
                ) {
                    Ok(template) => data.storage_path_template = template,
                    Err(e) => log::warn!(
                        "Failed to resolve storage path template during lazy load for {}: {}",
                        table_id,
                        e
                    ),
                }
            }

            let data_arc = Arc::new(data);
            cache.insert(table_id.clone(), data_arc.clone());

            return data_arc.arrow_schema();
        }

        Err(KalamDbError::TableNotFound(format!(
            "Table not found: {}",
            table_id
        )))
    }
}
