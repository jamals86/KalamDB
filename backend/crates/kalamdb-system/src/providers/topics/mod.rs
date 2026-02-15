//! System.topics table module (system_topics in RocksDB)
//!
//! This module contains all components for the system.topics table:
//! - Topic model and TopicRoute configuration
//! - Table schema definition with OnceLock caching
//! - IndexedEntityStore wrapper for type-safe storage
//! - TableProvider for DataFusion integration
//! - Index definitions for efficient queries by name/alias

pub mod models;
pub mod topics_provider;

pub use models::{Topic, TopicRoute};
pub use topics_provider::{TopicsStore, TopicsTableProvider};
