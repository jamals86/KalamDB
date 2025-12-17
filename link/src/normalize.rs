//! Column-order normalization for query results.
//!
//! This module centralizes column ordering rules so that both the CLI and SDKs
//! (which consume kalam-link) get consistent column orders for known result sets.
//!
//! Note: With the new schema-based format, column ordering is determined by the
//! server and reflected in the schema's index field. This normalization is now
//! primarily for display purposes and doesn't reorder actual row data.

use crate::models::QueryResponse;

/// Canonical order for system.tables information schema
const SYSTEM_TABLES_ORDER: &[&str] = &[
    "table_id",
    "table_name",
    "namespace_id",
    "table_type",
    "created_at",
    "storage_location",
    "storage_id",
    "use_user_storage",
    "flush_policy",
    "schema_version",
    "deleted_retention_hours",
    "access_level",
];

/// Return true if the columns look like the system.tables shape.
fn looks_like_system_tables(columns: &[String]) -> bool {
    // Heuristic: must contain these key identifiers
    ["table_id", "table_name", "namespace_id"]
        .into_iter()
        .all(|k| columns.iter().any(|c| c == k))
}

fn sort_columns(columns: &[String], preferred: &[&str]) -> Vec<String> {
    use std::collections::HashMap;
    let mut order_index = HashMap::new();
    for (i, &name) in preferred.iter().enumerate() {
        order_index.insert(name, i);
    }

    let mut listed: Vec<String> = Vec::new();
    let mut unlisted: Vec<String> = Vec::new();
    for c in columns {
        if order_index.contains_key(c.as_str()) {
            listed.push(c.clone());
        } else {
            unlisted.push(c.clone());
        }
    }
    listed.sort_by_key(|c| order_index.get(c.as_str()).copied().unwrap_or(usize::MAX));
    listed.extend(unlisted);
    listed
}

/// Normalize a QueryResponse's column orders based on the SQL and known schemas.
///
/// Note: With the new array-based row format, rows are already ordered by schema index.
/// This function provides backward compatibility but may be deprecated in favor of
/// server-side ordering via schema.
pub fn normalize_query_response(sql: &str, resp: &mut QueryResponse) {
    for result in &mut resp.results {
        // Get column names from schema
        let cols: Vec<String> = result.column_names();
        if cols.is_empty() {
            continue;
        }

        // Prefer SQL hint, but fall back to column shape heuristic
        let is_system_tables =
            sql.to_lowercase().contains("from system.tables") || looks_like_system_tables(&cols);

        if is_system_tables {
            // Note: With the new format, schema ordering is controlled by the server.
            // Reordering here would require reordering both schema and row arrays.
            // For now, we just log that normalization would be applied.
            // In the future, the server should return properly ordered results.
            let _preferred_order = sort_columns(&cols, SYSTEM_TABLES_ORDER);
            // TODO: If needed, reorder schema and row data to match preferred order
        }
    }
}
