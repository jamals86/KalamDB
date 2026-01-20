//! Storage and Manifest Tests
//!
//! Tests covering:
//! - Cold storage manifest
//! - Manifest cache operations
//! - Manifest flush integration
//! - RocksDB persistence
//! - Parquet file management

// Re-export test_support from crate root
pub(super) use crate::test_support;

// Test helpers for ignored tests
mod test_helpers;

// Storage Tests
mod test_cold_storage_manifest;
mod test_manifest_cache;
// NOTE: test_manifest_flush_integration requires kalamdb-core's internal test infrastructure
// and is marked #[ignore]. It should be run via kalamdb-core's test suite instead.
// mod test_manifest_flush_integration;
mod test_storage_compact;
