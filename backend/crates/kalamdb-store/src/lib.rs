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

pub mod common;
pub mod entity_store; // Phase 14: Type-safe EntityStore<K, V> with generic keys
pub mod index; // Generic secondary index support
pub mod indexed_store; // Phase 15: Automatic secondary index management
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
pub use storage_trait::{Operation, Partition, StorageBackend, StorageBackendAsync, StorageError};

// Re-export StorageKey from kalamdb-commons to avoid import inconsistency
pub use kalamdb_commons::StorageKey;

// Phase 14: Export new type-safe EntityStore traits
pub use entity_store::{
    CrossUserTableStore,
    EntityStore as EntityStoreV2, // Alias to avoid conflict during migration, FIXME: Rename to EntityStore later
    EntityStoreAsync,             // Async versions using spawn_blocking internally
};

// Export index types
pub use index::{FunctionExtractor, IndexKeyExtractor, SecondaryIndex};

// Phase 15: Export IndexedEntityStore for automatic index management
pub use indexed_store::{IndexDefinition, IndexedEntityStore};

#[cfg(feature = "datafusion")]
pub use indexed_store::{extract_i64_equality, extract_string_equality};

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
        .downcast_ref::<crate::rocksdb_impl::RocksDBBackend>()
        .map(|rb| rb.db().clone())
}
