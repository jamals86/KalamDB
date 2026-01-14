//! System Tables Registry
//!
//! Centralized registry for all system table providers. Replaces individual
//! provider fields in AppContext with a single registry pattern.
//!
//! **Phase 5 Completion**: Consolidates all 10 system table providers into
//! a single struct for cleaner AppContext API.

use super::providers::{
    AuditLogsTableProvider, AuditLogsTableSchema, JobsTableProvider, JobsTableSchema,
    LiveQueriesTableProvider, LiveQueriesTableSchema, ManifestTableProvider, ManifestTableSchema,
    NamespacesTableProvider, NamespacesTableSchema, StoragesTableProvider, StoragesTableSchema,
    TablesTableProvider, TablesTableSchema, UsersTableProvider, UsersTableSchema,
};
// SchemaRegistry will be passed as Arc parameter from kalamdb-core
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::TableProvider;
use kalamdb_commons::{models::TableId, schemas::TableDefinition, SystemTable};
use kalamdb_store::StorageBackend;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
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
    cluster_groups: RwLock<Option<Arc<dyn TableProvider + Send + Sync>>>,

    // Cached persisted system table definitions
    system_definitions: OnceCell<HashMap<TableId, Arc<TableDefinition>>>,

    // Cached Arrow schemas for persisted system tables
    system_schemas: OnceCell<HashMap<TableId, SchemaRef>>,
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
            cluster_groups: RwLock::new(None), // Initialized via set_cluster_groups_provider()

            system_definitions: OnceCell::new(),
            system_schemas: OnceCell::new(),
        }
    }

    fn definitions_map(&self) -> &HashMap<TableId, Arc<TableDefinition>> {
        self.system_definitions.get_or_init(|| {
            let defs: Vec<(SystemTable, TableDefinition)> = vec![
                (SystemTable::Users, UsersTableSchema::definition()),
                (SystemTable::Namespaces, NamespacesTableSchema::definition()),
                (SystemTable::Tables, TablesTableSchema::definition()),
                (SystemTable::Storages, StoragesTableSchema::definition()),
                (SystemTable::LiveQueries, LiveQueriesTableSchema::definition()),
                (SystemTable::Jobs, JobsTableSchema::definition()),
                (SystemTable::AuditLog, AuditLogsTableSchema::definition()),
                (SystemTable::Manifest, ManifestTableSchema::definition()),
            ];

            defs.into_iter()
                .map(|(table, definition)| (table.table_id(), Arc::new(definition)))
                .collect()
        })
    }

    pub fn get_system_definition(&self, table_id: &TableId) -> Option<Arc<TableDefinition>> {
        self.definitions_map().get(table_id).cloned()
    }

    pub fn all_system_table_definitions_cached(&self) -> Vec<Arc<TableDefinition>> {
        self.definitions_map().values().cloned().collect()
    }

    fn schemas_map(&self) -> &HashMap<TableId, SchemaRef> {
        self.system_schemas.get_or_init(|| {
            vec![
                (SystemTable::Users, UsersTableSchema::schema()),
                (SystemTable::Namespaces, NamespacesTableSchema::schema()),
                (SystemTable::Tables, TablesTableSchema::schema()),
                (SystemTable::Storages, StoragesTableSchema::schema()),
                (SystemTable::LiveQueries, LiveQueriesTableSchema::schema()),
                (SystemTable::Jobs, JobsTableSchema::schema()),
                (SystemTable::AuditLog, AuditLogsTableSchema::schema()),
                (SystemTable::Manifest, ManifestTableSchema::schema()),
            ]
            .into_iter()
            .map(|(table, schema)| (table.table_id(), schema))
            .collect()
        })
    }

    pub fn get_system_schema(&self, table_id: &TableId) -> Option<SchemaRef> {
        self.schemas_map().get(table_id).cloned()
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

    /// Get the system.cluster_groups provider (virtual table showing per-group status)
    pub fn cluster_groups(&self) -> Option<Arc<dyn TableProvider + Send + Sync>> {
        self.cluster_groups.read().unwrap().clone()
    }

    /// Set the system.cluster_groups provider (called from kalamdb-core with executor)
    pub fn set_cluster_groups_provider(&self, provider: Arc<dyn TableProvider + Send + Sync>) {
        log::debug!("SystemTablesRegistry: Setting cluster_groups provider");
        *self.cluster_groups.write().unwrap() = Some(provider);
    }

    // ===== Convenience Methods =====

    /// Get all system.* providers as a vector for bulk registration
    ///
    /// Returns tuples of (table_name, provider) for DataFusion schema registration.
    pub fn all_system_providers(&self) -> Vec<(SystemTable, Arc<dyn TableProvider>)> {
        let mut providers = vec![
            (
                SystemTable::Users,
                self.users.clone() as Arc<dyn TableProvider>,
            ),
            (
                SystemTable::Jobs,
                self.jobs.clone() as Arc<dyn TableProvider>,
            ),
            (
                SystemTable::Namespaces,
                self.namespaces.clone() as Arc<dyn TableProvider>,
            ),
            (
                SystemTable::Storages,
                self.storages.clone() as Arc<dyn TableProvider>,
            ),
            (
                SystemTable::LiveQueries,
                self.live_queries.clone() as Arc<dyn TableProvider>,
            ),
            (
                SystemTable::Tables,
                self.tables.clone() as Arc<dyn TableProvider>,
            ),
            (
                SystemTable::AuditLog,
                self.audit_logs.clone() as Arc<dyn TableProvider>,
            ),
            (
                SystemTable::Manifest,
                self.manifest.clone() as Arc<dyn TableProvider>,
            ),
        ];

        // Add stats if initialized (virtual view from kalamdb-core)
        if let Some(stats) = self.stats.read().unwrap().clone() {
            providers.push((
                SystemTable::Stats,
                stats as Arc<dyn TableProvider>,
            ));
        }

        // Add settings if initialized (virtual view from kalamdb-core)
        if let Some(settings) = self.settings.read().unwrap().clone() {
            providers.push((
                SystemTable::Settings,
                settings as Arc<dyn TableProvider>,
            ));
        }

        // Add server_logs if initialized
        if let Some(server_logs) = self.server_logs.read().unwrap().clone() {
            providers.push((
                SystemTable::ServerLogs,
                server_logs as Arc<dyn TableProvider>,
            ));
        }

        // Add cluster if initialized (virtual table showing OpenRaft metrics)
        if let Some(cluster) = self.cluster.read().unwrap().clone() {
            providers.push((
                SystemTable::Cluster,
                cluster as Arc<dyn TableProvider>,
            ));
        }

        // Add cluster_groups if initialized (virtual table showing per-group OpenRaft metrics)
        if let Some(cluster_groups) = self.cluster_groups.read().unwrap().clone() {
            providers.push((
                SystemTable::ClusterGroups,
                cluster_groups as Arc<dyn TableProvider>,
            ));
        }

        providers
    }
}
