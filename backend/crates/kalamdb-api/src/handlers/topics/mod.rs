//! Topic pub/sub HTTP handlers
//!
//! This module provides REST API endpoints for topic consumption and acknowledgment.
//! These endpoints complement the SQL surface (CONSUME, ACK) with HTTP/JSON interfaces.
//!
//! ## Endpoints
//! - POST /v1/api/topics/consume - Consume messages from a topic
//! - POST /v1/api/topics/ack - Acknowledge offset for consumer group
//!
//! **Authorization**: Endpoints require `service`, `dba`, or `system` role (NOT `user`).

pub mod models;

mod ack;
mod consume;

pub use ack::ack_handler;
pub use consume::consume_handler;
