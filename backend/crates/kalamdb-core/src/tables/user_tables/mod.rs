//! User tables module
//!
//! Provides EntityStore-based storage for user-scoped tables with:
//! - User isolation (data scoped by user_id)
//! - System column injection (_updated, _deleted)
//! - Dynamic partition management

pub mod user_table_store;
pub mod user_table_provider;
pub mod user_table_insert;
pub mod user_table_update;
pub mod user_table_delete;

pub use user_table_store::{UserTableStore, UserTableRow, UserTableRowId, new_user_table_store};
pub use user_table_provider::UserTableProvider;
pub use user_table_insert::UserTableInsertHandler;
pub use user_table_update::UserTableUpdateHandler;
pub use user_table_delete::UserTableDeleteHandler;

