// Aggregator for connection tests to ensure Cargo picks them up
//
// Run these tests with:
//   cargo test --test connection
//
// Run individual test files:
//   cargo test --test connection connection_options
//   cargo test --test connection subscription_options
//   cargo test --test connection reconnection
//   cargo test --test connection resume_seqid
//   cargo test --test connection timeout
//   cargo test --test connection live_connection
//
// Unit tests (no server required):
mod common;
#[path = "connection/connection_options_tests.rs"]
mod connection_options_tests;
#[path = "connection/subscription_options_tests.rs"]
mod subscription_options_tests;
#[path = "connection/reconnection_tests.rs"]
mod reconnection_tests;
#[path = "connection/resume_seqid_tests.rs"]
mod resume_seqid_tests;
#[path = "connection/timeout_tests.rs"]
mod timeout_tests;

// Integration tests (requires running KalamDB server):
#[path = "connection/live_connection_tests.rs"]
mod live_connection_tests;
