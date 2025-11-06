//! System Tables Registry
//!
//! Centralized registry for all system table providers. Replaces individual
//! provider fields in AppContext with a single registry pattern.
//!
//! **Phase 5 Completion**: Consolidates all 10 system table providers into
//! a single struct for cleaner AppContext API.

use super::{
    AuditLogsTableProvider, InformationSchemaColumnsProvider, InformationSchemaTablesProvider,
    JobsTableProvider, LiveQueriesTableProvider, NamespacesTableProvider, StatsTableProvider,
    StoragesTableProvider, TablesTableProvider, UsersTableProvider,
};
use crate::schema_registry::SchemaRegistry;
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
    
    // ===== Virtual tables =====
    stats: Arc<StatsTableProvider>,
    
    // ===== information_schema.* tables (lazy-initialized) =====
    information_schema_tables: RwLock<Option<Arc<InformationSchemaTablesProvider>>>,
    information_schema_columns: RwLock<Option<Arc<InformationSchemaColumnsProvider>>>,
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
            audit_logs: Arc::new(AuditLogsTableProvider::new(storage_backend)),
            
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
    pub fn set_information_schema_dependencies(&self, schema_registry: Arc<SchemaRegistry>) {
        // Initialize information_schema.tables
        let tables_provider = Arc::new(InformationSchemaTablesProvider::new(
            self.tables.clone(),
            schema_registry.clone(),
        ));
        *self.information_schema_tables.write().unwrap() = Some(tables_provider);

        // TODO: Initialize information_schema.columns (needs similar refactoring)
        // For now, leave as None - this provider needs to be refactored too
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
    
    /// Get the information_schema.tables provider
    pub fn information_schema_tables(&self) -> Option<Arc<InformationSchemaTablesProvider>> {
        self.information_schema_tables.read().unwrap().clone()
    }
    
    /// Get the information_schema.columns provider
    pub fn information_schema_columns(&self) -> Option<Arc<InformationSchemaColumnsProvider>> {
        self.information_schema_columns.read().unwrap().clone()
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
