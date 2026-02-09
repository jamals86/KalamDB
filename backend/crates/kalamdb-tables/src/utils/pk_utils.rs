//! Shared utilities for primary key operations
//!
//! This module provides common utilities used across PK existence checks, filtering,
//! and version resolution to avoid code duplication.

use datafusion::arrow::array::{
    Array, Int16Array, Int32Array, Int64Array, StringArray, UInt16Array, UInt32Array, UInt64Array,
};

/// Extract PK value as string from an Arrow array at given index.
///
/// Supports common PK types: String, Int64, Int32, Int16, UInt64, UInt32, UInt16.
/// Returns None if the value is null or the type is unsupported.
pub fn extract_pk_as_string(col: &dyn Array, idx: usize) -> Option<String> {
    if col.is_null(idx) {
        return None;
    }

    if let Some(arr) = col.as_any().downcast_ref::<StringArray>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<Int64Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<Int32Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<Int16Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<UInt64Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<UInt32Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<UInt16Array>() {
        return Some(arr.value(idx).to_string());
    }

    None
}
