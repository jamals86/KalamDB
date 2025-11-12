//! Shared tables module - Store types only
//!
//! **Phase 13.6**: Provider moved to crate::providers::SharedTableProvider
//! **Phase 13.7**: Flush logic moved to crate::providers::flush::SharedTableFlushJob
//!
//! This module now contains ONLY:
//! - SharedTableStore (EntityStore-based storage)
//! - SharedTableRow (data structure)

pub mod shared_table_store;

pub use shared_table_store::{new_shared_table_store, SharedTableRow, SharedTableStore};

// Re-export SharedTableRowId from commons for convenience
pub use kalamdb_commons::ids::SharedTableRowId;
