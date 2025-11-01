//! Unified Data Type System
//!
//! This module provides the single source of truth for all data types in KalamDB.
//! It replaces scattered type representations with a unified KalamDataType enum.

pub mod arrow_conversion;
pub mod kalam_data_type;
pub mod wire_format;

pub use arrow_conversion::{ArrowConversionError, FromArrowType, ToArrowType};
pub use kalam_data_type::KalamDataType;
pub use wire_format::{WireFormat, WireFormatError};
