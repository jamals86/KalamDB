//! Primary Key Index Helpers
//!
//! Common utilities for PK-based lookups used by UserTableProvider and SharedTableProvider.
//! Extracted to avoid code duplication and centralize optimizations.
//!
//! **Performance Optimizations**:
//! - `pk_exists_in_hot_optimized`: Single index scan with limit=1, no separate exists check
//! - Uses manifest-based pruning for cold storage via `pk_exists_in_cold`

use datafusion::scalar::ScalarValue;

/// Parse an id_value string into the appropriate ScalarValue type
///
/// Tries Int64 first (most common case for PKs), then falls back to Utf8.
/// This avoids allocating a String when the value is numeric.
#[inline]
pub fn parse_pk_value(id_value: &str) -> ScalarValue {
    if let Ok(n) = id_value.parse::<i64>() {
        ScalarValue::Int64(Some(n))
    } else {
        ScalarValue::Utf8(Some(id_value.to_string()))
    }
}
