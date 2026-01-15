//! Test driver for miscellaneous integration tests.
//!
//! Run with: cargo test --test test_misc

// Include the common test support
#[path = "../common/testserver/mod.rs"]
mod test_support;

// Include test modules organized by category
mod auth;
mod schema;
mod storage;
mod sql;
mod system;
mod production;
