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

pub use hybrid_table_provider::HybridTableProvider;
