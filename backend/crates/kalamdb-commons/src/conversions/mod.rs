//! Centralized data type and value conversion utilities
//!
//! This module provides a unified location for all conversion functions used throughout
//! the KalamDB codebase. The goal is to eliminate code duplication and provide a single
//! source of truth for all data type conversions.
//!
//! # Module Organization
//!
//! - `scalar_bytes` - ScalarValue ↔ bytes conversions (for index keys, storage keys)
//! - `scalar_numeric` - ScalarValue ↔ numeric types (f64, i64) conversions
//! - `scalar_string` - ScalarValue ↔ string conversions (for PKs, display)
//! - `scalar_size` - Memory size estimation for ScalarValue types
//!
//! # Usage Examples
//!
//! ```rust,ignore
//! use kalamdb_commons::conversions::{
//!     scalar_value_to_bytes,
//!     scalar_to_f64,
//!     scalar_to_pk_string,
//!     estimate_scalar_value_size,
//! };
//! use datafusion::scalar::ScalarValue;
//!
//! // Convert to bytes for indexing
//! let value = ScalarValue::Int64(Some(12345));
//! let bytes = scalar_value_to_bytes(&value);
//!
//! // Convert to f64 for arithmetic
//! let num = scalar_to_f64(&value).unwrap();
//!
//! // Convert to string for primary keys
//! let pk = scalar_to_pk_string(&value).unwrap();
//!
//! // Estimate memory usage
//! let size = estimate_scalar_value_size(&value);
//! ```

pub mod arrow_conversion;
pub mod arrow_json_conversion;
pub mod scalar_bytes;
pub mod scalar_numeric;
pub mod scalar_size;
pub mod scalar_string;
pub mod schema_metadata;

// Re-export commonly used functions at the module root for convenience
pub use scalar_bytes::{encode_pk_value, scalar_value_to_bytes};
pub use scalar_numeric::{as_f64, scalar_to_f64, scalar_to_i64};
pub use scalar_size::estimate_scalar_value_size;
pub use scalar_string::{parse_string_as_scalar, scalar_to_pk_string};
pub use arrow_json_conversion::*;
pub use schema_metadata::{
	read_kalam_data_type_metadata,
	with_kalam_data_type_metadata,
	KALAM_DATA_TYPE_METADATA_KEY,
};
