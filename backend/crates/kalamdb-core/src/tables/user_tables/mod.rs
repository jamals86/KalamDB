//! User tables module - Store types only
//!
//! **Phase 13.6**: Provider moved to crate::providers::UserTableProvider
//! **Phase 13.6**: DML handlers deleted (logic in providers)
//! **Phase 13.7**: Flush logic moved to crate::providers::flush::UserTableFlushJob
//!
//! This module now contains ONLY:
//! - UserTableStore (EntityStore-based storage)
//! - UserTableRow (data structure)

pub mod user_table_store;

pub use user_table_store::{new_user_table_store, UserTableRow, UserTableStore};

// Re-export UserTableRowId from commons for convenience
pub use kalamdb_commons::ids::UserTableRowId;
