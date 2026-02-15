//! Column-order normalization for query results.
//!
//! With the new schema-based format, column ordering is determined by the
//! server and reflected in the schema's index field.  This module is retained
//! as a no-op hook so call-sites compile and future normalization (e.g.
//! display hints) can be added here without touching callers.

use crate::models::QueryResponse;

/// Normalize a QueryResponse's column orders based on the SQL and known schemas.
///
/// Currently a no-op — server-side schema ordering is canonical.
#[inline]
pub fn normalize_query_response(_sql: &str, _resp: &mut QueryResponse) {}
