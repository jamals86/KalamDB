//! Storage and Manifest Tests
//!
//! Tests covering:
//! - Cold storage manifest
//! - Manifest cache operations (moved to kalamdb-core tests)
//! - Manifest flush integration
//! - RocksDB persistence
//! - Parquet file management

// Re-export test_support from crate root
pub(super) use crate::test_support;

// Test helpers for ignored tests
mod test_helpers;

// Storage Tests
mod test_cold_storage_manifest;
mod test_storage_compact;
