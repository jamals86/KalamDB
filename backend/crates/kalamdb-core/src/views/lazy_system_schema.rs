//! Lazy System Schema Provider
//!
//! A custom DataFusion SchemaProvider that lazily loads virtual views on first access.
//! Persisted system tables are served immediately from SystemTablesRegistry, while
//! virtual views (stats, settings, server_logs, datatypes, etc.) are created on-demand
//! and cached for subsequent access.
//!
//! **Benefits**:
//! - Faster startup time (skip view creation until needed)
//! - Lower memory usage (only accessed views in memory)
//! - Same performance for system tables (direct method calls)
//! - Microsecond overhead on first view access (cached after)

use async_trait::async_trait;
use dashmap::DashMap;
use datafusion::catalog::SchemaProvider;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use kalamdb_configs::ServerConfig;
use kalamdb_raft::CommandExecutor;
use kalamdb_session::secure_provider;
use kalamdb_system::{SystemTable, SystemTablesRegistry};
use parking_lot::RwLock;
use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use super::cluster::create_cluster_provider;
use super::cluster_groups::create_cluster_groups_provider;
use super::columns_view::create_columns_view_provider;
use super::datatypes::{DatatypesTableProvider, DatatypesView};
use super::server_logs::create_server_logs_provider;
use super::settings::{SettingsTableProvider, SettingsView};
use super::stats::{StatsTableProvider, StatsView};
use super::tables_view::create_tables_view_provider;

/// Configuration for lazy view initialization
pub struct LazyViewConfig {
    /// Server configuration (for settings view)
    pub config: Arc<ServerConfig>,
    /// Logs path (for server_logs view)
    pub logs_path: PathBuf,
    /// Command executor (for cluster views, set after Raft init)
    pub executor: RwLock<Option<Arc<dyn CommandExecutor>>>,
}

impl std::fmt::Debug for LazyViewConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyViewConfig")
            .field("logs_path", &self.logs_path)
            .field("has_executor", &self.executor.read().is_some())
            .finish()
    }
}

/// Lazy-loading SchemaProvider for the `system` schema
///
/// This provider:
/// 1. Serves persisted tables (users, jobs, etc.) directly from SystemTablesRegistry
/// 2. Creates virtual views (stats, settings, etc.) on first access and caches them
/// 3. Uses DashMap for lock-free concurrent caching
///
/// **Virtual Views** (lazy-loaded):
/// - `stats` - Runtime metrics (needs AppContext callback after init)
/// - `settings` - Server configuration
/// - `server_logs` - JSON log file reader
/// - `datatypes` - Arrow→KalamDB type mappings
/// - `tables` - Table metadata view
/// - `columns` - Column metadata view
/// - `cluster` - Cluster status (needs CommandExecutor)
/// - `cluster_groups` - Per-group cluster status (needs CommandExecutor)
#[derive(Debug)]
pub struct LazySystemSchemaProvider {
    /// Registry of persisted system tables
    system_tables: Arc<SystemTablesRegistry>,
    /// Configuration for lazy view initialization
    view_config: Arc<LazyViewConfig>,
    /// Cache for lazily-loaded view providers
    view_cache: DashMap<String, Arc<dyn TableProvider>>,
}

impl LazySystemSchemaProvider {
    /// Create a new lazy system schema provider
    pub fn new(
        system_tables: Arc<SystemTablesRegistry>,
        config: Arc<ServerConfig>,
        logs_path: PathBuf,
    ) -> Self {
        Self {
            system_tables,
            view_config: Arc::new(LazyViewConfig {
                config,
                logs_path,
                executor: RwLock::new(None),
            }),
            view_cache: DashMap::new(),
        }
    }

    /// Set the command executor (called after Raft initialization)
    pub fn set_executor(&self, executor: Arc<dyn CommandExecutor>) {
        *self.view_config.executor.write() = Some(executor);
    }

    /// Get the StatsView for callback wiring
    ///
    /// Since StatsView needs AppContext for metrics callback, we expose it
    /// so the caller can wire the callback after creation.
    pub fn get_or_create_stats_view(&self) -> Arc<StatsView> {
        // Check if already cached
        if let Some(provider) = self.view_cache.get("stats") {
            // Downcast to get the StatsView
            if let Some(stats_provider) = provider.as_any().downcast_ref::<StatsTableProvider>() {
                return Arc::clone(stats_provider.view());
            }
        }

        // Create and cache
        let stats_view = Arc::new(StatsView::new());
        let provider = Arc::new(StatsTableProvider::new(Arc::clone(&stats_view)));
        let secured = secure_provider(provider as Arc<dyn TableProvider>, SystemTable::Stats.table_id());
        self.view_cache.insert("stats".to_string(), secured);
        stats_view
    }

    /// List all known table names in the system schema
    fn known_table_names() -> Vec<String> {
        // Dynamically gather all system table/view names from SystemTable enum
        SystemTable::all()
            .iter()
            .map(|t| t.table_name().to_string())
            .collect()
    }

    /// Get persisted table provider (fast path - already created in SystemTablesRegistry)
    fn get_persisted_table(&self, name: &str) -> Option<Arc<dyn TableProvider>> {
        // Parse table name to SystemTable enum
        let system_table = SystemTable::from_name(name).ok()?;
        
        // Only return persisted tables (not virtual views)
        if system_table.is_view() {
            return None;
        }

        // Use registry's all_system_providers to find the provider
        self.system_tables
            .all_system_providers()
            .into_iter()
            .find(|(table, _)| table == &system_table)
            .map(|(_, provider)| provider)
    }

    /// Get or create virtual view provider (lazy path - cached after first access)
    fn get_or_create_view(&self, name: &str) -> Option<Arc<dyn TableProvider>> {
        log::debug!("get_or_create_view('{}') called", name);
        
        // Check cache first
        if let Some(provider) = self.view_cache.get(name) {
            log::debug!("get_or_create_view('{}') found in cache", name);
            return Some(Arc::clone(&*provider));
        }

        log::info!("get_or_create_view('{}') not in cache, creating new view...", name);

        // Create provider based on view name
        let (table, provider): (SystemTable, Arc<dyn TableProvider>) = match SystemTable::from_name(name).ok()? {
            SystemTable::Stats => {
                // Stats view - create with empty callback, caller will wire it
                let stats_view = Arc::new(StatsView::new());
                let provider = Arc::new(StatsTableProvider::new(stats_view));
                (SystemTable::Stats, provider as Arc<dyn TableProvider>)
            }
            SystemTable::Settings => {
                let settings_view = Arc::new(SettingsView::with_config((*self.view_config.config).clone()));
                let provider = Arc::new(SettingsTableProvider::new(settings_view));
                (SystemTable::Settings, provider as Arc<dyn TableProvider>)
            }
            SystemTable::ServerLogs => {
                let provider = Arc::new(create_server_logs_provider(&self.view_config.logs_path));
                (SystemTable::ServerLogs, provider as Arc<dyn TableProvider>)
            }
            SystemTable::Datatypes => {
                let datatypes_view = Arc::new(DatatypesView::new());
                let provider = Arc::new(DatatypesTableProvider::new(datatypes_view));
                (SystemTable::Datatypes, provider as Arc<dyn TableProvider>)
            }
            SystemTable::Tables => {
                let provider = Arc::new(create_tables_view_provider(Arc::clone(&self.system_tables)));
                (SystemTable::Tables, provider as Arc<dyn TableProvider>)
            }
            SystemTable::Columns => {
                let provider = Arc::new(create_columns_view_provider(Arc::clone(&self.system_tables)));
                (SystemTable::Columns, provider as Arc<dyn TableProvider>)
            }
            SystemTable::Cluster => {
                // Cluster view needs CommandExecutor - return None if not available
                let executor = self.view_config.executor.read();
                let executor = executor.as_ref()?;
                let provider = Arc::new(create_cluster_provider(Arc::clone(executor)));
                (SystemTable::Cluster, provider as Arc<dyn TableProvider>)
            }
            SystemTable::ClusterGroups => {
                // Cluster groups view needs CommandExecutor - return None if not available
                let executor = self.view_config.executor.read();
                let executor = executor.as_ref()?;
                let provider = Arc::new(create_cluster_groups_provider(Arc::clone(executor)));
                (SystemTable::ClusterGroups, provider as Arc<dyn TableProvider>)
            }
            _ => return None,
        };

        // Wrap with security and cache
        let secured: Arc<dyn TableProvider> = secure_provider(provider, table.table_id());
        self.view_cache.insert(name.to_string(), Arc::clone(&secured));
        
        log::debug!("LazySystemSchemaProvider: Created and cached view '{}'", name);
        Some(secured)
    }
}

#[async_trait]
impl SchemaProvider for LazySystemSchemaProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn table_names(&self) -> Vec<String> {
        let names = Self::known_table_names();
        log::info!("LazySystemSchemaProvider::table_names() returning {} tables", names.len());
        names
    }

    fn table_exist(&self, name: &str) -> bool {
        let exists = Self::known_table_names().contains(&name.to_string());
        log::debug!("LazySystemSchemaProvider::table_exist('{}') = {}", name, exists);
        exists
    }

    async fn table(&self, name: &str) -> DataFusionResult<Option<Arc<dyn TableProvider>>> {
        log::info!("LazySystemSchemaProvider::table() called for '{}'", name);
        
        // Fast path: persisted tables (already created in SystemTablesRegistry)
        if let Some(provider) = self.get_persisted_table(name) {
            log::info!("LazySystemSchemaProvider: Found persisted table '{}'", name);
            return Ok(Some(provider));
        }

        // Lazy path: virtual views (create on demand, cache for subsequent access)
        let result = self.get_or_create_view(name);
        if result.is_some() {
            log::info!("LazySystemSchemaProvider: Successfully created/cached view '{}'", name);
        } else {
            log::warn!("LazySystemSchemaProvider: Failed to create view '{}' - returning None", name);
        }
        Ok(result)
    }

    /// Override table_type for efficiency - returns without loading the full table
    async fn table_type(&self, name: &str) -> DataFusionResult<Option<TableType>> {
        if !self.table_exist(name) {
            return Ok(None);
        }

        // Use SystemTable::from_name and is_view() for dynamic type detection
        let system_table = SystemTable::from_name(name).ok();
        let table_type = match system_table {
            Some(table) if table.is_view() => TableType::View,
            Some(_) => TableType::Base,
            None => return Ok(None),
        };

        Ok(Some(table_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_provider() -> LazySystemSchemaProvider {
        let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(InMemoryBackend::new());
        let system_tables = Arc::new(SystemTablesRegistry::new(backend));
        let config = Arc::new(ServerConfig::default());
        let logs_path = std::path::PathBuf::from("/tmp/kalamdb-test/logs");
        std::fs::create_dir_all(&logs_path).ok();

        LazySystemSchemaProvider::new(system_tables, config, logs_path)
    }

    #[test]
    fn test_table_names_returns_all_known_tables() {
        let provider = create_test_provider();
        let names = provider.table_names();
        
        // Check all persisted tables
        assert!(names.contains(&"users".to_string()));
        assert!(names.contains(&"jobs".to_string()));
        assert!(names.contains(&"namespaces".to_string()));
        assert!(names.contains(&"storages".to_string()));
        
        // Check all virtual views
        assert!(names.contains(&"stats".to_string()));
        assert!(names.contains(&"settings".to_string()));
        assert!(names.contains(&"datatypes".to_string()));
        assert!(names.contains(&"tables".to_string()));
        assert!(names.contains(&"columns".to_string()));
    }

    #[test]
    fn test_table_exist_for_known_tables() {
        let provider = create_test_provider();
        
        assert!(provider.table_exist("users"));
        assert!(provider.table_exist("stats"));
        assert!(provider.table_exist("settings"));
        assert!(provider.table_exist("datatypes"));
        assert!(!provider.table_exist("nonexistent"));
    }

    #[tokio::test]
    async fn test_persisted_table_access() {
        let provider = create_test_provider();
        
        // Persisted tables should be available immediately
        let users = provider.table("users").await.unwrap();
        assert!(users.is_some());
        
        let jobs = provider.table("jobs").await.unwrap();
        assert!(jobs.is_some());
    }

    #[tokio::test]
    async fn test_lazy_view_access() {
        let provider = create_test_provider();
        
        // Virtual views should be created on first access
        assert!(provider.view_cache.is_empty() || !provider.view_cache.contains_key("settings"));
        
        let settings = provider.table("settings").await.unwrap();
        assert!(settings.is_some());
        
        // Should now be cached
        assert!(provider.view_cache.contains_key("settings"));
        
        // Second access should use cache
        let settings2 = provider.table("settings").await.unwrap();
        assert!(settings2.is_some());
    }

    #[tokio::test]
    async fn test_datatypes_view_lazy_load() {
        let provider = create_test_provider();
        
        let datatypes = provider.table("datatypes").await.unwrap();
        assert!(datatypes.is_some());
        assert!(provider.view_cache.contains_key("datatypes"));
    }

    #[tokio::test]
    async fn test_tables_view_lazy_load() {
        let provider = create_test_provider();
        
        let tables = provider.table("tables").await.unwrap();
        assert!(tables.is_some());
        assert!(provider.view_cache.contains_key("tables"));
    }

    #[tokio::test]
    async fn test_columns_view_lazy_load() {
        let provider = create_test_provider();
        
        let columns = provider.table("columns").await.unwrap();
        assert!(columns.is_some());
        assert!(provider.view_cache.contains_key("columns"));
    }

    #[tokio::test]
    async fn test_cluster_view_requires_executor() {
        let provider = create_test_provider();
        
        // Without executor, cluster view should return None
        let cluster = provider.table("cluster").await.unwrap();
        assert!(cluster.is_none());
        
        let cluster_groups = provider.table("cluster_groups").await.unwrap();
        assert!(cluster_groups.is_none());
    }

    #[tokio::test]
    async fn test_table_type_without_loading() {
        let provider = create_test_provider();
        
        // table_type should work without loading the table
        let users_type = provider.table_type("users").await.unwrap();
        assert_eq!(users_type, Some(TableType::Base));
        
        let stats_type = provider.table_type("stats").await.unwrap();
        assert_eq!(stats_type, Some(TableType::View));
        
        let unknown_type = provider.table_type("nonexistent").await.unwrap();
        assert_eq!(unknown_type, None);
    }

    #[tokio::test]
    async fn test_nonexistent_table_returns_none() {
        let provider = create_test_provider();
        
        let result = provider.table("nonexistent_table").await.unwrap();
        assert!(result.is_none());
    }
}
