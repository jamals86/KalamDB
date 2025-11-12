//! Stream tables module
//!
//! Provides in-memory EntityStore-based storage for ephemeral stream tables with:
//! - TTL-based automatic eviction (Phase 13.2: via _seq timestamp)
//! - MVCC architecture with user_id and _seq system columns
//! - In-memory only (not persisted to disk, ephemeral data pattern)

pub mod stream_table_store;

pub use stream_table_store::{
    new_stream_table_store, StreamTableRow, StreamTableStore,
};

// Re-export StreamTableRowId from kalamdb_commons for convenience
pub use kalamdb_commons::ids::StreamTableRowId;

