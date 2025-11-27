//! HTTP request handlers
//!
//! This module provides HTTP handlers for the KalamDB REST API and WebSocket endpoints.

pub mod events;
pub mod sql_handler;
pub mod ws_handler;

pub use sql_handler::*;
pub use ws_handler::websocket_handler;
