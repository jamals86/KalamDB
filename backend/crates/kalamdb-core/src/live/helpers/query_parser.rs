//! SQL query parsing utilities for live queries
//!
//! Re-exports the query parser from kalamdb-sql for live query functionality.
//! All SQL parsing logic is centralized in kalamdb-sql to avoid dependency duplication.

use crate::error::KalamDbError;
pub use kalamdb_sql::parser::query_parser::{QueryParseError, QueryParser};

/// Convert QueryParseError to KalamDbError
impl From<QueryParseError> for KalamDbError {
    fn from(err: QueryParseError) -> Self {
        match err {
            QueryParseError::ParseError(msg) => KalamDbError::InvalidSql(msg),
            QueryParseError::InvalidSql(msg) => KalamDbError::InvalidSql(msg),
        }
    }
}
