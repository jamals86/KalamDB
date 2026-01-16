//! Stream storage for stream tables.
//!
//! Provides append-only writes, time-range reads, and retention cleanup.

mod config;
mod error;
mod file_store;
mod record;
mod store_trait;
mod time_bucket;
mod utils;

pub use config::StreamLogConfig;
pub use error::{Result, StreamLogError};
pub use file_store::FileStreamLogStore;
pub use store_trait::StreamLogStore;
pub use time_bucket::{bucket_for_ttl, StreamTimeBucket};
