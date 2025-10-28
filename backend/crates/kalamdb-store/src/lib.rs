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
pub mod key_encoding;
pub mod rocksdb_impl;
// pub mod s3_storage; // T171: S3 storage backend (requires cmake build dependency)
pub mod sharding;
pub mod shared_table_store;
pub mod storage_trait;
pub mod stream_table_store;
pub mod traits; // EntityStore<T> trait
pub mod user_table_store;

pub use rocksdb_impl::RocksDBBackend;
// pub use s3_storage::S3Storage; // T171: Export S3Storage (requires cmake)
pub use sharding::{
    AlphabeticSharding, ConsistentHashSharding, NumericSharding, ShardingRegistry, ShardingStrategy,
};
pub use shared_table_store::SharedTableStore;
pub use storage_trait::{Operation, Partition, StorageBackend, StorageError};
pub use stream_table_store::StreamTableStore;
pub use traits::EntityStore; // Export EntityStore trait
pub use user_table_store::UserTableStore;

#[cfg(test)]
mod tests;

// Make test_utils available for testing in dependent crates
pub mod test_utils;
