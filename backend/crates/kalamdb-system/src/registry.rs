//! System Tables Registry
//!
//! Centralized registry for all system table providers. Replaces individual
//! provider fields in AppContext with a single registry pattern.
//!
//! **Phase 5 Completion**: Consolidates all 10 system table providers into
//! a single struct for cleaner AppContext API.

use super::{
    AuditLogsTableProvider, JobsTableProvider, LiveQueriesTableProvider, NamespacesTableProvider, 
    StatsTableProvider, StoragesTableProvider, TablesTableProvider, UsersTableProvider,
    ManifestTableProvider,
};
// SchemaRegistry will be passed as Arc parameter from kalamdb-core
use datafusion::datasource::TableProvider;
use kalamdb_store::StorageBackend;
use std::sync::{Arc, RwLock};

/// Registry of all system table providers
///
/// Provides centralized access to all system.* and information_schema.* tables.
/// Used by AppContext to eliminate 10 individual provider fields.
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
    stats: Arc<StatsTableProvider>,
    
    // ===== information_schema.* tables (lazy-initialized, using VirtualView pattern) =====
    information_schema_tables: RwLock<Option<Arc<dyn TableProvider>>>,
    information_schema_columns: RwLock<Option<Arc<dyn TableProvider>>>,
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
    pub fn new(
        storage_backend: Arc<dyn StorageBackend>
    ) -> Self {
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
            stats: Arc::new(StatsTableProvider::new(None)), // Will be wired with cache later
            
            // Information schema providers (lazy-initialized in set_information_schema_dependencies)
            information_schema_tables: RwLock::new(None),
            information_schema_columns: RwLock::new(None),
        }
    }

    /// Set information_schema dependencies after SchemaRegistry is created
    ///
    /// This is called from AppContext::init() after schema_registry is available.
    /// Note: This method is now implemented in kalamdb-core to avoid circular dependencies.
    pub fn set_information_schema_dependencies(&self, schema_registry: Arc<dyn std::any::Any + Send + Sync>) {
        // Implementation moved to kalamdb-core::app_context
        // Views are created there and set via set_information_schema_tables/columns methods
        let _ = schema_registry; // Suppress unused warning
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
    pub fn stats(&self) -> Arc<StatsTableProvider> {
        self.stats.clone()
    }

    /// Get the system.manifest provider
    pub fn manifest(&self) -> Arc<ManifestTableProvider> {
       self.manifest.clone()
    }

    /// Get the information_schema.tables provider
    pub fn information_schema_tables(&self) -> Option<Arc<dyn TableProvider>> {
        self.information_schema_tables.read().unwrap().clone()
    }
    
    /// Get the information_schema.columns provider
    pub fn information_schema_columns(&self) -> Option<Arc<dyn TableProvider>> {
        self.information_schema_columns.read().unwrap().clone()
    }
    
    /// Set the information_schema.tables provider (called from kalamdb-core)
    pub fn set_information_schema_tables(&self, provider: Arc<dyn TableProvider>) {
        *self.information_schema_tables.write().unwrap() = Some(provider);
    }
    
    /// Set the information_schema.columns provider (called from kalamdb-core)
    pub fn set_information_schema_columns(&self, provider: Arc<dyn TableProvider>) {
        *self.information_schema_columns.write().unwrap() = Some(provider);
    }
    
    // ===== Convenience Methods =====
    
    /// Get all system.* providers as a vector for bulk registration
    ///
    /// Returns tuples of (table_name, provider) for DataFusion schema registration.
    pub fn all_system_providers(&self) -> Vec<(&'static str, Arc<dyn datafusion::datasource::TableProvider>)> {
        vec![
            ("users", self.users.clone() as Arc<dyn datafusion::datasource::TableProvider>),
            ("jobs", self.jobs.clone() as Arc<dyn datafusion::datasource::TableProvider>),
            ("namespaces", self.namespaces.clone() as Arc<dyn datafusion::datasource::TableProvider>),
            ("storages", self.storages.clone() as Arc<dyn datafusion::datasource::TableProvider>),
            ("live_queries", self.live_queries.clone() as Arc<dyn datafusion::datasource::TableProvider>),
            ("tables", self.tables.clone() as Arc<dyn datafusion::datasource::TableProvider>),
            ("audit_logs", self.audit_logs.clone() as Arc<dyn datafusion::datasource::TableProvider>),
            ("stats", self.stats.clone() as Arc<dyn datafusion::datasource::TableProvider>),
                    ("manifest", self.manifest.clone() as Arc<dyn datafusion::datasource::TableProvider>),
        ]
    }
    
    /// Get all information_schema.* providers as a vector
    ///
    /// Only returns providers that have been initialized via set_information_schema_dependencies().
    pub fn all_information_schema_providers(&self) -> Vec<(&'static str, Arc<dyn datafusion::datasource::TableProvider>)> {
        let mut providers = Vec::new();
        
        if let Some(tables) = self.information_schema_tables.read().unwrap().clone() {
            providers.push(("tables", tables as Arc<dyn datafusion::datasource::TableProvider>));
        }
        
        if let Some(columns) = self.information_schema_columns.read().unwrap().clone() {
            providers.push(("columns", columns as Arc<dyn datafusion::datasource::TableProvider>));
        }
        
        providers
    }
}
