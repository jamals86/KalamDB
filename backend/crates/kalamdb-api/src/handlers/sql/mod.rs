//! SQL execution handlers for the `/v1/api/sql` REST API endpoint
//!
//! This module provides HTTP handlers for executing SQL statements via the REST API.
//! Authentication is handled automatically by the `AuthSession` extractor.
//!
//! ## Endpoints
//! - POST /v1/api/sql - Execute SQL statements (requires Authorization header)
//!
//! **Security Note**: This endpoint only accepts Bearer token authentication.
//! Basic auth (username/password) is not supported for SQL execution to encourage
//! secure token-based authentication patterns.

pub mod models;

mod execute;
mod file_utils;
mod forward;
mod helpers;

pub use execute::execute_sql_v1;
pub use file_utils::{
    extract_file_placeholders, extract_table_from_sql, parse_sql_payload,
    stage_and_finalize_files, substitute_file_placeholders,
};
