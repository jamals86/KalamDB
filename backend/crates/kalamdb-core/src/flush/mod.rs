//! Flush module for flush policy management
//!
//! This module manages flush policies that determine when data is written
//! from RocksDB to Parquet storage.
//!
//! **Phase 14 Step 11 Migration**: Flush job implementations moved to table modules:
//! - SharedTableFlushJob: crate::tables::shared_tables::shared_table_flush
//! - UserTableFlushJob: crate::tables::user_tables::user_table_flush
//!
//! This module now re-exports from new locations for backward compatibility.

pub mod util;

// Re-export from new locations (Phase 14 Step 11)
pub use crate::tables::base_flush::FlushJobResult;
pub use crate::tables::shared_tables::SharedTableFlushJob;
pub use crate::tables::user_tables::UserTableFlushJob;
