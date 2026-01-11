//! System Tables Registry
//!
//! Centralized registry for all system table providers. Replaces individual
//! provider fields in AppContext with a single registry pattern.
//!
//! **Phase 5 Completion**: Consolidates all 10 system table providers into
//! a single struct for cleaner AppContext API.

use super::providers::{
    AuditLogsTableProvider, JobsTableProvider, LiveQueriesTableProvider, ManifestTableProvider,
    NamespacesTableProvider, StoragesTableProvider,
    TablesTableProvider, UsersTableProvider,
};
// SchemaRegistry will be passed as Arc parameter from kalamdb-core
use datafusion::datasource::TableProvider;
use kalamdb_store::StorageBackend;
use std::sync::{Arc, RwLock};

/// Registry of all system table providers
///
/// Provides centralized access to all system.* tables.
/// Used by AppContext to eliminate 10 individual provider fields.
/// 
/// Note: information_schema.tables and information_schema.columns are provided
/// by DataFusion's built-in information_schema support (enabled via .with_information_schema(true)).
#[derive(Debug)]
pub struct SystemTablesRegistry {
    // ===== system.* tables (EntityStore-based) =====
    users: Arc<UsersTableProvider>,
    jobs: Arc<JobsTableProvider>,
    namespaces: Arc<NamespacesTableProvider>,
    storages: Arc<StoragesTableProvider>,
    live_queries: Arc<LiveQueriesTableProvider>,
    tables: Arc<TablesTableProvider>,
    audit_logs: Arc<AuditLogsTableProvider>,
    // ===== Manifest cache table =====
    manifest: Arc<ManifestTableProvider>,

    // ===== Virtual tables =====
    stats: RwLock<Option<Arc<dyn TableProvider + Send + Sync>>>,
    settings: RwLock<Option<Arc<dyn TableProvider + Send + Sync>>>,
    server_logs: RwLock<Option<Arc<dyn TableProvider + Send + Sync>>>,
    cluster: RwLock<Option<Arc<dyn TableProvider + Send + Sync>>>,
}

impl SystemTablesRegistry {
    /// Create a new system tables registry
    ///
    /// Initializes all system table providers from the storage backend.
    ///
    /// # Arguments
    /// * `storage_backend` - Storage backend for EntityStore-based providers
    /// * `kalam_sql` - KalamSQL adapter for information_schema providers
    ///
    /// # Example
    /// ```no_run
    /// use kalamdb_core::tables::system::SystemTablesRegistry;
    /// use std::sync::Arc;
    /// # use kalamdb_store::StorageBackend;
    ///
    /// # let backend: Arc<dyn StorageBackend> = unimplemented!();
    /// # let kalam_sql: Arc<KalamSql> = unimplemented!();
    /// let registry = SystemTablesRegistry::new(backend, kalam_sql);
    /// ```
    pub fn new(storage_backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            // EntityStore-based providers
            users: Arc::new(UsersTableProvider::new(storage_backend.clone())),
            jobs: Arc::new(JobsTableProvider::new(storage_backend.clone())),
            namespaces: Arc::new(NamespacesTableProvider::new(storage_backend.clone())),
            storages: Arc::new(StoragesTableProvider::new(storage_backend.clone())),
            live_queries: Arc::new(LiveQueriesTableProvider::new(storage_backend.clone())),
            tables: Arc::new(TablesTableProvider::new(storage_backend.clone())),
            audit_logs: Arc::new(AuditLogsTableProvider::new(storage_backend.clone())),

            // Manifest cache provider
            manifest: Arc::new(ManifestTableProvider::new(storage_backend)),

            // Virtual tables
            stats: RwLock::new(None), // Will be wired by kalamdb-core
            settings: RwLock::new(None), // Will be wired by kalamdb-core
            server_logs: RwLock::new(None), // Will be wired by kalamdb-core (dev only)
            cluster: RwLock::new(None), // Initialized via set_cluster_provider()
        }
    }

    // ===== Getter Methods =====

    /// Get the system.users provider
    pub fn users(&self) -> Arc<UsersTableProvider> {
        self.users.clone()
    }

    /// Get the system.jobs provider
    pub fn jobs(&self) -> Arc<JobsTableProvider> {
        self.jobs.clone()
    }

    /// Get the system.namespaces provider
    pub fn namespaces(&self) -> Arc<NamespacesTableProvider> {
        self.namespaces.clone()
    }

    /// Get the system.storages provider
    pub fn storages(&self) -> Arc<StoragesTableProvider> {
        self.storages.clone()
    }

    /// Get the system.live_queries provider
    pub fn live_queries(&self) -> Arc<LiveQueriesTableProvider> {
        self.live_queries.clone()
    }

    /// Get the system.tables provider
    pub fn tables(&self) -> Arc<TablesTableProvider> {
        self.tables.clone()
    }

    /// Get the system.audit_logs provider
    pub fn audit_logs(&self) -> Arc<AuditLogsTableProvider> {
        self.audit_logs.clone()
    }

    /// Get the system.stats provider (virtual table)
    pub fn stats(&self) -> Option<Arc<dyn TableProvider + Send + Sync>> {
        self.stats.read().unwrap().clone()
    }

    /// Set the system.stats provider (called from kalamdb-core)
    pub fn set_stats_provider(&self, provider: Arc<dyn TableProvider + Send + Sync>) {
        log::debug!("SystemTablesRegistry: Setting stats provider");
        *self.stats.write().unwrap() = Some(provider);
    }

    /// Get the system.settings provider (virtual table)
    pub fn settings(&self) -> Option<Arc<dyn TableProvider + Send + Sync>> {
        self.settings.read().unwrap().clone()
    }

    /// Set the system.settings provider (called from kalamdb-core)
    pub fn set_settings_provider(&self, provider: Arc<dyn TableProvider + Send + Sync>) {
        log::debug!("SystemTablesRegistry: Setting settings provider");
        *self.settings.write().unwrap() = Some(provider);
    }

    /// Get the system.server_logs provider (virtual table reading JSON logs)
    pub fn server_logs(&self) -> Option<Arc<dyn TableProvider + Send + Sync>> {
        self.server_logs.read().unwrap().clone()
    }

    /// Set the system.server_logs provider (called from kalamdb-core with logs path)
    pub fn set_server_logs_provider(&self, provider: Arc<dyn TableProvider + Send + Sync>) {
        log::debug!("SystemTablesRegistry: Setting server_logs provider");
        *self.server_logs.write().unwrap() = Some(provider);
    }

    /// Get the system.manifest provider
    pub fn manifest(&self) -> Arc<ManifestTableProvider> {
        self.manifest.clone()
    }

    /// Get the system.cluster provider (virtual table showing cluster status)
    pub fn cluster(&self) -> Option<Arc<dyn TableProvider + Send + Sync>> {
        self.cluster.read().unwrap().clone()
    }

    /// Set the system.cluster provider (called from kalamdb-core with executor)
    pub fn set_cluster_provider(&self, provider: Arc<dyn TableProvider + Send + Sync>) {
        log::debug!("SystemTablesRegistry: Setting cluster provider");
        *self.cluster.write().unwrap() = Some(provider);
    }

    // ===== Convenience Methods =====

    /// Get all system.* providers as a vector for bulk registration
    ///
    /// Returns tuples of (table_name, provider) for DataFusion schema registration.
    pub fn all_system_providers(
        &self,
    ) -> Vec<(&'static str, Arc<dyn datafusion::datasource::TableProvider>)> {
        let mut providers = vec![
            (
                "users",
                self.users.clone() as Arc<dyn datafusion::datasource::TableProvider>,
            ),
            (
                "jobs",
                self.jobs.clone() as Arc<dyn datafusion::datasource::TableProvider>,
            ),
            (
                "namespaces",
                self.namespaces.clone() as Arc<dyn datafusion::datasource::TableProvider>,
            ),
            (
                "storages",
                self.storages.clone() as Arc<dyn datafusion::datasource::TableProvider>,
            ),
            (
                "live_queries",
                self.live_queries.clone() as Arc<dyn datafusion::datasource::TableProvider>,
            ),
            (
                "tables",
                self.tables.clone() as Arc<dyn datafusion::datasource::TableProvider>,
            ),
            (
                "audit_logs",
                self.audit_logs.clone() as Arc<dyn datafusion::datasource::TableProvider>,
            ),
            (
                "manifest",
                self.manifest.clone() as Arc<dyn datafusion::datasource::TableProvider>,
            ),
        ];

        // Add stats if initialized (virtual view from kalamdb-core)
        if let Some(stats) = self.stats.read().unwrap().clone() {
            providers.push((
                "stats",
                stats as Arc<dyn datafusion::datasource::TableProvider>,
            ));
        }

        // Add settings if initialized (virtual view from kalamdb-core)
        if let Some(settings) = self.settings.read().unwrap().clone() {
            providers.push((
                "settings",
                settings as Arc<dyn datafusion::datasource::TableProvider>,
            ));
        }

        // Add server_logs if initialized
        if let Some(server_logs) = self.server_logs.read().unwrap().clone() {
            providers.push((
                "server_logs",
                server_logs as Arc<dyn datafusion::datasource::TableProvider>,
            ));
        }

        // Add cluster if initialized (virtual table showing OpenRaft metrics)
        if let Some(cluster) = self.cluster.read().unwrap().clone() {
            providers.push((
                "cluster",
                cluster as Arc<dyn datafusion::datasource::TableProvider>,
            ));
        }

        providers
    }
}
