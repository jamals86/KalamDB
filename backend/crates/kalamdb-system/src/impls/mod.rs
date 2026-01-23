use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::types::{Manifest, ManifestCacheEntry};
use kalamdb_commons::{StorageId, TableId, UserId};
use kalamdb_store::StorageError;
use std::sync::Arc;

/// Interface for LiveQueryManager implementations used by table providers.
pub trait LiveQueryManager: Send + Sync {
    type Notification: Send + Sync + 'static;

    fn notify_table_change_async(
        &self,
        user_id: UserId,
        table_id: TableId,
        notification: Self::Notification,
    );
}

/// Interface for ManifestService implementations used by table providers.
pub trait ManifestService: Send + Sync {
    fn get_or_load(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Option<Arc<ManifestCacheEntry>>, StorageError>;

    fn validate_manifest(&self, manifest: &Manifest) -> Result<(), StorageError>;

    fn mark_as_stale(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError>;

    fn rebuild_manifest(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError>;

    fn mark_pending_write(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError>;

    fn ensure_manifest_initialized(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError>;

    fn stage_before_flush(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
    ) -> Result<(), StorageError>;

    fn get_manifest_user_ids(&self, table_id: &TableId) -> Result<Vec<UserId>, StorageError>;
}

/// Interface for SchemaRegistry implementations used by table providers.
pub trait SchemaRegistry: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn get_arrow_schema(&self, table_id: &TableId) -> Result<SchemaRef, Self::Error>;

    fn get_table_if_exists(
        &self,
        table_id: &TableId,
    ) -> Result<Option<Arc<TableDefinition>>, Self::Error>;

    fn get_arrow_schema_for_version(
        &self,
        table_id: &TableId,
        schema_version: u32,
    ) -> Result<SchemaRef, Self::Error>;

    fn get_storage_id(&self, table_id: &TableId) -> Result<StorageId, Self::Error>;
}

/// Interface for cluster leadership checks used by providers.
#[async_trait::async_trait]
pub trait ClusterCoordinator: Send + Sync {
    async fn is_cluster_mode(&self) -> bool;

    async fn is_leader_for_user(&self, user_id: &UserId) -> bool;

    async fn is_leader_for_shared(&self) -> bool;

    async fn leader_addr_for_user(&self, user_id: &UserId) -> Option<String>;
}
