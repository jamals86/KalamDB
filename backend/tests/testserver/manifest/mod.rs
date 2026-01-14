//! Manifest Tests
//!
//! Tests covering:
//! - Manifest flush operations
//! - Manifest persistence
//! - Manifest metadata management

#[path = "../../common/testserver/mod.rs"]
#[allow(dead_code)]
pub(super) mod test_support;

// Manifest Tests
mod test_manifest_flush_http_v2;
mod test_manifest_persistence_http;
