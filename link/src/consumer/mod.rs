//! Kafka-style topic consumer for kalam-link.

pub mod core;
pub mod models;
pub mod utils;

pub use core::{ConsumerBuilder, TopicConsumer};
pub use models::{
    AutoOffsetReset, CommitMode, CommitResult, ConsumerConfig, ConsumerOffsets, ConsumerRecord,
    PayloadMode, TopicOp,
};
