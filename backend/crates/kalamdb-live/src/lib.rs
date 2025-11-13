//! KalamDB Live Query Streaming Library
//!
//! This crate provides real-time query result streaming functionality for KalamDB.
//! It enables clients to subscribe to live queries and receive updates as data changes.

pub mod connection_registry;
pub mod error;
pub mod filter;
pub mod initial_data;
pub mod manager;

// Re-export main types
pub use connection_registry::{ConnectionId, LiveId, LiveQueryOptions, LiveQueryRegistry, NodeId};
pub use error::{KalamDbError, LiveError, Result};
pub use filter::FilterCache;
pub use initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
pub use manager::LiveQueryManager;
