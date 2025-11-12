//! Flush module for flush policy management and utilities
//!
//! This module manages flush policies that determine when data is written
//! from RocksDB to Parquet storage.
//!
//! Phase 13.7 Migration: All flush logic moved to providers/flush:
//! - SharedTableFlushJob: crate::providers::flush::SharedTableFlushJob
//! - UserTableFlushJob: crate::providers::flush::UserTableFlushJob
//! - JsonBatchBuilder: crate::providers::flush::util::JsonBatchBuilder
//!
//! This module now re-exports from providers/flush for backward compatibility.

// Re-export from providers/flush (Phase 13.7)
pub use crate::providers::flush::{
    FlushExecutor, FlushJobResult, FlushMetadata, JsonBatchBuilder, SharedTableFlushJob,
    SharedTableFlushMetadata, TableFlush, UserTableFlushJob, UserTableFlushMetadata,
};

// Legacy re-export for backward compatibility
pub mod util {
    pub use crate::providers::flush::util::*;
}
