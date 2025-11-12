//! Shared tables module - Store types and flush jobs
//!
//! **Phase 13.6**: Provider moved to crate::providers::SharedTableProvider
//!
//! This module now contains ONLY:
//! - SharedTableStore (EntityStore-based storage)
//! - SharedTableRow (data structure)
//! - SharedTableFlushJob (background flush operations)

pub mod shared_table_flush;
pub mod shared_table_store;

pub use shared_table_flush::SharedTableFlushJob;
pub use shared_table_store::{new_shared_table_store, SharedTableRow, SharedTableStore};

// Re-export SharedTableRowId from commons for convenience
pub use kalamdb_commons::ids::SharedTableRowId;
