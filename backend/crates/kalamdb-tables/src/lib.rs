//! # kalamdb-tables
//!
//! Table providers for user, shared, and stream tables in KalamDB.
//!
//! This crate contains DataFusion TableProvider implementations for:
//! - **UserTableProvider**: Per-user partitioned tables with RLS (Row-Level Security)
//! - **SharedTableProvider**: Global tables accessible to all users
//! - **StreamTableProvider**: Time-windowed streaming tables with TTL
//!
//! ## Architecture
//!
//! ### User Tables
//! - Data partitioned by `user_id` for efficient per-user queries
//! - Automatic row-level security filtering
//! - Hot storage (RocksDB) + cold storage (Parquet files)
//! - Flush policy based on row count and time
//!
//! ### Shared Tables
//! - Global tables with no user partitioning
//! - Shared across all users in a namespace
//! - Same hot/cold storage architecture as user tables
//!
//! ### Stream Tables
//! - Time-series data with configurable TTL
//! - Automatic eviction of old data
//! - Optimized for append-only workloads
//! - Window-based queries
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_tables::{UserTableProvider, SharedTableProvider, StreamTableProvider};
//!
//! // Create user table provider
//! let user_provider = UserTableProvider::new(
//!     table_id,
//!     schema,
//!     store,
//!     filestore,
//! )?;
//!
//! // Register with DataFusion
//! session_state.register_table(table_name, Arc::new(user_provider))?;
//! ```

pub mod user_tables;
pub mod shared_tables;
pub mod stream_tables;
pub mod error;
pub mod store_ext;

// Re-export commonly used types
pub use error::{TableError, Result};

// Re-export table stores
pub use user_tables::user_table_store::{UserTableRow, UserTableStore, new_user_table_store};
pub use shared_tables::shared_table_store::{SharedTableRow, SharedTableStore, new_shared_table_store};
pub use stream_tables::stream_table_store::{StreamTableRow, StreamTableStore, new_stream_table_store};

// Re-export extension traits
pub use store_ext::{UserTableStoreExt, SharedTableStoreExt, StreamTableStoreExt};
