//! Tables module for table types and providers
//!
//! This module contains implementations for different table types:
//! - User tables: Per-user isolated tables
//! - Shared tables: Global tables accessible to all users
//! - Stream tables: Ephemeral event streaming tables
//! - System tables: Internal system metadata tables

pub mod hybrid_table_provider;
pub mod parquet_scan;
pub mod rocksdb_scan;
pub mod system;
pub mod user_table_delete;
pub mod user_table_insert;
pub mod user_table_provider;
pub mod user_table_update;

pub use hybrid_table_provider::HybridTableProvider;
pub use user_table_delete::UserTableDeleteHandler;
pub use user_table_insert::UserTableInsertHandler;
pub use user_table_provider::UserTableProvider;
pub use user_table_update::UserTableUpdateHandler;
