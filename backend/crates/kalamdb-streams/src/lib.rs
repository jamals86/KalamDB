//! Stream storage for stream tables.
//!
//! Provides append-only writes, time-range reads, and retention cleanup.
//!
//! Two storage backends are available:
//! - `MemoryStreamLogStore`: In-memory BTreeMap-based storage (ephemeral, fast)
//! - `FileStreamLogStore`: File-based persistent storage (durable, slower)

mod config;
mod error;
mod file_store;
mod memory_store;
mod record;
mod store_trait;
mod time_bucket;
mod utils;

pub use config::StreamLogConfig;
pub use error::{Result, StreamLogError};
pub use file_store::FileStreamLogStore;
pub use memory_store::MemoryStreamLogStore;
pub use store_trait::StreamLogStore;
pub use time_bucket::{bucket_for_ttl, StreamTimeBucket};
