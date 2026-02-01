use crate::error::KalamDbError;
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::websocket::ChangeNotification;
use kalamdb_commons::TableId;
use kalamdb_filestore::StorageRegistry;
use kalamdb_system::{
    ClusterCoordinator as ClusterCoordinatorTrait,
    NotificationService as NotificationServiceTrait,
    ManifestService as ManifestServiceTrait,
    SchemaRegistry as SchemaRegistryTrait,
};
use std::sync::Arc;

/// Shared core state for all table providers
///
/// **Memory Optimization**: All provider types share this core structure,
/// reducing per-table memory footprint from 3× allocation to 1× allocation.
///
/// **Phase 12 Refactoring**: Uses kalamdb-registry services directly
///
/// **Services**:
/// - `schema_registry`: Table schema management and caching (from kalamdb-registry)
/// - `system_columns`: SeqId generation, _deleted flag handling (from kalamdb-registry)
/// - `notification_service`: WebSocket notifications (optional, from kalamdb-core)
/// - `storage_registry`: Storage path resolution (optional, from kalamdb-core)
pub struct TableProviderCore {
    /// Table identity shared by provider implementations
    table_id: Arc<TableId>,

    /// Logical table type used for routing/storage decisions
    table_type: TableType,

    /// Schema registry for table metadata and Arrow schema caching
    pub schema_registry: Arc<dyn SchemaRegistryTrait<Error = KalamDbError>>,

    /// System columns service for _seq and _deleted management
    pub system_columns: Arc<kalamdb_system::SystemColumnsService>,

    /// Storage registry for resolving full storage paths (optional)
    pub storage_registry: Option<Arc<StorageRegistry>>,

    /// Manifest service interface (for manifest cache + rebuilds)
    pub manifest_service: Arc<dyn ManifestServiceTrait>,

    /// Live query manager interface (for change notifications)
    pub notification_service: Arc<dyn NotificationServiceTrait<Notification = ChangeNotification>>,

    /// Cluster coordinator for leader checks
    pub cluster_coordinator: Arc<dyn ClusterCoordinatorTrait>,
}

impl TableProviderCore {
    /// Create new core with required services
    pub fn new(
        table_id: TableId,
        table_type: TableType,
        schema_registry: Arc<dyn SchemaRegistryTrait<Error = KalamDbError>>,
        system_columns: Arc<kalamdb_system::SystemColumnsService>,
        storage_registry: Option<Arc<StorageRegistry>>,
        manifest_service: Arc<dyn ManifestServiceTrait>,
        notification_service: Arc<dyn NotificationServiceTrait<Notification = ChangeNotification>>,
        cluster_coordinator: Arc<dyn ClusterCoordinatorTrait>,
    ) -> Self {
        Self {
            table_id: Arc::new(table_id),
            table_type,
            schema_registry,
            system_columns,
            storage_registry,
            manifest_service,
            notification_service,
            cluster_coordinator,
        }
    }

    /// Add StorageRegistry to core
    pub fn with_storage_registry(mut self, registry: Arc<StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
    }

    /// ManifestService accessor
    pub fn manifest_service(&self) -> &Arc<dyn ManifestServiceTrait> {
        &self.manifest_service
    }

    /// LiveQueryManager accessor
    pub fn notification_service(&self) -> &Arc<dyn NotificationServiceTrait<Notification = ChangeNotification>> {
        &self.notification_service
    }

    /// Cluster coordinator accessor
    pub fn cluster_coordinator(&self) -> &Arc<dyn ClusterCoordinatorTrait> {
        &self.cluster_coordinator
    }

    /// TableId accessor (shared across providers)
    pub fn table_id(&self) -> &TableId {
        self.table_id.as_ref()
    }

    /// Best-effort primary key column_id for this table.
    /// Returns 0 if the schema is missing or no primary key is defined.
    pub fn primary_key_column_id(&self) -> u64 {
        self.schema_registry
            .get_table_if_exists(self.table_id())
            .ok()
            .flatten()
            .and_then(|def| def.columns.iter().find(|c| c.is_primary_key).map(|c| c.column_id))
            .unwrap_or(0)
    }

    /// Cloneable TableId handle (avoids leaking Arc internals to callers)
    pub fn table_id_arc(&self) -> Arc<TableId> {
        self.table_id.clone()
    }

    /// TableType accessor
    pub fn table_type(&self) -> TableType {
        self.table_type
    }
}
