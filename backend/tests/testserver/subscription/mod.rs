//! Live Query Subscription Tests
//!
//! Tests covering:
//! - Live query inserts
//! - Live query updates
//! - Live query deletes
//! - Stream TTL eviction

#[path = "../../common/testserver/mod.rs"]
#[allow(dead_code)]
pub(super) mod test_support;

// Subscription Tests
mod test_live_query_inserts;
mod test_live_query_updates;
mod test_live_query_deletes;
mod test_stream_ttl_eviction_sql;
