//! WebSocket subscription management for real-time updates.
//!
//! Provides real-time change notifications via WebSocket connections.

mod manager;
mod reader;

pub use manager::SubscriptionManager;
pub(crate) use reader::ws_reader_loop;
