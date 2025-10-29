//! KalamCore facade for initializing core stores from a generic StorageBackend.
//!
//! This provides a minimal, forward-compatible constructor that accepts
//! `Arc<dyn kalamdb_store::StorageBackend>` and wires up the
//! table stores used by services.

use crate::stores::{SharedTableStore, StreamTableStore, UserTableStore};
use std::sync::Arc;

/// Core store handles required by kalamdb-core services.
pub struct KalamCore {
    pub user_table_store: Arc<UserTableStore>,
    pub shared_table_store: Arc<SharedTableStore>,
    pub stream_table_store: Arc<StreamTableStore>,
}

impl KalamCore {
    /// Build table stores from a generic `StorageBackend`.
    ///
    /// Creates EntityStore-based table stores that work with any StorageBackend.
    pub fn new(backend: Arc<dyn kalamdb_store::StorageBackend>) -> anyhow::Result<Self> {
        let user_table_store = Arc::new(UserTableStore::new(backend.clone()));
        let shared_table_store = Arc::new(SharedTableStore::new(backend.clone()));
        let stream_table_store = Arc::new(StreamTableStore::new(backend));

        Ok(Self {
            user_table_store,
            shared_table_store,
            stream_table_store,
        })
    }
}
