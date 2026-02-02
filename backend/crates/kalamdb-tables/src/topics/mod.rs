//! Topic tables module - Store types for pub/sub messaging
//!
//! This module contains storage implementations for KalamDB's pub/sub system:
//! - TopicMessageStore (EntityStore-based message storage)
//! - TopicOffsetStore (EntityStore-based offset tracking)
//! - TopicMessageSchema (cached Arrow schema for CONSUME queries)
//!
//! Similar to shared_tables, this uses EntityStore for efficient key-value access.

pub mod topic_message_models;
pub mod topic_message_schema;
pub mod topic_message_store;
pub mod topic_offset_models;
pub mod topic_offset_store;

pub use topic_message_models::{TopicMessage, TopicMessageId};
pub use topic_message_schema::topic_message_schema;
pub use topic_message_store::{TopicMessageStore};
pub use topic_offset_models::{TopicOffset, TopicOffsetId};
pub use topic_offset_store::{TopicOffsetStore};