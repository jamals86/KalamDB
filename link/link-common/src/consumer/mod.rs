//! Kafka-style topic consumer support shared by the KalamDB SDK.

/// Data models (always available — serializable structs, no tokio dependency).
pub mod models;

#[cfg(all(feature = "tokio-runtime", feature = "consumer"))]
pub mod core;
#[cfg(all(feature = "tokio-runtime", feature = "consumer"))]
pub mod utils;

#[cfg(all(feature = "tokio-runtime", feature = "consumer"))]
pub use core::{ConsumerBuilder, TopicConsumer};

#[cfg(feature = "tokio-runtime")]
pub use models::ConsumerConfig;
pub use models::{
    AckResponse, AutoOffsetReset, CommitMode, CommitResult, ConsumeMessage, ConsumeRequest,
    ConsumeResponse, ConsumerOffsets, ConsumerRecord, PayloadMode, TopicOp,
};
