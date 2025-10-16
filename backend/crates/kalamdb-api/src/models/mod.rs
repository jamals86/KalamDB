//! API data models
//!
//! This module defines the request and response structures for the KalamDB API,
//! including REST API models and WebSocket message formats.

pub mod sql_request;
pub mod sql_response;
pub mod ws_notification;
pub mod ws_subscription;

// Re-export commonly used types
pub use sql_request::SqlRequest;
pub use sql_response::{ErrorDetail, QueryResult, SqlResponse};
pub use ws_notification::{ChangeType, Notification};
pub use ws_subscription::{Subscription, SubscriptionOptions, SubscriptionRequest};
