//! Table row models for user, shared, and stream tables.
//!
//! **Phase 3 (Module Consolidation)**: Models moved to respective table modules.
//! This module now re-exports for backward compatibility.
//!
//! - `UserTableRow`: Moved to `crate::tables::user_tables::UserTableRow`
//! - `SharedTableRow`: Moved to `crate::tables::shared_tables::SharedTableRow`
//! - `StreamTableRow`: Moved to `crate::tables::stream_tables::StreamTableRow`

// Re-export table row models from their respective modules
pub use crate::tables::user_tables::user_table_store::UserTableRow;
pub use crate::tables::shared_tables::shared_table_store::SharedTableRow;
pub use crate::tables::stream_tables::stream_table_store::StreamTableRow;
