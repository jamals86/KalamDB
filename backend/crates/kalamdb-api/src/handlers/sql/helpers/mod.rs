//! SQL execution helper functions
//!
//! This module provides helper utilities for SQL execution:
//! - Parameter parsing and validation
//! - File cleanup after errors
//! - Result conversion from Arrow to JSON
//! - Sensitive data masking

mod converter;
mod executor;
mod files;
mod params;

pub use executor::execute_single_statement;
pub use files::cleanup_files;
pub use params::parse_scalar_params;
