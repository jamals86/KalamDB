//! Core execution models for SQL handlers (v3)
//!
//! This module consolidates the core types previously defined in
//! `handlers/types.rs` into dedicated files under `executor/models/`.
//!
//! Types:
//! - ExecutionContext
//! - ExecutionResult
//! - ExecutionMetadata
//!
//! Note: Parameters use DataFusion's native `datafusion::scalar::ScalarValue` directly.

mod execution_context;
mod execution_result;
mod execution_metadata;

pub use execution_context::ExecutionContext;
pub use execution_result::ExecutionResult;
pub use execution_metadata::ExecutionMetadata;

// Re-export DataFusion's ScalarValue for convenience
pub use datafusion::scalar::ScalarValue;
