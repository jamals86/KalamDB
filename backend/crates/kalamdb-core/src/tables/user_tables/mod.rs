//! User tables module - Store types and flush jobs
//!
//! **Phase 13.6**: Provider moved to crate::providers::UserTableProvider
//! **Phase 13.6**: DML handlers deleted (logic in providers)
//!
//! This module now contains ONLY:
//! - UserTableStore (EntityStore-based storage)
//! - UserTableRow (data structure)
//! - UserTableFlushJob (background flush operations)

pub mod user_table_flush;
pub mod user_table_store;

pub use user_table_flush::UserTableFlushJob;
pub use user_table_store::{new_user_table_store, UserTableRow, UserTableStore};

// Re-export UserTableRowId from commons for convenience
pub use kalamdb_commons::ids::UserTableRowId;
