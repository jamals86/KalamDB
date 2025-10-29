//! # kalamdb-store
//!
//! Low-level key-value store abstraction for user, shared, and stream tables.
//! This crate isolates all direct RocksDB interactions for non-system tables,
//! allowing kalamdb-core to remain free of RocksDB dependencies.
//!
//! ## Architecture
//!
//! ```text
//! kalamdb-core (business logic)
//!     ↓
//! kalamdb-store (K/V operations)
//!     ↓
//! RocksDB (storage engine)
//! ```
//!
//! ## Table Types
//!
//! - **User Tables**: Isolated per user with key format `{user_id}:{row_id}`
//! - **Shared Tables**: Global data with key format `{row_id}`
//! - **Stream Tables**: Ephemeral events with key format `{timestamp_ms}:{row_id}`

pub mod audit_log;
pub mod common;
pub mod entity_store; // Phase 14: Type-safe EntityStore<K, V> with generic keys
pub mod index; // Generic secondary index support
pub mod key_encoding;
pub mod rocksdb_impl;
pub mod rocksdb_init;
// pub mod s3_storage; // T171: S3 storage backend (requires cmake build dependency)
pub mod sharding;
pub mod storage_trait;
pub mod traits; // Old EntityStore<T> trait (to be deprecated after Phase 14 migration)

// NOTE: Old RocksDB-based table stores removed - use EntityStore implementations in kalamdb-core instead
// - UserTableStore, SharedTableStore, StreamTableStore are now in kalamdb-core/src/stores/
// pub mod shared_table_store;
// pub mod stream_table_store;
// pub mod user_table_store;

pub use rocksdb_impl::RocksDBBackend;
pub use rocksdb_init::RocksDbInit;
// pub use s3_storage::S3Storage; // T171: Export S3Storage (requires cmake)
pub use sharding::{
    AlphabeticSharding, ConsistentHashSharding, NumericSharding, ShardingRegistry, ShardingStrategy,
};
pub use storage_trait::{Operation, Partition, StorageBackend, StorageError};

// Phase 14: Export new type-safe EntityStore traits
pub use entity_store::{
    CrossUserTableStore,
    EntityStore as EntityStoreV2, // Alias to avoid conflict during migration
};

// Export index types
pub use audit_log::AuditLogStore;
pub use index::{FunctionExtractor, IndexKeyExtractor, SecondaryIndex};

// NOTE: Old RocksDB-based table stores removed - use EntityStore implementations in kalamdb-core instead
// pub use shared_table_store::SharedTableStore;
// pub use stream_table_store::StreamTableStore;
// pub use user_table_store::UserTableStore;

// Make test_utils available for testing in dependent crates
pub mod test_utils;

/// Attempt to extract a RocksDB handle from a generic `StorageBackend`.
///
/// Returns `Some(Arc<rocksdb::DB>)` when the backend is a RocksDB-backed implementation,
/// otherwise returns `None`.
pub fn try_extract_rocksdb_db(
    backend: &std::sync::Arc<dyn crate::storage_trait::StorageBackend>,
) -> Option<std::sync::Arc<rocksdb::DB>> {
    backend
        .as_any()
        .downcast_ref::<crate::rocksdb_impl::RocksDBBackend>().map(|rb| rb.db().clone())
}
