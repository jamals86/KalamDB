//! System.topic_offsets table module (system_topic_offsets in RocksDB)
//!
//! This module contains all components for the system.topic_offsets table:
//! - TopicOffset model for consumer group progress tracking
//! - Table schema definition with OnceLock caching
//! - IndexedEntityStore wrapper for type-safe storage
//! - TableProvider for DataFusion integration
//! - Composite primary key (topic_id, group_id, partition_id)

pub mod models;
pub mod topic_offsets_provider;
pub mod topic_offsets_table;

pub use models::TopicOffset;
pub use topic_offsets_provider::{TopicOffsetsStore, TopicOffsetsTableProvider};
pub use topic_offsets_table::TopicOffsetsTableSchema;
