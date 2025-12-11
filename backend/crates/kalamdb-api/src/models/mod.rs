//! API data models
//!
//! This module defines the request and response structures for the KalamDB API,
//! including REST API models and WebSocket message formats.

pub mod sql_request;
pub mod sql_response;
pub mod ws_notification;

// Re-export commonly used types
pub use sql_request::QueryRequest;
pub use sql_response::{ErrorDetail, QueryResult, ResponseStatus, SqlResponse};
pub use ws_notification::{ChangeType, Notification};
