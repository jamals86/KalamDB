//! Tables module - Data structures and stores
//!
//! **Phase 13.7: Flush logic moved to crate::providers::flush**
//!
//! This module now contains ONLY:
//! - Store types (UserTableStore, SharedTableStore, StreamTableStore)
//! - Row structures (UserTableRow, SharedTableRow, StreamTableRow)
//! - Arrow/JSON conversion utilities
//! - System tables (metadata views)
//!
//! **DEPRECATED (Phase 13.6)**:
//! - UserTableProvider, SharedTableProvider, StreamTableProvider → use crate::providers::*
//! - UserTableInsertHandler, UpdateHandler, DeleteHandler → logic now in providers
//! - unified_dml, version_resolution → moved to crate::providers::*
//!
//! **DEPRECATED (Phase 13.7)**:
//! - base_flush, UserTableFlushJob, SharedTableFlushJob → moved to crate::providers::flush::*
pub mod shared_tables;
pub mod stream_tables;
pub mod system;
pub mod user_tables;

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

// Re-export flush types from providers for backward compatibility (Phase 13.7)
// DEPRECATED: Import directly from crate::providers::flush instead
pub use crate::providers::flush::{
    FlushExecutor, FlushJobResult, FlushMetadata, SharedTableFlushJob,
    SharedTableFlushMetadata, TableFlush, UserTableFlushJob, UserTableFlushMetadata,
};
