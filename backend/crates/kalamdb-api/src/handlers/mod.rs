//! HTTP request handlers
//!
//! This module provides HTTP handlers for the KalamDB REST API and WebSocket endpoints.

pub mod auth;
pub mod cluster_handler;
pub mod events;
pub mod sql_handler;
pub mod ws_handler;

pub use auth::{login_handler, logout_handler, me_handler, refresh_handler};
pub use cluster_handler::cluster_health_handler;
pub use sql_handler::*;
pub use ws_handler::websocket_handler;
