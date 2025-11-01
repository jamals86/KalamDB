//! Stream tables module
//!
//! Provides in-memory EntityStore-based storage for ephemeral stream tables with:
//! - TTL-based automatic eviction
//! - System column injection (inserted_at, _updated, _deleted)
//! - In-memory only (not persisted to disk)

pub mod stream_table_provider;
pub mod stream_table_store;

pub use stream_table_provider::StreamTableProvider;
pub use stream_table_store::{
    new_stream_table_store, StreamTableRow, StreamTableRowId, StreamTableStore,
};
