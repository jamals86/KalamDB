//! SQL Tests
//!
//! Tests covering:
//! - SQL command execution
//! - DML parameters
//! - Namespace validation
//! - Naming validation
//! - Quickstart scenarios

#[path = "../../common/testserver/mod.rs"]
#[allow(dead_code)]
pub(super) mod test_support;

// SQL Tests
mod test_dml_parameters_http;
mod test_namespace_validation_http;
mod test_naming_validation_http;
mod test_quickstart_http;
mod test_user_sql_commands_http;
