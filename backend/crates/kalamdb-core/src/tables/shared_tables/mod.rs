//! Shared tables module
//!
//! Provides EntityStore-based storage for cross-user shared tables with:
//! - Access control (public, private, restricted)
//! - System column injection (_updated, _deleted)
//! - Dynamic partition management

pub mod shared_table_flush;
pub mod shared_table_provider;
pub mod shared_table_store;

pub use shared_table_flush::SharedTableFlushJob;
pub use shared_table_provider::SharedTableProvider;
pub use shared_table_store::{new_shared_table_store, SharedTableRow, SharedTableStore};

// Re-export SharedTableRowId from commons for convenience
pub use kalamdb_commons::ids::SharedTableRowId;
