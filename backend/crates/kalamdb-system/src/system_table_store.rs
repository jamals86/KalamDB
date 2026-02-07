//! System table store implementation using EntityStore pattern.
//!
//! Provides a wrapper around EntityStore for system tables with admin-only access control.
//!
//! ## Note
//!
//! For system tables that need secondary indexes, use `IndexedEntityStore` from `kalamdb-store`
//! instead. It provides automatic atomic index management via RocksDB WriteBatch.
//!
//! ## Key Serialization
//!
//! All keys are serialized using `StorageKey::storage_key()` which uses storekey encoding
//! for proper lexicographic ordering. Use `EntityStore::scan_all_typed()` and
//! `EntityStore::scan_prefix_typed()` for properly deserialized keys.

use crate::error::SystemError;
use crate::system_table_trait::SystemTableProviderExt;
use crate::SystemTable;
use kalamdb_commons::{storage::Partition, KSerializable};
use kalamdb_store::{
    entity_store::{CrossUserTableStore, EntityStore},
    StorageBackend, StorageKey,
};
use std::sync::Arc;

/// Generic store for system tables with admin-only access control.
///
/// This is a thin wrapper around EntityStore that provides:
/// - Typed access to system tables (K = key type, V = value type)
/// - Admin-only access control (CrossUserTableStore returns None for table_access)
/// - Integration with SystemTableProviderExt trait
///
/// All scan operations with typed keys are available via the `EntityStore` trait methods:
/// - `scan_all_typed()` - Scan all entries with typed (K, V) pairs
/// - `scan_prefix_typed()` - Scan entries matching a key prefix
/// - `scan_iterator()` - Returns an iterator over typed (K, V) pairs
///
/// For tables that need secondary indexes, use `IndexedEntityStore` instead.
#[derive(Clone)]
pub struct SystemTableStore<K, V> {
    backend: Arc<dyn StorageBackend>,
    system_table: SystemTable,
    partition: Partition,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> SystemTableStore<K, V> {
    /// Create a new system table store
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    /// * `system_table` - Which system table this store manages
    pub fn new(backend: Arc<dyn StorageBackend>, system_table: SystemTable) -> Self {
        let partition_name = system_table
            .column_family_name()
            .expect("SystemTableStore requires a persisted system table (not a view)");
        Self {
            backend,
            system_table,
            partition: Partition::new(partition_name),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the system table type
    pub fn system_table(&self) -> &SystemTable {
        &self.system_table
    }
}

/// Implement EntityStore trait to enable get/put/delete/scan operations
impl<K, V> EntityStore<K, V> for SystemTableStore<K, V>
where
    K: StorageKey,
    V: KSerializable + 'static,
{
    /// Returns a reference to the storage backend.
    ///
    /// ⚠️ **INTERNAL USE ONLY** - Do not call this method directly from outside the trait!
    /// This method is only meant to be used internally by EntityStore trait methods.
    /// Use the provided EntityStore methods (get, put, delete, scan_*, etc.) instead.
    #[doc(hidden)]
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    fn partition(&self) -> Partition {
        self.partition.clone()
    }
}

/// Implement SystemTableProviderExt for integration with table providers
impl<K: Send + Sync, V: Send + Sync> SystemTableProviderExt for SystemTableStore<K, V> {
    fn table_name(&self) -> &str {
        self.system_table.table_name()
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
    V: KSerializable + 'static,
{
    fn table_access(&self) -> Option<kalamdb_commons::models::TableAccess> {
        None // System tables are admin-only
    }
}
