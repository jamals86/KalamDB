//! Tables module - Re-exports from kalamdb-tables and kalamdb-system
//!
//! This module re-exports table stores from kalamdb-tables crate and
//! system tables from kalamdb-system crate for backward compatibility.

// Re-export table stores from kalamdb-tables
pub mod user_tables {
    pub use kalamdb_tables::user_tables::*;
}

pub mod shared_tables {
    pub use kalamdb_tables::shared_tables::*;
}

pub mod stream_tables {
    pub use kalamdb_tables::stream_tables::*;
}

// System tables remain in kalamdb-core (they import from kalamdb-system)
pub mod system;

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
