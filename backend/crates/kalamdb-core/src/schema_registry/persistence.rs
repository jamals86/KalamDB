use crate::error::KalamDbError;
use crate::schema_registry::cached_table_data::CachedTableData;
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

        // Check if it's a system table
        if table_id.namespace_id().is_system_namespace() {
            use kalamdb_system::system_table_definitions::all_system_table_definitions;
            let all_defs = all_system_table_definitions();
            if let Some((_, def)) = all_defs.into_iter().find(|(id, _)| id == table_id) {
                return Ok(Some(Arc::new(def)));
            }
        }

        // Slow path: query persistence layer via AppContext
        let app_ctx = crate::app_context::AppContext::get();
        let tables_provider = app_ctx.system_tables().tables();

        match tables_provider.get_table_by_id(table_id)? {
            Some(table_def) => {
                // Cache the result with fully initialized storage config
                let table_arc = Arc::new(table_def);
                let data = CachedTableData::from_table_definition(table_id, table_arc.clone())?;
                cache.insert(table_id.clone(), Arc::new(data));
                Ok(Some(table_arc))
            }
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

        // Populate cache immediately with fully initialized storage config
        let table_arc = Arc::new(table_def.clone());
        let data = CachedTableData::from_table_definition(table_id, table_arc)?;
        cache.insert(table_id.clone(), Arc::new(data));

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

        // Slow path: query persistence via AppContext and cache result
        let app_ctx = crate::app_context::AppContext::get();
        let tables_provider = app_ctx.system_tables().tables();

        match tables_provider.get_table_by_id(table_id)? {
            Some(table_def) => {
                // Cache with fully initialized storage config
                let table_arc = Arc::new(table_def);
                let data = CachedTableData::from_table_definition(table_id, table_arc)?;
                cache.insert(table_id.clone(), Arc::new(data));
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Get table definition if it exists (optimized single-call pattern)
    ///
    /// Combines table existence check + definition fetch in one operation.
    /// Use this instead of calling `table_exists()` followed by `get_table_definition()`.
    ///
    /// # Performance
    /// - Cache hit: Returns immediately (no duplicate lookups)
    /// - Cache miss: Single persistence query + cache population
    /// - Prevents double fetch: table_exists() then get_table_definition()
    pub fn get_table_if_exists(
        cache: &TableCache,
        table_id: &TableId,
    ) -> Result<Option<Arc<TableDefinition>>, KalamDbError> {
        // Fast path: check cache
        if let Some(cached) = cache.get(table_id) {
            return Ok(Some(Arc::clone(&cached.table)));
        }

        // Check if it's a system table
        if table_id.namespace_id().is_system_namespace() {
            use kalamdb_system::system_table_definitions::all_system_table_definitions;
            let all_defs = all_system_table_definitions();
            if let Some((_, def)) = all_defs.into_iter().find(|(id, _)| id == table_id) {
                return Ok(Some(Arc::new(def)));
            }
        }

        // Slow path: query persistence layer via AppContext (if available)
        // Use try_get() to support test contexts where AppContext may not be initialized
        let Some(app_ctx) = crate::app_context::AppContext::try_get() else {
            return Ok(None);
        };
        let tables_provider = app_ctx.system_tables().tables();

        match tables_provider.get_table_by_id(table_id)? {
            Some(table_def) => {
                // Cache the result with fully initialized storage config
                let table_arc = Arc::new(table_def);
                let data = CachedTableData::from_table_definition(table_id, table_arc.clone())?;
                cache.insert(table_id.clone(), Arc::new(data));
                Ok(Some(table_arc))
            }
            None => Ok(None),
        }
    }

    /// Get Arrow schema for a table (Phase 10: Arrow Schema Memoization)
    pub fn get_arrow_schema(
        cache: &TableCache,
        table_id: &TableId,
    ) -> Result<(Arc<arrow::datatypes::Schema>, Option<TableId>), KalamDbError> {
        // Fast path: check cache
        if let Some(cached) = cache.get(table_id) {
            return Ok((cached.arrow_schema()?, None));
        }

        // Slow path: try to load from persistence (lazy loading)
        // get_table_definition now properly initializes storage config in cache
        if Self::get_table_definition(cache, table_id)?.is_some() {
            log::debug!("Lazy loading table definition for {}", table_id);

            // Cache is now populated with fully initialized entry - retrieve it
            if let Some(cached) = cache.get(table_id) {
                let evicted = None; // Already inserted in get_table_definition
                return Ok((cached.arrow_schema()?, evicted));
            }
        }

        Err(KalamDbError::TableNotFound(format!(
            "Table not found: {}",
            table_id
        )))
    }
}
