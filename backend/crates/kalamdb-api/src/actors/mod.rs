//! Actix actors for WebSocket management
//!
//! This module provides actor implementations for managing WebSocket connections
//! and live query subscriptions.

pub mod ws_session;

pub use ws_session::WebSocketSession;
