//! Test driver for testserver integration tests.
//!
//! Run with: cargo test --test test_testserver

// Include the common test support
#[path = "../common/testserver/mod.rs"]
mod test_support;

// Include test modules organized by category
mod tables;
mod sql;
mod system;
mod flush;
mod storage;
mod manifest;
mod subscription;
mod observability;
mod stress;
mod cluster;

// Include standalone smoke test
mod test_http_test_server_smoke;
