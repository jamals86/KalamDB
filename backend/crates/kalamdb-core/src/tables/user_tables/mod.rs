//! User tables module
//!
//! Provides EntityStore-based storage for user-scoped tables with:
//! - User isolation (data scoped by user_id)
//! - System column injection (_updated, _deleted)
//! - Dynamic partition management

pub mod user_table_delete;
pub mod user_table_flush;
pub mod user_table_insert;
pub mod user_table_provider;
pub mod user_table_store;
pub mod user_table_update;

pub use user_table_delete::UserTableDeleteHandler;
pub use user_table_flush::UserTableFlushJob;
pub use user_table_insert::UserTableInsertHandler;
pub use user_table_store::{new_user_table_store, UserTableRow, UserTableStore};
pub use user_table_update::UserTableUpdateHandler;
pub use user_table_provider::UserTableProvider;

// Re-export UserTableRowId from commons for convenience
pub use kalamdb_commons::ids::UserTableRowId;
