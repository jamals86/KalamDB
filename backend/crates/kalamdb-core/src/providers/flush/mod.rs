//! Flush module for table data persistence
//!
//! This module consolidates all flushing logic from RocksDB to Parquet files.
//! It provides a unified trait-based architecture that eliminates code duplication.
//!
//! ## Module Structure
//!
//! - `base`: Common flush trait and result types
//! - `users`: User table flush implementation (multi-file, RLS-enforced)
//! - `shared`: Shared table flush implementation (single-file)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::providers::flush::{UserTableFlushJob, FlushJobResult};
//!
//! let flush_job = UserTableFlushJob::new(/* ... */);
//! let result = flush_job.execute()?;
//! ```

pub mod base;
pub mod shared;
pub mod users;
pub mod util;

// Re-export common types
pub use base::{
    FlushJobResult, FlushMetadata, SharedTableFlushMetadata, TableFlush,
    UserTableFlushMetadata,
};
pub use shared::SharedTableFlushJob;
pub use users::UserTableFlushJob;
pub use util::JsonBatchBuilder;
