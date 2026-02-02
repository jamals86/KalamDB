//! Integration test driver for Topic Pub/Sub end-to-end tests.
//!
//! Run with: cargo test --test test_topic_integration

// Include the common test support
#[path = "../common/testserver/mod.rs"]
mod test_support;

// Topic pub/sub integration tests
mod topic_pubsub;
