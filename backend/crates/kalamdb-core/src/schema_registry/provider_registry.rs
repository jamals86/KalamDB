use crate::error::KalamDbError;
use dashmap::DashMap;
use datafusion::datasource::TableProvider;
use kalamdb_commons::models::TableId;
use std::sync::{Arc, OnceLock};

/// Registry for DataFusion table providers
pub struct ProviderRegistry {
    /// Cached DataFusion providers per table (shared/stream safe to reuse)
    providers: DashMap<TableId, Arc<dyn TableProvider + Send + Sync>>,

    /// DataFusion base session context for table registration (set once during init)
    base_session_context: OnceLock<Arc<datafusion::prelude::SessionContext>>,
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRegistry")
            .field("providers_count", &self.providers.len())
            .field(
                "base_session_context",
                &self.base_session_context.get().is_some(),
            )
            .finish()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: DashMap::new(),
            base_session_context: OnceLock::new(),
        }
    }

    /// Set the DataFusion base session context for table registration
    pub fn set_base_session_context(&self, session: Arc<datafusion::prelude::SessionContext>) {
        let _ = self.base_session_context.set(session);
    }

    /// Insert a DataFusion provider into the cache for a table
    pub fn insert_provider(
        &self,
        table_id: TableId,
        provider: Arc<dyn TableProvider + Send + Sync>,
    ) -> Result<(), KalamDbError> {
        log::info!(
            "[SchemaRegistry] Inserting provider for table {}.{}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );
        // Store in our cache
        self.providers.insert(table_id.clone(), provider.clone());

        // Also register with DataFusion's catalog if available
        if let Some(base_session) = self.base_session_context.get() {
            // Use constant catalog name "kalam" instead of catalog_names().first()
            // to avoid Vec allocation and be more explicit
            let catalog = base_session.catalog("kalam").ok_or_else(|| {
                KalamDbError::InvalidOperation("Catalog 'kalam' not found".to_string())
            })?;

            // Get or create namespace schema
            let schema = catalog
                .schema(table_id.namespace_id().as_str())
                .unwrap_or_else(|| {
                    // Create namespace schema if it doesn't exist
                    let new_schema =
                        Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
                    catalog
                        .register_schema(table_id.namespace_id().as_str(), new_schema.clone())
                        .expect("Failed to register namespace schema");
                    new_schema
                });

            // Register table with DataFusion, tolerate duplicates
            match schema.register_table(table_id.table_name().as_str().to_string(), provider) {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    // If the table already exists, treat as idempotent success
                    if msg.to_lowercase().contains("already exists")
                        || msg.to_lowercase().contains("exists")
                    {
                        log::warn!(
                            "[SchemaRegistry] Table {}.{} already registered in DataFusion; continuing",
                            table_id.namespace_id().as_str(),
                            table_id.table_name().as_str()
                        );
                    } else {
                        return Err(KalamDbError::InvalidOperation(format!(
                            "Failed to register table with DataFusion: {}",
                            e
                        )));
                    }
                }
            }

            log::info!(
                "[SchemaRegistry] Registered table {}.{} with DataFusion catalog",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
        }

        Ok(())
    }

    /// Remove a cached DataFusion provider for a table and unregister from DataFusion
    pub fn remove_provider(&self, table_id: &TableId) -> Result<(), KalamDbError> {
        // Remove from internal cache
        let _ = self.providers.remove(table_id);

        // Unregister from DataFusion if base_session_context is set
        if let Some(base_session) = self.base_session_context.get() {
            let catalog_name = base_session
                .state()
                .config()
                .options()
                .catalog
                .default_catalog
                .clone();

            let catalog = base_session.catalog(&catalog_name).ok_or_else(|| {
                KalamDbError::InvalidOperation(format!("Catalog '{}' not found", catalog_name))
            })?;

            // Get namespace schema
            if let Some(schema) = catalog.schema(table_id.namespace_id().as_str()) {
                // Deregister table from DataFusion
                schema
                    .deregister_table(table_id.table_name().as_str())
                    .map_err(|e| {
                        KalamDbError::InvalidOperation(format!(
                            "Failed to deregister table from DataFusion: {}",
                            e
                        ))
                    })?;

                log::debug!(
                    "Unregistered table {}.{} from DataFusion catalog",
                    table_id.namespace_id().as_str(),
                    table_id.table_name().as_str()
                );
            }
        }

        Ok(())
    }

    /// Get a cached DataFusion provider for a table
    pub fn get_provider(&self, table_id: &TableId) -> Option<Arc<dyn TableProvider + Send + Sync>> {
        let result = self.providers.get(table_id).map(|e| Arc::clone(e.value()));
        if result.is_some() {
            log::trace!(
                "[SchemaRegistry] Retrieved provider for table {}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
        } else {
            log::warn!(
                "[SchemaRegistry] Provider NOT FOUND for table {}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
        }
        result
    }

    /// Clear all providers
    pub fn clear(&self) {
        self.providers.clear();
    }

    /// Get number of providers
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
