//! Tables module - Data structures and stores
//!
//! **Phase 13.6: Providers moved to crate::providers**
//!
//! This module now contains ONLY:
//! - Store types (UserTableStore, SharedTableStore, StreamTableStore)
//! - Row structures (UserTableRow, SharedTableRow, StreamTableRow)
//! - Flush job types (UserTableFlushJob, SharedTableFlushJob)
//! - Arrow/JSON conversion utilities
//! - System tables (metadata views)
//!
//! **DEPRECATED**:
//! - UserTableProvider, SharedTableProvider, StreamTableProvider → use crate::providers::*
//! - UserTableInsertHandler, UpdateHandler, DeleteHandler → logic now in providers
//! - unified_dml, version_resolution → moved to crate::providers::*

pub mod arrow_json_conversion;
pub mod base_flush;
pub mod shared_tables;
pub mod stream_tables;
pub mod system;
pub mod user_tables;

// Re-export flush types
pub use base_flush::{
    FlushJobResult, FlushMetadata, SharedTableFlushMetadata, TableFlush, UserTableFlushMetadata,
};
pub use shared_tables::SharedTableFlushJob;
pub use user_tables::UserTableFlushJob;

// NOTE: BaseTableProvider trait and TableProviderCore are now in crate::providers::base
// The old tables/base_table_provider.rs has been deleted (Phase 13.6)

// Re-export store types and row structures
pub use shared_tables::{
    new_shared_table_store, SharedTableRow, SharedTableRowId, SharedTableStore,
};
pub use stream_tables::{
    new_stream_table_store, StreamTableRow, StreamTableRowId, StreamTableStore,
};
pub use user_tables::{
    new_user_table_store, UserTableRow, UserTableRowId, UserTableStore,
};

// Re-export helper functions
pub use arrow_json_conversion::{arrow_value_to_json, schema_with_system_columns};
