//! KalamCore facade for initializing core stores from a generic StorageBackend.
//!
//! This provides a minimal, forward-compatible constructor that accepts
//! `Arc<dyn kalamdb_store::storage_trait::StorageBackend>` and wires up the
//! table stores used by services. It downcasts to the RocksDB backend when
//! needed to construct existing store types.

use std::sync::Arc;

use kalamdb_store::{
    try_extract_rocksdb_db, SharedTableStore, StreamTableStore, UserTableStore,
};

/// Core store handles required by kalamdb-core services.
pub struct KalamCore {
    pub user_table_store: Arc<UserTableStore>,
    pub shared_table_store: Arc<SharedTableStore>,
    pub stream_table_store: Arc<StreamTableStore>,
}

impl KalamCore {
    /// Build table stores from a generic `StorageBackend`.
    ///
    /// Currently requires a RocksDB-backed implementation to construct
    /// the underlying store types; returns an error for unsupported backends.
    pub fn new(
        backend: Arc<dyn kalamdb_store::storage_trait::StorageBackend>,
    ) -> anyhow::Result<Self> {
        if let Some(db) = try_extract_rocksdb_db(&backend) {
            let user_table_store = Arc::new(UserTableStore::new(db.clone())?);
            let shared_table_store = Arc::new(SharedTableStore::new(db.clone())?);
            let stream_table_store = Arc::new(StreamTableStore::new(db.clone())?);

            Ok(Self {
                user_table_store,
                shared_table_store,
                stream_table_store,
            })
        } else {
            Err(anyhow::anyhow!(
                "Unsupported StorageBackend for table stores (expected RocksDB-backed)"
            ))
        }
    }
}
