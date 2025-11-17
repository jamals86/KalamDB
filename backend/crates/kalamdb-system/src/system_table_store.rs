//! System table store implementation using EntityStore pattern.
//!
//! Provides a wrapper around EntityStore for system tables with admin-only access control.

use crate::error::SystemError;
use crate::system_table_trait::SystemTableProviderExt;
use kalamdb_store::{
    entity_store::{CrossUserTableStore, EntityStore},
    StorageBackend, StorageKey,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Generic store for system tables with admin-only access control.
///
/// This is a thin wrapper around EntityStore that provides:
/// - Typed access to system tables (K = key type, V = value type)
/// - Admin-only access control (CrossUserTableStore returns None for table_access)
/// - Integration with SystemTableProviderExt trait
pub struct SystemTableStore<K, V> {
    backend: Arc<dyn StorageBackend>,
    partition: String,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> SystemTableStore<K, V> {
    /// Create a new system table store
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    /// * `partition` - Partition name (e.g., "system_users")
    pub fn new(backend: Arc<dyn StorageBackend>, partition: impl Into<String>) -> Self {
        Self {
            backend,
            partition: partition.into(),
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Implement EntityStore trait to enable get/put/delete/scan operations
impl<K, V> EntityStore<K, V> for SystemTableStore<K, V>
where
    K: StorageKey,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    fn partition(&self) -> &str {
        &self.partition
    }
}

/// Implement SystemTableProviderExt for integration with table providers
impl<K: Send + Sync, V: Send + Sync> SystemTableProviderExt for SystemTableStore<K, V> {
    fn table_name(&self) -> &str {
        self.partition
            .strip_prefix("system_")
            .unwrap_or(&self.partition)
    }

    fn schema_ref(&self) -> arrow::datatypes::SchemaRef {
        Arc::new(arrow::datatypes::Schema::empty())
    }

    fn load_batch(&self) -> std::result::Result<arrow::record_batch::RecordBatch, SystemError> {
        Err(SystemError::InvalidOperation(
            "System tables do not support RecordBatch loading".to_string(),
        ))
    }
}

/// Implement CrossUserTableStore to mark as admin-only (no per-user access control)
impl<K, V> CrossUserTableStore<K, V> for SystemTableStore<K, V>
where
    K: StorageKey,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn table_access(&self) -> Option<kalamdb_commons::models::TableAccess> {
        None // System tables are admin-only
    }
}
