//! Tables module for table types and providers
//!
//! This module contains implementations for different table types:
//! - User tables: Per-user isolated tables (EntityStore-based)
//! - Shared tables: Global tables accessible to all users (EntityStore-based)
//! - Stream tables: Ephemeral event streaming tables (in-memory EntityStore)
//! - System tables: Internal system metadata tables (EntityStore-based v2 providers)

pub mod arrow_json_conversion;
pub mod base_table_provider;
pub mod base_flush;
pub mod shared_tables;
pub mod stream_tables;
pub mod system;
pub mod unified_dml; // Phase 12: Unified DML functions for MVCC
pub mod user_tables;
pub mod version_resolution; // T052: Version resolution across storage layers

// Re-export flush types
pub use base_flush::{
    FlushJobResult, FlushMetadata, SharedTableFlushMetadata, TableFlush, UserTableFlushMetadata,
};
pub use shared_tables::SharedTableFlushJob;
pub use user_tables::UserTableFlushJob;

// Re-export base provider traits
pub use base_table_provider::{BaseTableProvider, TableProviderCore};

// Re-export from consolidated modules
pub use shared_tables::{
    new_shared_table_store, SharedTableProvider, SharedTableRow, SharedTableRowId, SharedTableStore,
};
pub use stream_tables::{
    new_stream_table_store, StreamTableRow, StreamTableRowId, StreamTableStore,
};
pub use user_tables::{
    new_user_table_store, UserTableDeleteHandler, UserTableInsertHandler,
    UserTableProvider, UserTableRow, UserTableRowId, UserTableStore, UserTableUpdateHandler,
};

// Re-export unified DML functions
pub use unified_dml::{
    append_version, extract_user_pk_value, generate_storage_key, resolve_latest_version,
    validate_primary_key,
};
