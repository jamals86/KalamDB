//! Table Tests
//!
//! Tests covering:
//! - User table lifecycle and isolation
//! - Shared table functionality
//! - Stream table operations

// Re-export test_support from parent
pub(super) use super::test_support;

// Table Tests
mod test_shared_tables_http;
mod test_stream_tables_http;
mod test_user_tables_http;
