//! SQL execution helper functions
//!
//! This module provides helper utilities for SQL execution:
//! - Parameter parsing and validation
//! - File cleanup after errors
//! - Result conversion from Arrow to JSON
//! - Sensitive data masking

mod params;
mod files;
mod executor;
mod converter;

pub use params::parse_scalar_params;
pub use files::cleanup_files;
pub use executor::execute_single_statement;
