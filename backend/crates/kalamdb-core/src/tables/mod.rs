//! Tables module for table types and providers
//!
//! This module contains implementations for different table types:
//! - User tables: Per-user isolated tables (EntityStore-based)
//! - Shared tables: Global tables accessible to all users (EntityStore-based)
//! - Stream tables: Ephemeral event streaming tables (in-memory EntityStore)
//! - System tables: Internal system metadata tables (EntityStore-based v2 providers)

pub mod arrow_json_conversion;
pub mod parquet_scan;
pub mod system;
pub mod user_tables;
pub mod shared_tables;
pub mod stream_tables;

// Re-export from consolidated modules
pub use user_tables::{
    UserTableStore, UserTableRow, UserTableRowId, new_user_table_store,
    UserTableProvider,
    UserTableInsertHandler,
    UserTableUpdateHandler,
    UserTableDeleteHandler,
};
pub use shared_tables::{
    SharedTableStore, SharedTableRow, SharedTableRowId, new_shared_table_store,
    SharedTableProvider,
};
pub use stream_tables::{
    StreamTableStore, StreamTableRow, StreamTableRowId, new_stream_table_store,
    StreamTableProvider,
};
