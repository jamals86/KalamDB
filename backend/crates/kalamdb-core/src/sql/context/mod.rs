//! Core execution models for SQL handlers (v3)
//!
//! This module consolidates the core types previously defined in
//! `handlers/types.rs` into dedicated files under `executor/models/`.
//!
//! Types:
//! - ExecutionContext
//! - ExecutionResult
//! - ExecutionMetadata
//! - ErrorResponse
//!
//! Note: Parameters use DataFusion's native `datafusion::scalar::ScalarValue` directly.

mod error_response;
mod execution_context;
mod execution_metadata;
mod execution_result;

pub use error_response::ErrorResponse;
pub use execution_context::ExecutionContext;
pub use execution_metadata::ExecutionMetadata;
pub use execution_result::ExecutionResult;

// Re-export SessionUserContext from kalamdb_session for TableProviders to read user_id and role
pub use kalamdb_session::SessionUserContext;

// Re-export DataFusion's ScalarValue for convenience
pub use datafusion::scalar::ScalarValue;
