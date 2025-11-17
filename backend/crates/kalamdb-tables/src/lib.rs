//! # kalamdb-tables
//!
//! Table storage layer for user, shared, and stream tables in KalamDB.
//!
//! This crate contains the storage abstractions (stores) for:
//! - **UserTableStore**: Per-user partitioned table storage with RocksDB backend
//! - **SharedTableStore**: Global table storage accessible to all users
//! - **StreamTableStore**: Time-windowed streaming table storage with TTL
//!
//! **Note**: DataFusion TableProvider implementations are in `kalamdb-core/providers/`,
//! not in this crate. This crate provides only the storage layer.
//!
//! ## Architecture
//!
//! ### User Tables Storage
//! - Data partitioned by `user_id` for efficient per-user queries
//! - Implements EntityStore trait for type-safe key-value operations
//! - RocksDB backend with bincode serialization
//!
//! ### Shared Tables Storage
//! - Global storage with no user partitioning
//! - Shared across all users in a namespace
//! - Same EntityStore implementation as user tables
//!
//! ### Stream Tables Storage
//! - Time-series data storage with configurable TTL
//! - Automatic eviction of old data via background jobs
//! - Optimized for append-only workloads
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_tables::{UserTableStore, SharedTableStore, StreamTableStore};
//! use kalamdb_store::EntityStore;
//!
//! // Create and use user table store
//! let store = UserTableStore::new(backend, "user_data");
//! store.put(&key, &row)?;
//! let row = store.get(&key)?;
//! ```

pub mod error;
pub mod shared_tables;
pub mod store_ext;
pub mod stream_tables;
pub mod user_tables;

// Re-export commonly used types
pub use error::{Result, TableError};

// Re-export table stores
pub use shared_tables::shared_table_store::{
    new_shared_table_store, SharedTableRow, SharedTableStore,
};
pub use stream_tables::stream_table_store::{
    new_stream_table_store, StreamTableRow, StreamTableStore,
};
pub use user_tables::user_table_store::{new_user_table_store, UserTableRow, UserTableStore};

// Re-export extension traits
pub use store_ext::{SharedTableStoreExt, StreamTableStoreExt, UserTableStoreExt};
